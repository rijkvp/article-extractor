#[macro_use]
mod macros;
mod config;
mod error;
mod article;
pub mod images;

use reqwest;
use url;
use regex;
use log::{
    error,
    debug,
    info,
    warn,
};
use crate::article::Article;
use libxml::parser::Parser;
use libxml::xpath::Context;
use libxml::tree::{
    Document,
    Node,
    SaveOptions,
};
use std::path::PathBuf;
use std::ops::Index;
use failure::ResultExt;
use std::error::Error;
use crate::config::{
    GrabberConfig,
    ConfigCollection
};
use encoding_rs::{
    Encoding,
};
use chrono::NaiveDateTime;
use std::str::FromStr;
use crate::images::ImageDownloader;
use self::error::{
    ScraperError,
    ScraperErrorKind
};


pub struct ArticleScraper {
    pub image_downloader: ImageDownloader,
    config_files: ConfigCollection,
    client: reqwest::Client,
}

impl ArticleScraper {
    pub fn new(config_path: PathBuf) -> Result<ArticleScraper, ScraperError> {

        let config_files = GrabberConfig::parse_directory(&config_path).context(ScraperErrorKind::Config)?;

        Ok(ArticleScraper {
            image_downloader: ImageDownloader::new((2048, 2048)),
            config_files: config_files,
            client: reqwest::Client::new(),
        })
    }

