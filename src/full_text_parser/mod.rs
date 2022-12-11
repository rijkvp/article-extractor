pub mod config;
pub mod error;
mod fingerprints;

#[cfg(test)]
mod tests;

use self::config::{ConfigCollection, ConfigEntry};
use self::error::FullTextParserError;
use crate::article::Article;
use crate::util::Util;
use chrono::DateTime;
use encoding_rs::Encoding;
use fingerprints::Fingerprints;
use libxml::parser::Parser;
use libxml::tree::{Document, Node, SaveOptions};
use libxml::xpath::Context;
use log::{debug, error, info, warn};
use reqwest::header::HeaderMap;
use reqwest::Client;
use std::path::Path;
use std::str::FromStr;

pub struct FullTextParser {
    config_files: ConfigCollection,
}

impl FullTextParser {
    pub async fn new(config_path: Option<&Path>) -> Self {
        let config_files = ConfigCollection::parse(config_path).await;
        Self { config_files }
    }

    pub async fn parse(
        &self,
        url: &url::Url,
        client: &Client,
    ) -> Result<Article, FullTextParserError> {
        info!("Scraping article: '{}'", url.as_str());

        // check if we have a config for the url
        let config = self.get_grabber_config(url);
        let global_config = self
            .config_files
            .get("global.txt")
            .ok_or(FullTextParserError::Config)?;

        let headers = Util::generate_headers(config, global_config)?;

        let response = client
            .head(url.clone())
            .headers(headers)
            .send()
            .await
            .map_err(|err| {
                error!("Failed head request to: '{}' - '{}'", url.as_str(), err);
                FullTextParserError::Http
            })?;

        // check if url redirects and we need to pick up the new url
        let url = if let Some(new_url) = Util::check_redirect(&response, url) {
            debug!("Url '{}' redirects to '{}'", url.as_str(), new_url.as_str());
            new_url
        } else {
            url.clone()
        };

        // check if we are dealing with text/html
        if !Util::check_content_type(&response)? {
            return Err(FullTextParserError::ContentType);
        }

        let mut article = Article {
            title: None,
            author: None,
            url: url.clone(),
            date: None,
            html: None,
            thumbnail_url: None,
        };

        let mut document = Document::new().map_err(|()| FullTextParserError::Xml)?;
        let mut root =
            Node::new("article", None, &document).map_err(|()| FullTextParserError::Xml)?;
        document.set_root_element(&root);

        Self::generate_head(&mut root, &document)?;

        self.parse_pages(&mut article, &url, &mut root, config, global_config, client)
            .await?;

        let context = Context::new(&document).map_err(|()| {
            error!("Failed to create xpath context for extracted article");
            FullTextParserError::Xml
        })?;

        if let Err(error) = Self::prevent_self_closing_tags(&context) {
            error!("Preventing self closing tags failed - '{}'", error);
            return Err(error);
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
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
        client: &Client,
    ) -> Result<(), FullTextParserError> {
        let headers = Util::generate_headers(config, global_config)?;
        let html = Self::download(url, client, headers).await?;

        // see if
        let config = if config.is_none() {
            if let Some(url) = Fingerprints::detect(&html) {
                self.get_grabber_config(&url)
            } else {
                config
            }
        } else {
            config
        };

        let mut document = Self::parse_html(html, config, global_config)?;
        let mut xpath_ctx = Self::get_xpath_ctx(&document)?;

        // check for single page link
        let rule = Util::select_rule(
            config.and_then(|c| c.single_page_link.as_deref()),
            global_config.single_page_link.as_deref(),
        );
        if let Some(xpath_single_page_link) = rule {
            debug!(
                "Single page link xpath specified in config '{}'",
                xpath_single_page_link
            );

            if let Some(single_page_url) = Util::find_page_url(&xpath_ctx, xpath_single_page_link) {
                // parse again with single page url
                debug!("Single page link found '{}'", single_page_url);

                return self
                    .parse_single_page(
                        article,
                        &single_page_url,
                        root,
                        config,
                        global_config,
                        client,
                    )
                    .await;
            }
        }

        Self::extract_metadata(&xpath_ctx, config, global_config, article);
        if article.thumbnail_url.is_none() {
            Self::check_for_thumbnail(&xpath_ctx, article);
        }
        Self::strip_junk(&xpath_ctx, config, global_config, url);
        Self::extract_body(&xpath_ctx, root, config, global_config)?;

        while let Some(url) = self.check_for_next_page(&xpath_ctx, config, global_config) {
            let headers = Util::generate_headers(config, global_config)?;
            let html = Self::download(&url, client, headers).await?;
            document = Self::parse_html(html, config, global_config)?;
            xpath_ctx = Self::get_xpath_ctx(&document)?;
            Self::strip_junk(&xpath_ctx, config, global_config, &url);
            Self::extract_body(&xpath_ctx, root, config, global_config)?;
        }

        Ok(())
    }

    fn parse_html(
        html: String,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
    ) -> Result<Document, FullTextParserError> {
        // replace matches in raw html

        let mut html = html;
        if let Some(config) = config {
            for replace in &config.replace {
                html = html.replace(&replace.to_replace, &replace.replace_with);
            }
        }

        for replace in &global_config.replace {
            html = html.replace(&replace.to_replace, &replace.replace_with);
        }

        // parse html
        let parser = Parser::default_html();
        Ok(parser.parse_string(html.as_str()).map_err(|err| {
            error!("Parsing HTML failed for downloaded HTML {:?}", err);
            FullTextParserError::Xml
        })?)
    }

    fn get_xpath_ctx(doc: &Document) -> Result<Context, FullTextParserError> {
        Ok(Context::new(doc).map_err(|()| {
            error!("Creating xpath context failed for downloaded HTML");
            FullTextParserError::Xml
        })?)
    }

    async fn parse_single_page(
        &self,
        article: &mut Article,
        url: &url::Url,
        root: &mut Node,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
        client: &Client,
    ) -> Result<(), FullTextParserError> {
        let headers = Util::generate_headers(config, global_config)?;
        let html = Self::download(url, client, headers).await?;
        let document = Self::parse_html(html, config, global_config)?;
        let xpath_ctx = Self::get_xpath_ctx(&document)?;
        Self::extract_metadata(&xpath_ctx, config, global_config, article);
        Self::check_for_thumbnail(&xpath_ctx, article);
        Self::strip_junk(&xpath_ctx, config, global_config, url);
        Self::extract_body(&xpath_ctx, root, config, global_config)?;

        Ok(())
    }

    async fn download(
        url: &url::Url,
        client: &Client,
        headers: HeaderMap,
    ) -> Result<String, FullTextParserError> {
        let response = client
            .get(url.as_str())
            .headers(headers)
            .send()
            .await
            .map_err(|err| {
                error!(
                    "Downloading HTML failed: GET '{}' - '{}'",
                    url.as_str(),
                    err
                );
                FullTextParserError::Http
            })?;

        if response.status().is_success() {
            let headers = response.headers().clone();
            let text = response
                .text()
                .await
                .map_err(|_| FullTextParserError::Http)?;

            if let Some(decoded_html) =
                Self::decode_html(&text, Self::get_encoding_from_html(&text))
            {
                return Ok(decoded_html);
            }

            if let Some(decoded_html) =
                Self::decode_html(&text, Self::get_encoding_from_http_header(&headers))
            {
                return Ok(decoded_html);
            }

            warn!("No encoding of HTML detected - assuming utf-8");
            return Ok(text);
        }

        Err(FullTextParserError::Http)
    }

    fn get_encoding_from_http_header(headers: &reqwest::header::HeaderMap) -> Option<&str> {
        headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|header| header.to_str().ok())
            .and_then(|content_type| {
                regex::Regex::new(r#"charset=([^"']+)"#)
                    .expect("Failed to parse regex")
                    .captures(content_type)
            })
            .and_then(|captures| captures.get(1))
            .map(|regex_match| regex_match.as_str())
    }

    fn get_encoding_from_html(html: &str) -> Option<&str> {
        let regex =
            regex::Regex::new(r#"<meta.*?charset=([^"']+)"#).expect("Failed to parse regex");
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

    fn get_host_name(url: &url::Url) -> Result<String, FullTextParserError> {
        match url.host_str() {
            Some(name) => {
                let mut name = name;
                if name.starts_with("www.") && name.len() > 4 {
                    name = &name[4..]
                }
                Ok(name.into())
            }
            None => {
                error!("Getting config failed due to bad Url");
                Err(FullTextParserError::Config)
            }
        }
    }

    fn get_grabber_config(&self, url: &url::Url) -> Option<&ConfigEntry> {
        let conf = Self::get_host_name(url)
            .ok()
            .map(|url| url + ".txt")
            .and_then(|name| self.config_files.get(&name));

        if conf.is_none() {
            log::warn!("No config found for url '{}'", url);
        }

        conf
    }

    fn check_for_thumbnail(context: &Context, article: &mut Article) {
        if let Some(thumb) = Self::get_attribute(context, "//meta[contains(@name, 'twitter:image')]", "content").ok() {
            article.thumbnail_url = Some(thumb);
            return
        }

        if let Some(thumb) = Self::get_attribute(context, "//meta[contains(@name, 'og:image')]", "content").ok() {
            article.thumbnail_url = Some(thumb);
            return
        }

        if let Some(thumb) = Self::get_attribute(context, "//link[contains(@rel, 'image_src')]", "href").ok() {
            article.thumbnail_url = Some(thumb);
            return
        }
    }

    fn fix_lazy_images(
        context: &Context,
        class: &str,
        property_url: &str,
    ) -> Result<(), FullTextParserError> {
        let xpath = &format!("//img[contains(@class, '{}')]", class);
        let node_vec = Util::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if node
                .get_property(property_url)
                .and_then(|correct_url| node.set_property("src", &correct_url).ok())
                .is_none()
            {
                warn!("Failed to fix lazy loading image");
            }
        }
        Ok(())
    }

    fn fix_iframe_size(context: &Context, site_name: &str) -> Result<(), FullTextParserError> {
        let xpath = &format!("//iframe[contains(@src, '{}')]", site_name);
        let node_vec = Util::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            let video_wrapper = node
                .get_parent()
                .and_then(|mut parent| parent.new_child(None, "div").ok());
            if let Some(mut video_wrapper) = video_wrapper {
                let success = video_wrapper
                    .set_property("class", "videoWrapper")
                    .ok()
                    .and_then(|()| node.set_property("width", "100%").ok())
                    .and_then(|()| node.set_property("height", "100%").ok())
                    .ok_or_else(|| {
                        node.unlink();
                        video_wrapper.add_child(&mut node)
                    })
                    .is_err();
                if !success {
                    warn!("Failed to add iframe as child of video wrapper <div>");
                }
            } else {
                warn!("Failed to get parent of iframe");
            }
        }
        Ok(())
    }

    fn remove_attribute(
        context: &Context,
        tag: Option<&str>,
        attribute: &str,
    ) -> Result<(), FullTextParserError> {
        let xpath_tag = tag.unwrap_or("*");

        let xpath = &format!("//{}[@{}]", xpath_tag, attribute);
        let node_vec = Util::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Err(err) = node.remove_property(attribute) {
                log::warn!(
                    "Failed to remove attribute '{}' from node: {}",
                    attribute,
                    err
                );
            }
        }
        Ok(())
    }

