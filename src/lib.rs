mod article;
mod config;
mod error;
pub mod images;

use self::error::{ScraperError, ScraperErrorKind};
use crate::article::Article;
use crate::config::{ConfigCollection, GrabberConfig};
use crate::images::ImageDownloader;
use chrono::NaiveDateTime;
use encoding_rs::Encoding;
use failure::ResultExt;
use libxml::parser::Parser;
use libxml::tree::{Document, Node, SaveOptions};
use libxml::xpath::Context;
use log::{debug, error, info, warn};
use regex;
use reqwest;
use std::collections;
use std::error::Error;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::thread;
use url;

pub struct ArticleScraper {
    pub image_downloader: ImageDownloader,
    config_files: Arc<RwLock<Option<ConfigCollection>>>,
    client: reqwest::Client,
}

impl ArticleScraper {
    pub fn new(config_path: PathBuf) -> Result<ArticleScraper, ScraperError> {
        let config_files = Arc::new(RwLock::new(None));

        let locked_config_files = config_files.clone();
        thread::spawn(move || {
            if let Ok(config_files) = GrabberConfig::parse_directory(&config_path) {
                locked_config_files
                    .write()
                    .expect("Failed to lock config file cache")
                    .replace(config_files);
            } else {
                locked_config_files
                    .write()
                    .expect("Failed to lock config file cache")
                    .replace(collections::HashMap::new());
            }
        });

        Ok(ArticleScraper {
            image_downloader: ImageDownloader::new((2048, 2048)),
            config_files,
            client: reqwest::Client::new(),
        })
    }

    pub async fn parse(
        &self,
        url: url::Url,
        download_images: bool,
    ) -> Result<Article, ScraperError> {
        info!("Scraping article: '{}'", url.as_str());
        let response = self
            .client
            .head(url.clone())
            .send()
            .await
            .map_err(|err| {
                error!(
                    "Failed head request to: '{}' - '{}'",
                    url.as_str(),
                    err.description()
                );
                err
            })
            .context(ScraperErrorKind::Http)?;

        // check if url redirects and we need to pick up the new url
        let mut url = url;
        if let Some(new_url) = ArticleScraper::check_redirect(&response) {
            debug!("Url '{}' redirects to '{}'", url.as_str(), new_url.as_str());
            url = new_url;
        }

        // check if we are dealing with text/html
        if !ArticleScraper::check_content_type(&response)? {
            return Err(ScraperErrorKind::ContentType)?;
        }

        // check if we have a config for the url
        let config = self.get_grabber_config(&url)?;

        let mut article = Article {
            title: None,
            author: None,
            url: url.clone(),
            date: None,
            html: None,
        };

        let mut document = Document::new().map_err(|()| ScraperErrorKind::Xml)?;

        let mut root = Node::new("article", None, &document).map_err(|()| ScraperErrorKind::Xml)?;

        document.set_root_element(&root);

        ArticleScraper::generate_head(&mut root, &document)?;

        self.parse_pages(&mut article, &url, &mut root, &config)
            .await?;

        let context = Context::new(&document).map_err(|()| {
            error!("Failed to create xpath context for extracted article");
            ScraperErrorKind::Xml
        })?;

        if let Err(error) = ArticleScraper::prevent_self_closing_tags(&context) {
            error!("Preventing self closing tags failed - '{}'", error);
            return Err(error);
        }

        if let Err(error) = ArticleScraper::eliminate_noscrip_tag(&context) {
            error!("Eliminating <noscript> tag failed - '{}'", error);
            return Err(error);
        }

        if download_images {
            if let Err(error) = self
                .image_downloader
                .download_images_from_context(&context)
                .await
            {
                error!("Downloading images failed: '{}'", error);
            }
        }

        // serialize content
        let options = SaveOptions {
            format: false,
            no_declaration: false,
            no_empty_tags: true,
            no_xhtml: false,
            xhtml: false,
            as_xml: false,
            as_html: true,
            non_significant_whitespace: false,
        };
        let html = document.to_string_with_options(options);
        article.html = Some(html);

        Ok(article)
    }