    pub fn parse(&self, url: url::Url, download_images: bool) -> Result<Article, ScraperError> {

        info!("Scraping article: {}", url.as_str());
        let response = self.client.head(url.clone()).send()
            .map_err(|err| {
                error!("Failed head request to: {} - {}", url.as_str(), err.description());
                err
            })
            .context(ScraperErrorKind::Http)?;

        // check if url redirects and we need to pick up the new url
        let mut url = url;
        if let Some(new_url) = ArticleScraper::check_redirect(&response) {
            debug!("Url {} redirects to {}", url.as_str(), new_url.as_str());
            url = new_url;
        }

        // check if we are dealing with text/html
        if !ArticleScraper::check_content_type(&response)? {
            return Err(ScraperErrorKind::ContentType)?
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

        let mut document = Document::new().map_err(|()| {
            ScraperErrorKind::Xml
        })?;

        let mut root = Node::new("article", None, &document).map_err(|()| {
            ScraperErrorKind::Xml
        })?;

        document.set_root_element(&root);

        ArticleScraper::generate_head(&mut root, &document)?;

        self.parse_first_page(&mut article, &url, &mut root, config)?;

        let context = Context::new(&document).map_err(|()| {
            error!("Failed to create xpath context for extracted article");
            ScraperErrorKind::Xml
        })?;

        if let Err(error) = ArticleScraper::prevent_self_closing_tags(&context) {
            error!("Preventing self closing tags failed - {}", error);
            return Err(error)
        }

        if let Err(error) = ArticleScraper::eliminate_noscrip_tag(&context) {
            error!("Eliminating <noscript> tag failed - {}", error);
            return Err(error)
        }

        if download_images {
            if let Err(error) = self.image_downloader.download_images_from_context(&context) {
                error!("Downloading images failed: {}", error);
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

    fn parse_first_page(&self, article: &mut Article, url: &url::Url, root: &mut Node, config: &GrabberConfig) -> Result<(), ScraperError> {

        let mut html = ArticleScraper::download(&url, &self.client)?;
        parse_html!(html, config, xpath_ctx);

        // check for single page link
        let mut xpath_ctx = xpath_ctx;
        if let Some(xpath_single_page_link) = config.single_page_link.clone() {
            debug!("Single page link xpath specified in config {}", xpath_single_page_link);
            if let Ok(result) = xpath_ctx.findvalue(&xpath_single_page_link, None) {
                // parse again with single page url
                debug!("Single page link found {}", result);
                let single_page_url = url::Url::parse(&result).context(ScraperErrorKind::Url)?;
                return self.parse_single_page(article, &single_page_url, root, config);
            }
        }

        ArticleScraper::extract_metadata(&xpath_ctx, config, article);
        ArticleScraper::strip_junk(&xpath_ctx, config, &url);
        ArticleScraper::extract_body(&xpath_ctx, root, config)?;

        self.check_for_next_page(&xpath_ctx, config, root)
    }

    fn parse_next_page(&self, url: &url::Url, root: &mut Node, config: &GrabberConfig) -> Result<(), ScraperError> {

        let mut html = ArticleScraper::download(&url, &self.client)?;
        parse_html!(html, config, xpath_ctx);
        ArticleScraper::strip_junk(&xpath_ctx, config, &url);
        ArticleScraper::extract_body(&xpath_ctx, root, config)?;

        self.check_for_next_page(&xpath_ctx, config, root)
    }

    fn parse_single_page(&self, article: &mut Article, url: &url::Url, root: &mut Node, config: &GrabberConfig) -> Result<(), ScraperError> {
        
        let mut html = ArticleScraper::download(&url, &self.client)?;
        parse_html!(html, config, xpath_ctx);
        ArticleScraper::extract_metadata(&xpath_ctx, config, article);
        ArticleScraper::strip_junk(&xpath_ctx, config, &url);
        ArticleScraper::extract_body(&xpath_ctx, root, config)?;

        Ok(())
    }

    fn download(url: &url::Url, client: &reqwest::Client) -> Result<String, ScraperError> {

        let mut response = client.get(url.as_str()).send()
            .map_err(|err| {
                error!("Downloading HTML failed: GET {} - {}", url.as_str(), err.description());
                err
            })
            .context(ScraperErrorKind::Http)?;

        if response.status().is_success() {
            let text = response.text().context(ScraperErrorKind::Http)?;
            {
                if let Some(decoded_html) = ArticleScraper::decode_html(&text, ArticleScraper::get_encoding_from_html(&text)) {
                    return Ok(decoded_html)
                }

                if let Some(decoded_html) = ArticleScraper::decode_html(&text, ArticleScraper::get_encoding_from_http_header(response.headers())) {
                    return Ok(decoded_html)
                }
            }

            warn!("No encoding of HTML detected - assuming utf-8");
            return Ok(text)
        }
        
        Err(ScraperErrorKind::Http)?
    }

    fn get_encoding_from_http_header(headers: &reqwest::header::HeaderMap) -> Option<&str> {

        if let Some(content_type) = headers.get(reqwest::header::CONTENT_TYPE) {
            if let Ok(content_type) = content_type.to_str() {
                let regex = regex::Regex::new(r#"charset=([^"']+)"#).unwrap();
                if let Some(captures) = regex.captures(content_type) {
                    if let Some(regex_match) = captures.get(1) {
                        return Some(regex_match.as_str())
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
                return Some(regex_match.as_str())
            }
        }
        None
    }

    fn decode_html(html: &str, encoding: Option<&str>) -> Option<String> {

        if let Some(encoding) = encoding {
            if let Some(encoding) = Encoding::for_label(encoding.as_bytes()) {
                let (decoded_html, _, invalid_chars) = encoding.decode(html.as_bytes());

                if !invalid_chars {
                    return Some(decoded_html.into_owned())
                }
            }
            warn!("Could not decode HTML. Encoding: {}", encoding);
        }
        None
    }

    fn get_grabber_config(&self, url: &url::Url) -> Result<&GrabberConfig, ScraperError> {

        let config_name = match url.host_str()
        {
            Some(name) => {
                let mut name = name;
                if name.starts_with("www.") {
                    name = &name[4..]
                }
                name
            },
            None => {
                error!("Getting config failed due to bad Url");
                return Err(ScraperErrorKind::Config)?
            },
        };

        let config_name = config_name.to_owned() + ".txt";

        if !self.config_files.contains_key(&config_name) {
            error!("No config file of the name {} fount", config_name);
            Err(ScraperErrorKind::Config)?
        }

        Ok(self.config_files.index(&config_name))
    }

    fn check_content_type(response: &reqwest::Response) -> Result<bool, ScraperError> {
    
        if response.status().is_success() {
            if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
                if let Ok(content_type) = content_type.to_str() {
                    if content_type.contains("text/html") {
                        return Ok(true)
                    }
                }
            }

            error!("Content type is not text/HTML");
            return Ok(false)
        }

        error!("Failed to determine content type");
        Err(ScraperErrorKind::Http)?
    }

    fn check_redirect(response: &reqwest::Response) -> Option<url::Url> {
        
        if response.status() == reqwest::StatusCode::PERMANENT_REDIRECT {
            debug!("Article url redirects to {}", response.url().as_str());
            return Some(response.url().clone())
        }

        None
    }

    fn extract_value(context: &Context, xpath: &str) -> Result<String, ScraperError> {

        evaluate_xpath!(context, xpath, node_vec);
        xpath_result_empty!(node_vec, xpath);
        if let Some(val) = node_vec.get(0) {
            return Ok(val.get_content())
        }
        
        Err(ScraperErrorKind::Xml)?
    }

    fn extract_value_merge(context: &Context, xpath: &str) -> Result<String, ScraperError> {

        evaluate_xpath!(context, xpath, node_vec);
        xpath_result_empty!(node_vec, xpath);
        let mut val = String::new();
        for node in node_vec {
            val.push_str(&node.get_content());
        }
        
        return Ok(val.trim().to_string())
    }

    fn strip_node(context: &Context, xpath: &String) -> Result<(), ScraperError> {

        let mut ancestor = xpath.clone();
        if ancestor.starts_with("//") {
            ancestor = ancestor.chars().skip(2).collect();
        }

        let query = &format!("{}[not(ancestor::{})]", xpath, ancestor);
        evaluate_xpath!(context, query, node_vec);
        for mut node in node_vec {
            node.unlink();
        }
        Ok(())
    }

    fn strip_id_or_class(context: &Context, id_or_class: &String) -> Result<(), ScraperError> {

        let xpath = &format!("//*[contains(@class, '{}') or contains(@id, '{}')]", id_or_class, id_or_class);
        evaluate_xpath!(context, xpath, node_vec);
        for mut node in node_vec {
            node.unlink();
        }
        Ok(())
    }

    fn fix_lazy_images(context: &Context, class: &str, property_url: &str) -> Result<(), ScraperError> {
        
        let xpath = &format!("//img[contains(@class, '{}')]", class);
        evaluate_xpath!(context, xpath, node_vec);
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
        evaluate_xpath!(context, xpath, node_vec);
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
                return Err(ScraperErrorKind::Xml)?
            }
            
            error!("Failed to get parent of iframe");
            return Err(ScraperErrorKind::Xml)?
        }
        Ok(())
    }

    fn remove_attribute(context: &Context, tag: Option<&str>, attribute: &str) -> Result<(), ScraperError> {

        let xpath_tag = match tag {
            Some(tag) => tag,
            None => "*"
        };

        let xpath = &format!("//{}[@{}]", xpath_tag, attribute);
        evaluate_xpath!(context, xpath, node_vec);
        for mut node in node_vec {
            if let Err(_) = node.remove_property(attribute) {
                return Err(ScraperErrorKind::Xml)?
            }
        }
        Ok(())
    }

    fn add_attribute(context: &Context, tag: Option<&str>, attribute: &str, value: &str) -> Result<(), ScraperError> {

        let xpath_tag = match tag {
            Some(tag) => tag,
            None => "*"
        };

        let xpath = &format!("//{}", xpath_tag);
        evaluate_xpath!(context, xpath, node_vec);
        for mut node in node_vec {
            if let Err(_) = node.set_attribute(attribute, value) {
                return Err(ScraperErrorKind::Xml)?
            }
        }
        Ok(())
    }

    fn get_attribute(context: &Context, xpath: &str, attribute: &str) -> Result<String, ScraperError> {

        evaluate_xpath!(context, xpath, node_vec);
        xpath_result_empty!(node_vec, xpath);
        for node in node_vec {
            if let Some(value) = node.get_attribute(attribute) {
                return Ok(value)
            }
        }

        Err(ScraperErrorKind::Xml)?
    }

    fn repair_urls(context: &Context, xpath: &str, attribute: &str, article_url: &url::Url) -> Result<(), ScraperError> {

        evaluate_xpath!(context, xpath, node_vec);
        for mut node in node_vec {
            if let Some(val) = node.get_attribute(attribute) {
                if let Err(url::ParseError::RelativeUrlWithoutBase) = url::Url::parse(&val) {
                    if let Ok(fixed_url) = ArticleScraper::complete_url(article_url, &val) {
                        if let Err(_) = node.set_attribute(attribute, fixed_url.as_str()) {
                            return Err(ScraperErrorKind::Xml)?
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn complete_url(article_url: &url::Url, incomplete_url: &str) -> Result<url::Url, ScraperError> {

        let mut completed_url = article_url.scheme().to_owned();
        completed_url.push(':');

        if !incomplete_url.starts_with("//") {
            match article_url.host() {
                Some(url::Host::Domain(host)) => {
                    completed_url.push_str("//");
                    completed_url.push_str(host);
                }
                _ => return Err(ScraperErrorKind::Url)?
            };
        }

        completed_url.push_str(incomplete_url);
        let url = url::Url::parse(&completed_url).context(ScraperErrorKind::Url)?;
        return Ok(url)
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
            let _ = ArticleScraper::strip_node(&context, &format!("//img[contains(@src,'{}')]", xpath_strip_img_src));
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
        let _ = ArticleScraper::strip_node(&context, &String::from("//*[contains(@style,'display:none')]"));

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
                debug!("Article title: {}", title);
                article.title = Some(title);
                break;
            }
        }

        // try to get the author
        for xpath_author in &config.xpath_author {
            if let Ok(author) = ArticleScraper::extract_value(&context, xpath_author) {
                debug!("Article author: {}", author);
                article.author = Some(author);
                break;
            }
        }

        // try to get the date
        for xpath_date in &config.xpath_date {
            if let Ok(date_string) = ArticleScraper::extract_value(&context, xpath_date) {
                debug!("Article date: {}", date_string);
                if let Ok(date) = NaiveDateTime::from_str(&date_string) {
                    article.date = Some(date);
                    break;
                }
                else {
                    warn!("Parsing the date string '{}' failed", date_string);
                }
            }
        }
    }

    fn extract_body(context: &Context, root: &mut Node, config: &GrabberConfig) -> Result<(), ScraperError> {

        let mut found_something = false;
        for xpath_body in &config.xpath_body {
            found_something = ArticleScraper::extract_body_single(&context, root, xpath_body)?;
        }

        if !found_something {
            return Err(ScraperErrorKind::Scrape)?
        }

        Ok(())
    }

    fn extract_body_single(context: &Context, root: &mut Node, xpath: &str) -> Result<bool, ScraperError> {

        let mut found_something = false;
        {
            evaluate_xpath!(context, xpath, node_vec);
            xpath_result_empty!(node_vec, xpath);
            for mut node in node_vec {
                if node.get_property("style").is_some() {
                    if let Err(_) = node.remove_property("style") {
                        return Err(ScraperErrorKind::Xml)?
                    }
                }

                node.unlink();
                if let Ok(_) = root.add_child(&mut node) {
                    found_something = true;
                }
                else {
                    error!("Failed to add body to prepared document");
                    return Err(ScraperErrorKind::Xml)?
                }
            }
        }

        Ok(found_something)
    }

    fn check_for_next_page(&self, context: &Context, config: &GrabberConfig, root: &mut Node) -> Result<(), ScraperError> {

        if let Some(next_page_xpath) = config.next_page_link.clone() {
            if let Ok(next_page_string) = ArticleScraper::get_attribute(&context, &next_page_xpath, "href") {
                if let Ok(next_page_url) = url::Url::parse(&next_page_string) {
                    return self.parse_next_page(&next_page_url, root, config)
                }
            }
        }

        // last page reached
        Ok(())
    }

    fn generate_head(root: &mut Node, document: &Document) -> Result<(), ScraperError> {

        if let Ok(mut head_node) = Node::new("head", None, document) {
            if let Ok(()) = root.add_prev_sibling(&mut head_node) {
                if let Ok(mut meta) = head_node.new_child(None, "meta") {
                    if let Ok(_) = meta.set_property("charset", "utf-8") {
                        return Ok(())
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
        evaluate_xpath!(context, xpath, node_vec);
        for mut node in node_vec {
            if node.get_name() == "meta" {
                continue
            }

            let _ = node.add_text_child(None, "empty", "");
        }

        Ok(())
    }

    fn eliminate_noscrip_tag(context: &Context) -> Result<(), ScraperError> {

        let xpath = "//noscript";
        evaluate_xpath!(context, xpath, node_vec);

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
    
    #[test]
    pub fn golem() {
        let config_path = PathBuf::from(r"./resources/tests/golem");
        let out_path = PathBuf::from(r"./test_output");
        let url = url::Url::parse("https://www.golem.de/news/http-error-418-fehlercode-ich-bin-eine-teekanne-darf-bleiben-1708-129460.html").unwrap();

        let grabber = ArticleScraper::new(config_path).unwrap();
        let article = grabber.parse(url, true).unwrap();
        article.save_html(&out_path).unwrap();

        assert_eq!(article.title, Some(String::from("HTTP Error 418: Fehlercode \"Ich bin eine Teekanne\" darf bleiben")));
        assert_eq!(article.author, Some(String::from("Hauke Gierow")));
    }

    #[test]
    pub fn phoronix() {
        let config_path = PathBuf::from(r"./resources/tests/phoronix");
        let out_path = PathBuf::from(r"./test_output");
        let url = url::Url::parse("http://www.phoronix.com/scan.php?page=article&item=amazon_ec2_bare&num=1").unwrap();

        let grabber = ArticleScraper::new(config_path).unwrap();
        let article = grabber.parse(url, true).unwrap();
        article.save_html(&out_path).unwrap();

        assert_eq!(article.title, Some(String::from("Amazon EC2 Cloud Benchmarks Against Bare Metal Systems")));
    }
}