    fn add_attribute(
        context: &Context,
        tag: Option<&str>,
        attribute: &str,
        value: &str,
    ) -> Result<(), FullTextParserError> {
        let xpath_tag = tag.unwrap_or("*");

        let xpath = &format!("//{}", xpath_tag);
        let node_vec = Util::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Err(err) = node.set_attribute(attribute, value) {
                log::warn!("Failed to set attribute '{}' on node: {}", attribute, err);
            }
        }
        Ok(())
    }

    fn get_attribute(
        context: &Context,
        xpath: &str,
        attribute: &str,
    ) -> Result<String, FullTextParserError> {
        Util::evaluate_xpath(context, xpath, false)?
            .iter()
            .find_map(|node| node.get_attribute(attribute))
            .ok_or(FullTextParserError::Xml)
    }

    fn repair_urls(
        context: &Context,
        xpath: &str,
        attribute: &str,
        article_url: &url::Url,
    ) -> Result<(), FullTextParserError> {
        let node_vec = Util::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Some(url) = node.get_attribute(attribute) {
                let is_relative_url = url::Url::parse(&url)
                    .err()
                    .map(|err| err == url::ParseError::RelativeUrlWithoutBase)
                    .unwrap_or(false);

                if is_relative_url {
                    let completed_url = Self::complete_url(article_url, &url)?;
                    let _ = node
                        .set_attribute(attribute, completed_url.as_str())
                        .map_err(|_| FullTextParserError::Scrape)?;
                }
            }
        }
        Ok(())
    }

    fn complete_url(
        article_url: &url::Url,
        incomplete_url: &str,
    ) -> Result<url::Url, FullTextParserError> {
        let mut completed_url = article_url.scheme().to_owned();
        completed_url.push(':');

        if !incomplete_url.starts_with("//") {
            match article_url.host() {
                Some(url::Host::Domain(host)) => {
                    completed_url.push_str("//");
                    completed_url.push_str(host);
                }
                _ => return Err(FullTextParserError::Scrape),
            };
        }

        if !completed_url.ends_with('/') && !incomplete_url.starts_with('/') {
            completed_url.push('/');
        }
        completed_url.push_str(incomplete_url);
        let url = url::Url::parse(&completed_url)?;
        Ok(url)
    }

    fn strip_junk(
        context: &Context,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
        url: &url::Url,
    ) {
        // strip specified xpath
        if let Some(config) = config {
            for xpath_strip in &config.xpath_strip {
                let _ = Util::strip_node(context, xpath_strip);
            }
        }

        for xpath_strip in &global_config.xpath_strip {
            let _ = Util::strip_node(context, xpath_strip);
        }

        // strip everything with specified 'id' or 'class'
        if let Some(config) = config {
            for xpaht_strip_class in &config.strip_id_or_class {
                let _ = Util::strip_id_or_class(context, xpaht_strip_class);
            }
        }

        for xpaht_strip_class in &global_config.strip_id_or_class {
            let _ = Util::strip_id_or_class(context, xpaht_strip_class);
        }

        // strip any <img> element where @src attribute contains this substring
        if let Some(config) = config {
            for xpath_strip_img_src in &config.strip_image_src {
                let _ = Util::strip_node(
                    context,
                    &format!("//img[contains(@src,'{}')]", xpath_strip_img_src),
                );
            }
        }

        for xpath_strip_img_src in &global_config.strip_image_src {
            let _ = Util::strip_node(
                context,
                &format!("//img[contains(@src,'{}')]", xpath_strip_img_src),
            );
        }

        let _ = Self::fix_lazy_images(context, "lazyload", "data-src");
        let _ = Self::fix_iframe_size(context, "youtube.com");
        let _ = Self::remove_attribute(context, None, "style");
        let _ = Self::remove_attribute(context, Some("a"), "onclick");
        let _ = Self::remove_attribute(context, Some("img"), "srcset");
        let _ = Self::remove_attribute(context, Some("img"), "sizes");
        let _ = Self::add_attribute(context, Some("a"), "target", "_blank");

        let _ = Self::repair_urls(context, "//img", "src", url);
        let _ = Self::repair_urls(context, "//a", "src", url);
        let _ = Self::repair_urls(context, "//a", "href", url);
        let _ = Self::repair_urls(context, "//object", "data", url);
        let _ = Self::repair_urls(context, "//iframe", "src", url);

        // strip elements using Readability.com and Instapaper.com ignore class names
        // .entry-unrelated and .instapaper_ignore
        // See http://blog.instapaper.com/post/730281947
        let _ = Util::strip_node(
            context,
            "//*[contains(@class,' entry-unrelated ') or contains(@class,' instapaper_ignore ')]",
        );

        // strip elements that contain style="display: none;"
        let _ = Util::strip_node(context, "//*[contains(@style,'display:none')]");

        // strip all comments
        let _ = Util::strip_node(context, "//comment()");

        // strip all empty url-tags <a/>
        let _ = Util::strip_node(context, "//a[not(node())]");

        // strip all external css and fonts
        let _ = Util::strip_node(context, "//*[@type='text/css']");
    }

    fn extract_metadata(
        context: &Context,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
        article: &mut Article,
    ) {
        // try to get title
        if let Some(config) = config {
            for xpath_title in &config.xpath_title {
                if let Ok(title) = Util::extract_value_merge(context, xpath_title) {
                    debug!("Article title: '{}'", title);
                    article.title = Some(title);
                    break;
                }
            }
        }

        if article.title.is_none() {
            for xpath_title in &global_config.xpath_title {
                if let Ok(title) = Util::extract_value_merge(context, xpath_title) {
                    debug!("Article title: '{}'", title);
                    article.title = Some(title);
                    break;
                }
            }
        }

        // try to get the author
        if let Some(config) = config {
            for xpath_author in &config.xpath_author {
                if let Ok(author) = Util::extract_value(context, xpath_author) {
                    debug!("Article author: '{}'", author);
                    article.author = Some(author);
                    break;
                }
            }
        }

        if article.author.is_none() {
            for xpath_author in &global_config.xpath_author {
                if let Ok(author) = Util::extract_value(context, xpath_author) {
                    debug!("Article author: '{}'", author);
                    article.author = Some(author);
                    break;
                }
            }
        }

        // try to get the date
        if let Some(config) = config {
            for xpath_date in &config.xpath_date {
                if let Ok(date_string) = Util::extract_value(context, xpath_date) {
                    debug!("Article date: '{}'", date_string);
                    if let Ok(date) = DateTime::from_str(&date_string) {
                        article.date = Some(date);
                        break;
                    } else {
                        warn!("Parsing the date string '{}' failed", date_string);
                    }
                }
            }
        }

        if article.date.is_none() {
            for xpath_date in &global_config.xpath_date {
                if let Ok(date_string) = Util::extract_value(context, xpath_date) {
                    debug!("Article date: '{}'", date_string);
                    if let Ok(date) = DateTime::from_str(&date_string) {
                        article.date = Some(date);
                        break;
                    } else {
                        warn!("Parsing the date string '{}' failed", date_string);
                    }
                }
            }
        }
    }

    fn extract_body(
        context: &Context,
        root: &mut Node,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
    ) -> Result<(), FullTextParserError> {
        let mut found_something = false;

        if let Some(config) = config {
            for xpath_body in &config.xpath_body {
                found_something = Self::extract_body_single(context, root, xpath_body)?;
            }
        }

        if !found_something {
            for xpath_body in &global_config.xpath_body {
                found_something = Self::extract_body_single(context, root, xpath_body)?;
            }
        }

        if !found_something {
            log::error!("no body found");
            return Err(FullTextParserError::Scrape);
        }

        Ok(())
    }

    fn extract_body_single(
        context: &Context,
        root: &mut Node,
        xpath: &str,
    ) -> Result<bool, FullTextParserError> {
        let mut found_something = false;
        {
            let node_vec = Util::evaluate_xpath(context, xpath, false)?;
            for mut node in node_vec {
                if node.get_property("style").is_some() && node.remove_property("style").is_err() {
                    return Err(FullTextParserError::Xml);
                }

                node.unlink();
                if root.add_child(&mut node).is_ok() {
                    found_something = true;
                } else {
                    error!("Failed to add body to prepared document");
                    return Err(FullTextParserError::Xml);
                }
            }
        }

        Ok(found_something)
    }

    fn check_for_next_page(
        &self,
        context: &Context,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
    ) -> Option<url::Url> {
        if let Some(config) = config {
            if let Some(next_page_xpath) = config.next_page_link.as_deref() {
                if let Ok(next_page_string) = Self::get_attribute(context, next_page_xpath, "href")
                {
                    if let Ok(next_page_url) = url::Url::parse(&next_page_string) {
                        return Some(next_page_url);
                    }
                }
            }
        } else if let Some(next_page_xpath) = global_config.next_page_link.as_deref() {
            if let Ok(next_page_string) = Self::get_attribute(context, next_page_xpath, "href") {
                if let Ok(next_page_url) = url::Url::parse(&next_page_string) {
                    return Some(next_page_url);
                }
            }
        }

        // last page reached
        None
    }

    fn generate_head(root: &mut Node, document: &Document) -> Result<(), FullTextParserError> {
        if let Ok(mut head_node) = Node::new("head", None, document) {
            if let Ok(()) = root.add_prev_sibling(&mut head_node) {
                if let Ok(mut meta) = head_node.new_child(None, "meta") {
                    if meta.set_property("charset", "utf-8").is_ok() {
                        return Ok(());
                    }
                }
            }
        }

        Err(FullTextParserError::Xml)
    }

    fn prevent_self_closing_tags(context: &Context) -> Result<(), FullTextParserError> {
        // search document for empty tags and add a empty text node as child
        // this prevents libxml from self closing non void elements such as iframe

        let xpath = "//*[not(node())]";
        let node_vec = Util::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if node.get_name() == "meta" {
                continue;
            }

            let _ = node.add_text_child(None, "empty", "");
        }

        Ok(())
    }
}