    async fn parse_pages(
        &self,
        article: &mut Article,
        url: &url::Url,
        root: &mut Node,
        config: &GrabberConfig,
    ) -> Result<(), ScraperError> {
        let html = ArticleScraper::download(&url, &self.client).await?;
        let mut document = Self::parse_html(html, config)?;
        let mut xpath_ctx = Self::get_xpath_ctx(&document)?;

        // check for single page link
        if let Some(xpath_single_page_link) = config.single_page_link.clone() {
            debug!(
                "Single page link xpath specified in config '{}'",
                xpath_single_page_link
            );
            if let Ok(result) = xpath_ctx.findvalue(&xpath_single_page_link, None) {
                if !result.trim().is_empty() {
                    // parse again with single page url
                    debug!("Single page link found '{}'", result);
                    let single_page_url = url::Url::parse(&result).context(ScraperErrorKind::Url)?;
                    return self
                        .parse_single_page(article, &single_page_url, root, config)
                        .await;
                }
            }
        }

        ArticleScraper::extract_metadata(&xpath_ctx, config, article);
        ArticleScraper::strip_junk(&xpath_ctx, config, &url);
        ArticleScraper::extract_body(&xpath_ctx, root, config)?;

        loop {
            if let Some(url) = self.check_for_next_page(&xpath_ctx, config) {
                let html = ArticleScraper::download(&url, &self.client).await?;
                document = Self::parse_html(html, config)?;
                xpath_ctx = Self::get_xpath_ctx(&document)?;
                ArticleScraper::strip_junk(&xpath_ctx, config, &url);
                ArticleScraper::extract_body(&xpath_ctx, root, config)?;
            } else {
                break;
            }
        }

        Ok(())
    }

    fn parse_html(html: String, config: &GrabberConfig) -> Result<Document, ScraperError> {
        // replace matches in raw html

        let mut html = html;
        for replace in &config.replace {
            html = html.replace(&replace.to_replace, &replace.replace_with);
        }

        // parse html
        let parser = Parser::default_html();
        Ok(parser.parse_string(html.as_str()).map_err(|err| {
            error!("Parsing HTML failed for downloaded HTML {:?}", err);
            ScraperErrorKind::Xml
        })?)
    }

    fn get_xpath_ctx(doc: &Document) -> Result<Context, ScraperError> {
        Ok(Context::new(&doc).map_err(|()| {
            error!("Creating xpath context failed for downloaded HTML");
            ScraperErrorKind::Xml
        })?)
    }

    pub fn evaluate_xpath(
        xpath_ctx: &Context,
        xpath: &str,
        thorw_if_empty: bool,
    ) -> Result<Vec<Node>, ScraperError> {
        let res = xpath_ctx.evaluate(xpath).map_err(|()| {
            error!("Evaluation of xpath '{}' yielded no results", xpath);
            ScraperErrorKind::Xml
        })?;

        let node_vec = res.get_nodes_as_vec();

        if node_vec.len() == 0 {
            error!("Evaluation of xpath '{}' yielded no results", xpath);
            if thorw_if_empty {
                return Err(ScraperErrorKind::Xml)?;
            }
        }

        Ok(node_vec)
    }

    async fn parse_single_page(
        &self,
        article: &mut Article,
        url: &url::Url,
        root: &mut Node,
        config: &GrabberConfig,
    ) -> Result<(), ScraperError> {
        let html = ArticleScraper::download(&url, &self.client).await?;
        let document = Self::parse_html(html, config)?;
        let xpath_ctx = Self::get_xpath_ctx(&document)?;
        ArticleScraper::extract_metadata(&xpath_ctx, config, article);
        ArticleScraper::strip_junk(&xpath_ctx, config, &url);
        ArticleScraper::extract_body(&xpath_ctx, root, config)?;

        Ok(())
    }

    async fn download(url: &url::Url, client: &reqwest::Client) -> Result<String, ScraperError> {
        let response = client
            .get(url.as_str())
            .send()
            .await
            .map_err(|err| {
                error!(
                    "Downloading HTML failed: GET '{}' - '{}'",
                    url.as_str(),
                    err.description()
                );
                err
            })
            .context(ScraperErrorKind::Http)?;

        if response.status().is_success() {
            let headers = response.headers().clone();
            let text = response.text().await.context(ScraperErrorKind::Http)?;
            {
                if let Some(decoded_html) = ArticleScraper::decode_html(
                    &text,
                    ArticleScraper::get_encoding_from_html(&text),
                ) {
                    return Ok(decoded_html);
                }

                if let Some(decoded_html) = ArticleScraper::decode_html(
                    &text,
                    ArticleScraper::get_encoding_from_http_header(&headers),
                ) {
                    return Ok(decoded_html);
                }
            }

            warn!("No encoding of HTML detected - assuming utf-8");
            return Ok(text);
        }

        Err(ScraperErrorKind::Http)?
    }

    fn get_encoding_from_http_header(headers: &reqwest::header::HeaderMap) -> Option<&str> {
        if let Some(content_type) = headers.get(reqwest::header::CONTENT_TYPE) {
            if let Ok(content_type) = content_type.to_str() {
                let regex = regex::Regex::new(r#"charset=([^"']+)"#).unwrap();
                if let Some(captures) = regex.captures(content_type) {
                    if let Some(regex_match) = captures.get(1) {
                        return Some(regex_match.as_str());
                    }
                }
            }
        }
        None
    }

    fn get_encoding_from_html(html: &str) -> Option<&str> {
        let regex = regex::Regex::new(r#"<meta.*?charset=([^"']+)"#).unwrap();
        if let Some(captures) = regex.captures(html) {
            if let Some(regex_match) = captures.get(1) {
                return Some(regex_match.as_str());
            }
        }
        None
    }

    fn decode_html(html: &str, encoding: Option<&str>) -> Option<String> {
        if let Some(encoding) = encoding {
            if let Some(encoding) = Encoding::for_label(encoding.as_bytes()) {
                let (decoded_html, _, invalid_chars) = encoding.decode(html.as_bytes());

                if !invalid_chars {
                    return Some(decoded_html.into_owned());
                }
            }
            warn!("Could not decode HTML. Encoding: '{}'", encoding);
        }
        None
    }

    fn get_grabber_config(&self, url: &url::Url) -> Result<GrabberConfig, ScraperError> {
        let config_name = match url.host_str() {
            Some(name) => {
                let mut name = name;
                if name.starts_with("www.") {
                    name = &name[4..]
                }
                name
            }
            None => {
                error!("Getting config failed due to bad Url");
                return Err(ScraperErrorKind::Config)?;
            }
        };

        let config_name = config_name.to_owned() + ".txt";

        if let Some(config_files) = &*self.config_files.read().unwrap() {
            match config_files.get(&config_name) {
                Some(config) => return Ok(config.clone()),
                None => {
                    error!("No config file of the name '{}' fount", config_name);
                    Err(ScraperErrorKind::Config)?
                }
            }
        } else {
            error!("Config files have not been parsed yet.");
            return Err(ScraperErrorKind::Config)?;
        }
    }

    fn check_content_type(response: &reqwest::Response) -> Result<bool, ScraperError> {
        if response.status().is_success() {
            if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
                if let Ok(content_type) = content_type.to_str() {
                    if content_type.contains("text/html") {
                        return Ok(true);
                    }
                }
            }

            error!("Content type is not text/HTML");
            return Ok(false);
        }

        error!("Failed to determine content type");
        Err(ScraperErrorKind::Http)?
    }

    fn check_redirect(response: &reqwest::Response) -> Option<url::Url> {
        if response.status() == reqwest::StatusCode::PERMANENT_REDIRECT {
            debug!("Article url redirects to '{}'", response.url().as_str());
            return Some(response.url().clone());
        }

        None
    }

    fn extract_value(context: &Context, xpath: &str) -> Result<String, ScraperError> {
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;
        if let Some(val) = node_vec.get(0) {
            return Ok(val.get_content());
        }

        Err(ScraperErrorKind::Xml)?
    }

    fn extract_value_merge(context: &Context, xpath: &str) -> Result<String, ScraperError> {
        let node_vec = Self::evaluate_xpath(context, xpath, true)?;
        let mut val = String::new();
        for node in node_vec {
            val.push_str(&node.get_content());
        }

        return Ok(val.trim().to_string());
    }

    fn strip_node(context: &Context, xpath: &String) -> Result<(), ScraperError> {
        let mut ancestor = xpath.clone();
        if ancestor.starts_with("//") {
            ancestor = ancestor.chars().skip(2).collect();
        }

        let query = &format!("{}[not(ancestor::{})]", xpath, ancestor);
        let node_vec = Self::evaluate_xpath(context, query, false)?;
        for mut node in node_vec {
            node.unlink();
        }
        Ok(())
    }

    fn strip_id_or_class(context: &Context, id_or_class: &String) -> Result<(), ScraperError> {
        let xpath = &format!(
            "//*[contains(@class, '{}') or contains(@id, '{}')]",
            id_or_class, id_or_class
        );

        let mut ancestor = xpath.clone();
        if ancestor.starts_with("//") {
            ancestor = ancestor.chars().skip(2).collect();
        }

        let query = &format!("{}[not(ancestor::{})]", xpath, ancestor);
        let node_vec = Self::evaluate_xpath(context, query, false)?;
        for mut node in node_vec {
            node.unlink();
        }
        Ok(())
    }

    fn fix_lazy_images(
        context: &Context,
        class: &str,
        property_url: &str,
    ) -> Result<(), ScraperError> {
        let xpath = &format!("//img[contains(@class, '{}')]", class);
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Some(correct_url) = node.get_property(property_url) {
                if let Err(_) = node.set_property("src", &correct_url) {
                    return Err(ScraperErrorKind::Xml)?;
                }
            }
        }
        Ok(())
    }

    fn fix_iframe_size(context: &Context, site_name: &str) -> Result<(), ScraperError> {
        let xpath = &format!("//iframe[contains(@src, '{}')]", site_name);
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Some(mut parent) = node.get_parent() {
                if let Ok(mut video_wrapper) = parent.new_child(None, "div") {
                    if let Ok(()) = video_wrapper.set_property("class", "videoWrapper") {
                        if let Ok(()) = node.set_property("width", "100%") {
                            if let Ok(()) = node.remove_property("height") {
                                node.unlink();
                                video_wrapper.add_child(&mut node).map_err(|_| {
                                    error!("Failed to add iframe as child of video wrapper <div>");
                                    ScraperErrorKind::Xml
                                })?;
                            }
                        }
                    }
                }

                error!("Failed to add video wrapper <div> as parent of iframe");
                return Err(ScraperErrorKind::Xml)?;
            }

            error!("Failed to get parent of iframe");
            return Err(ScraperErrorKind::Xml)?;
        }
        Ok(())
    }

    fn remove_attribute(
        context: &Context,
        tag: Option<&str>,
        attribute: &str,
    ) -> Result<(), ScraperError> {
        let xpath_tag = match tag {
            Some(tag) => tag,
            None => "*",
        };

        let xpath = &format!("//{}[@{}]", xpath_tag, attribute);
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Err(_) = node.remove_property(attribute) {
                return Err(ScraperErrorKind::Xml)?;
            }
        }
        Ok(())
    }

    fn add_attribute(
        context: &Context,
        tag: Option<&str>,
        attribute: &str,
        value: &str,
    ) -> Result<(), ScraperError> {
        let xpath_tag = match tag {
            Some(tag) => tag,
            None => "*",
        };

        let xpath = &format!("//{}", xpath_tag);
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Err(_) = node.set_attribute(attribute, value) {
                return Err(ScraperErrorKind::Xml)?;
            }
        }
        Ok(())
    }

    fn get_attribute(
        context: &Context,
        xpath: &str,
        attribute: &str,
    ) -> Result<String, ScraperError> {
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;
        for node in node_vec {
            if let Some(value) = node.get_attribute(attribute) {
                return Ok(value);
            }
        }

        Err(ScraperErrorKind::Xml)?
    }

    fn repair_urls(
        context: &Context,
        xpath: &str,
        attribute: &str,
        article_url: &url::Url,
    ) -> Result<(), ScraperError> {
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Some(val) = node.get_attribute(attribute) {
                if let Err(url::ParseError::RelativeUrlWithoutBase) = url::Url::parse(&val) {
                    if let Ok(fixed_url) = ArticleScraper::complete_url(article_url, &val) {
                        if let Err(_) = node.set_attribute(attribute, fixed_url.as_str()) {
                            return Err(ScraperErrorKind::Xml)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn complete_url(
        article_url: &url::Url,
        incomplete_url: &str,
    ) -> Result<url::Url, ScraperError> {
        let mut completed_url = article_url.scheme().to_owned();
        completed_url.push(':');

        if !incomplete_url.starts_with("//") {
            match article_url.host() {
                Some(url::Host::Domain(host)) => {
                    completed_url.push_str("//");
                    completed_url.push_str(host);
                }
                _ => return Err(ScraperErrorKind::Url)?,
            };
        }

        if !completed_url.ends_with('/') && !incomplete_url.starts_with('/') {
            completed_url.push_str("/");
        }
        completed_url.push_str(incomplete_url);
        let url = url::Url::parse(&completed_url).context(ScraperErrorKind::Url)?;
        return Ok(url);
    }

    fn strip_junk(context: &Context, config: &GrabberConfig, url: &url::Url) {
        // strip specified xpath
        for xpath_strip in &config.xpath_strip {
            let _ = ArticleScraper::strip_node(&context, xpath_strip);
        }

        // strip everything with specified 'id' or 'class'
        for xpaht_strip_class in &config.strip_id_or_class {
            let _ = ArticleScraper::strip_id_or_class(&context, xpaht_strip_class);
        }

        // strip any <img> element where @src attribute contains this substring
        for xpath_strip_img_src in &config.strip_image_src {
            let _ = ArticleScraper::strip_node(
                &context,
                &format!("//img[contains(@src,'{}')]", xpath_strip_img_src),
            );
        }

        let _ = ArticleScraper::fix_lazy_images(&context, "lazyload", "data-src");
        let _ = ArticleScraper::fix_iframe_size(&context, "youtube.com");
        let _ = ArticleScraper::remove_attribute(&context, None, "style");
        let _ = ArticleScraper::remove_attribute(&context, Some("a"), "onclick");
        let _ = ArticleScraper::remove_attribute(&context, Some("img"), "srcset");
        let _ = ArticleScraper::remove_attribute(&context, Some("img"), "sizes");
        let _ = ArticleScraper::add_attribute(&context, Some("a"), "target", "_blank");

        let _ = ArticleScraper::repair_urls(&context, "//img", "src", &url);
        let _ = ArticleScraper::repair_urls(&context, "//a", "src", &url);
        let _ = ArticleScraper::repair_urls(&context, "//a", "href", &url);
        let _ = ArticleScraper::repair_urls(&context, "//object", "data", &url);
        let _ = ArticleScraper::repair_urls(&context, "//iframe", "src", &url);

        // strip elements using Readability.com and Instapaper.com ignore class names
        // .entry-unrelated and .instapaper_ignore
        // See http://blog.instapaper.com/post/730281947
        let _ = ArticleScraper::strip_node(&context, &String::from(
            "//*[contains(@class,' entry-unrelated ') or contains(@class,' instapaper_ignore ')]"));

        // strip elements that contain style="display: none;"
        let _ = ArticleScraper::strip_node(
            &context,
            &String::from("//*[contains(@style,'display:none')]"),
        );

        // strip all scripts
        let _ = ArticleScraper::strip_node(&context, &String::from("//script"));

        // strip all comments
        let _ = ArticleScraper::strip_node(&context, &String::from("//comment()"));

        // strip all empty url-tags <a/>
        let _ = ArticleScraper::strip_node(&context, &String::from("//a[not(node())]"));

        // strip all external css and fonts
        let _ = ArticleScraper::strip_node(&context, &String::from("//*[@type='text/css']"));
    }

    fn extract_metadata(context: &Context, config: &GrabberConfig, article: &mut Article) {
        // try to get title
        for xpath_title in &config.xpath_title {
            if let Ok(title) = ArticleScraper::extract_value_merge(&context, xpath_title) {
                debug!("Article title: '{}'", title);
                article.title = Some(title);
                break;
            }
        }

        // try to get the author
        for xpath_author in &config.xpath_author {
            if let Ok(author) = ArticleScraper::extract_value(&context, xpath_author) {
                debug!("Article author: '{}'", author);
                article.author = Some(author);
                break;
            }
        }

        // try to get the date
        for xpath_date in &config.xpath_date {
            if let Ok(date_string) = ArticleScraper::extract_value(&context, xpath_date) {
                debug!("Article date: '{}'", date_string);
                if let Ok(date) = NaiveDateTime::from_str(&date_string) {
                    article.date = Some(date);
                    break;
                } else {
                    warn!("Parsing the date string '{}' failed", date_string);
                }
            }
        }
    }

    fn extract_body(
        context: &Context,
        root: &mut Node,
        config: &GrabberConfig,
    ) -> Result<(), ScraperError> {
        let mut found_something = false;
        for xpath_body in &config.xpath_body {
            found_something = ArticleScraper::extract_body_single(&context, root, xpath_body)?;
        }

        if !found_something {
            return Err(ScraperErrorKind::Scrape)?;
        }

        Ok(())
    }

    fn extract_body_single(
        context: &Context,
        root: &mut Node,
        xpath: &str,
    ) -> Result<bool, ScraperError> {
        let mut found_something = false;
        {
            let node_vec = Self::evaluate_xpath(context, xpath, false)?;
            for mut node in node_vec {
                if node.get_property("style").is_some() {
                    if let Err(_) = node.remove_property("style") {
                        return Err(ScraperErrorKind::Xml)?;
                    }
                }

                node.unlink();
                if let Ok(_) = root.add_child(&mut node) {
                    found_something = true;
                } else {
                    error!("Failed to add body to prepared document");
                    return Err(ScraperErrorKind::Xml)?;
                }
            }
        }

        Ok(found_something)
    }

    fn check_for_next_page(&self, context: &Context, config: &GrabberConfig) -> Option<url::Url> {
        if let Some(next_page_xpath) = config.next_page_link.clone() {
            if let Ok(next_page_string) =
                ArticleScraper::get_attribute(&context, &next_page_xpath, "href")
            {
                if let Ok(next_page_url) = url::Url::parse(&next_page_string) {
                    return Some(next_page_url);
                }
            }
        }

        // last page reached
        None
    }

    fn generate_head(root: &mut Node, document: &Document) -> Result<(), ScraperError> {
        if let Ok(mut head_node) = Node::new("head", None, document) {
            if let Ok(()) = root.add_prev_sibling(&mut head_node) {
                if let Ok(mut meta) = head_node.new_child(None, "meta") {
                    if let Ok(_) = meta.set_property("charset", "utf-8") {
                        return Ok(());
                    }
                }
            }
        }

        Err(ScraperErrorKind::Xml)?
    }

    fn prevent_self_closing_tags(context: &Context) -> Result<(), ScraperError> {
        // search document for empty tags and add a empty text node as child
        // this prevents libxml from self closing non void elements such as iframe

        let xpath = "//*[not(node())]";
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if node.get_name() == "meta" {
                continue;
            }

            let _ = node.add_text_child(None, "empty", "");
        }

        Ok(())
    }

    fn eliminate_noscrip_tag(context: &Context) -> Result<(), ScraperError> {
        let xpath = "//noscript";
        let node_vec = Self::evaluate_xpath(context, xpath, false)?;

        for mut node in node_vec {
            if let Some(mut parent) = node.get_parent() {
                node.unlink();
                let children = node.get_child_nodes();
                for mut child in children {
                    child.unlink();
                    let _ = parent.add_child(&mut child);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[tokio::test(basic_scheduler)]
    async fn golem() {
        let config_path = PathBuf::from(r"./resources/tests/golem");
        let out_path = PathBuf::from(r"./test_output");
        let url = url::Url::parse("https://www.golem.de/news/http-error-418-fehlercode-ich-bin-eine-teekanne-darf-bleiben-1708-129460.html").unwrap();

        let grabber = ArticleScraper::new(config_path).unwrap();
        let article = grabber.parse(url, true).await.unwrap();
        article.save_html(&out_path).unwrap();

        assert_eq!(
            article.title,
            Some(String::from(
                "HTTP Error 418: Fehlercode \"Ich bin eine Teekanne\" darf bleiben"
            ))
        );
        assert_eq!(article.author, Some(String::from("Hauke Gierow")));
    }

    #[tokio::test(basic_scheduler)]
    async fn phoronix() {
        let config_path = PathBuf::from(r"./resources/tests/phoronix");
        let out_path = PathBuf::from(r"./test_output");
        let url = url::Url::parse(
            "http://www.phoronix.com/scan.php?page=article&item=amazon_ec2_bare&num=1",
        )
        .unwrap();

        let grabber = ArticleScraper::new(config_path).unwrap();
        let article = grabber.parse(url, true).await.unwrap();
        article.save_html(&out_path).unwrap();

        assert_eq!(
            article.title,
            Some(String::from(
                "Amazon EC2 Cloud Benchmarks Against Bare Metal Systems"
            ))
        );
    }
}
