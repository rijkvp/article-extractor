pub mod config;
pub mod error;
mod fingerprints;
mod metadata;
mod readability;

#[cfg(test)]
mod tests;

use self::config::{ConfigCollection, ConfigEntry};
use self::error::FullTextParserError;
use self::readability::Readability;
use crate::article::Article;
use crate::constants;
use crate::util::Util;

use encoding_rs::Encoding;
use fingerprints::Fingerprints;
use libxml::parser::Parser;
use libxml::tree::{Document, Node};
use libxml::xpath::Context;
use log::{debug, error, info, warn};
use reqwest::header::HeaderMap;
use reqwest::{Client, Url};
use std::path::Path;
use std::str::from_utf8;

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
        libxml::tree::node::set_node_rc_guard(10);

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
            thumbnail_url: None,
            document: None,
        };

        let mut document = Document::new().map_err(|()| FullTextParserError::Xml)?;
        let mut root =
            Node::new("article", None, &document).map_err(|()| FullTextParserError::Xml)?;
        document.set_root_element(&root);

        Self::generate_head(&mut root, &document)?;

        let headers = Util::generate_headers(config, global_config)?;
        let html = Self::download(&url, client, headers).await?;

        // check for fingerprints
        let config = if config.is_none() {
            if let Some(url) = Fingerprints::detect(&html) {
                self.get_grabber_config(&url)
            } else {
                config
            }
        } else {
            config
        };

        self.parse_pages(
            &mut article,
            &html,
            &mut root,
            config,
            global_config,
            client,
        )
        .await?;

        let context = Context::new(&document).map_err(|()| {
            error!("Failed to create xpath context for extracted article");
            FullTextParserError::Xml
        })?;

        if let Err(error) = Self::prevent_self_closing_tags(&context) {
            error!("Preventing self closing tags failed - '{}'", error);
            return Err(error);
        }

        Self::post_process_content(&document)?;

        article.document = Some(document);

        Ok(article)
    }

    async fn parse_pages(
        &self,
        article: &mut Article,
        html: &str,
        root: &mut Node,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
        client: &Client,
    ) -> Result<(), FullTextParserError> {
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

                if let Err(error) = self
                    .parse_single_page(
                        article,
                        &single_page_url,
                        root,
                        config,
                        global_config,
                        client,
                    )
                    .await
                {
                    log::warn!("Single Page parsing: {}", error);
                    log::debug!("Continuing with regular parser.");
                }
            }
        }

        metadata::extract(&xpath_ctx, config, Some(global_config), article);
        if article.thumbnail_url.is_none() {
            Self::check_for_thumbnail(&xpath_ctx, article);
        }
        Self::strip_junk(&xpath_ctx, config, global_config);
        Self::fix_urls(&xpath_ctx, &article.url);
        Self::unwrap_noscript_images(&xpath_ctx)?;
        let found_body = Self::extract_body(&xpath_ctx, root, config, global_config)?;

        if !found_body {
            if let Err(error) = Readability::extract_body(document, root, article.title.as_deref())
            {
                log::error!("Both ftr and readability failed to find content: {}", error);
                return Err(error);
            }
        }

        while let Some(url) = self.check_for_next_page(&xpath_ctx, config, global_config) {
            let headers = Util::generate_headers(config, global_config)?;
            let html = Self::download(&url, client, headers).await?;
            document = Self::parse_html(&html, config, global_config)?;
            xpath_ctx = Self::get_xpath_ctx(&document)?;
            Self::strip_junk(&xpath_ctx, config, global_config);
            Self::fix_urls(&xpath_ctx, &url);
            Self::unwrap_noscript_images(&xpath_ctx)?;
            Self::extract_body(&xpath_ctx, root, config, global_config)?;
        }

        Ok(())
    }

    fn parse_html(
        html: &str,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
    ) -> Result<Document, FullTextParserError> {
        // replace matches in raw html

        let mut html = html.to_owned();
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
        parser.parse_string(html.as_str()).map_err(|err| {
            error!("Parsing HTML failed for downloaded HTML {:?}", err);
            FullTextParserError::Xml
        })
    }

    fn get_xpath_ctx(doc: &Document) -> Result<Context, FullTextParserError> {
        Context::new(doc).map_err(|()| {
            error!("Creating xpath context failed for downloaded HTML");
            FullTextParserError::Xml
        })
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
        let document = Self::parse_html(&html, config, global_config)?;
        let xpath_ctx = Self::get_xpath_ctx(&document)?;
        metadata::extract(&xpath_ctx, config, Some(global_config), article);
        Self::check_for_thumbnail(&xpath_ctx, article);
        Self::strip_junk(&xpath_ctx, config, global_config);
        Self::fix_urls(&xpath_ctx, url);
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
            let bytes = response
                .bytes()
                .await
                .map_err(|_| FullTextParserError::Http)?;

            match from_utf8(&bytes) {
                Ok(utf8_str) => {
                    debug!("Valid utf-8 string");
                    return Ok(utf8_str.into());
                }
                Err(error) => {
                    debug!("Invalid utf-8 string");
                    let lossy_string = std::string::String::from_utf8_lossy(&bytes);

                    if let Some(encoding) = Self::get_encoding_from_html(&lossy_string) {
                        debug!("Encoding extracted from HTML: '{}'", encoding);
                        if let Some(decoded_html) = Self::decode_html(&bytes, encoding) {
                            return Ok(decoded_html);
                        }
                    }

                    if let Some(encoding) = Self::get_encoding_from_http_header(&headers) {
                        debug!("Encoding extracted from headers: '{}'", encoding);
                        if let Some(decoded_html) = Self::decode_html(&bytes, encoding) {
                            return Ok(decoded_html);
                        }
                    }

                    return Err(FullTextParserError::Utf8(error));
                }
            }
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
            regex::Regex::new(r#"<meta.*?charset="*(.*?)""#).expect("Failed to parse regex");
        if let Some(captures) = regex.captures(html) {
            if let Some(regex_match) = captures.get(1) {
                return Some(regex_match.as_str());
            }
        }
        None
    }

    fn decode_html(bytes: &[u8], encoding: &str) -> Option<String> {
        if let Some(encoding) = Encoding::for_label(encoding.as_bytes()) {
            let (decoded_html, _, invalid_chars) = encoding.decode(bytes);

            if !invalid_chars {
                return Some(decoded_html.into_owned());
            }
        }
        warn!("Could not decode HTML. Encoding: '{}'", encoding);
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
        if let Ok(thumb) = Util::get_attribute(
            context,
            "//meta[contains(@name, 'twitter:image')]",
            "content",
        ) {
            article.thumbnail_url = Some(thumb);
            return;
        }

        if let Ok(thumb) =
            Util::get_attribute(context, "//meta[contains(@name, 'og:image')]", "content")
        {
            article.thumbnail_url = Some(thumb);
            return;
        }

        if let Ok(thumb) =
            Util::get_attribute(context, "//link[contains(@rel, 'image_src')]", "href")
        {
            article.thumbnail_url = Some(thumb);
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
                    node.set_attribute(attribute, completed_url.as_str())
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

    fn fix_urls(context: &Context, url: &Url) {
        let _ = Self::repair_urls(context, "//img", "src", url);
        let _ = Self::repair_urls(context, "//a", "src", url);
        let _ = Self::repair_urls(context, "//a", "href", url);
        let _ = Self::repair_urls(context, "//object", "data", url);
        let _ = Self::repair_urls(context, "//iframe", "src", url);
    }

    fn strip_junk(context: &Context, config: Option<&ConfigEntry>, global_config: &ConfigEntry) {
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
        let _ = Self::remove_attribute(context, Some("a"), "onclick");
        let _ = Self::remove_attribute(context, Some("img"), "srcset");
        let _ = Self::remove_attribute(context, Some("img"), "sizes");
        let _ = Self::add_attribute(context, Some("a"), "target", "_blank");

        // strip elements using Readability.com and Instapaper.com ignore class names
        // .entry-unrelated and .instapaper_ignore
        // See http://blog.instapaper.com/post/730281947
        let _ = Util::strip_node(
            context,
            "//*[contains(@class,' entry-unrelated ') or contains(@class,' instapaper_ignore ')]",
        );

        // strip elements that contain style="display: none;"
        let _ = Util::strip_node(context, "//*[contains(@style,'display:none')]");
        let _ = Util::strip_node(context, "//*[contains(@style,'display: none')]");
        let _ = Self::remove_attribute(context, None, "style");

        // strip all comments
        let _ = Util::strip_node(context, "//input");
        let _ = Util::strip_node(context, "//textarea");
        let _ = Util::strip_node(context, "//select");
        let _ = Util::strip_node(context, "//button");

        // strip all input elements
        let _ = Util::strip_node(context, "//comment()");

        // strip all scripts
        let _ = Util::strip_node(context, "//script");

        // strip all styles
        let _ = Util::strip_node(context, "//style");

        // strip all empty url-tags <a/>
        let _ = Util::strip_node(context, "//a[not(node())]");

        // strip all external css and fonts
        let _ = Util::strip_node(context, "//*[@type='text/css']");

        // other junk
        let _ = Util::strip_node(context, "//object");
        let _ = Util::strip_node(context, "//embed");
        let _ = Util::strip_node(context, "//footer");
        let _ = Util::strip_node(context, "//link");
        let _ = Util::strip_node(context, "//aside");
    }

    /**
     * Find all <noscript> that are located after <img> nodes, and which contain only one
     * <img> element. Replace the first image with the image from inside the <noscript> tag,
     * and remove the <noscript> tag. This improves the quality of the images we use on
     * some sites (e.g. Medium).
     **/
    fn unwrap_noscript_images(ctx: &Context) -> Result<(), FullTextParserError> {
        // Find img without source or attributes that might contains image, and remove it.
        // This is done to prevent a placeholder img is replaced by img from noscript in next step.
        let img_nodes = Util::evaluate_xpath(ctx, "//img", false)?;
        for mut img_node in img_nodes {
            let attrs = img_node.get_attributes();

            let keep = attrs.iter().any(|(name, value)| {
                name == "src"
                    || name == "srcset"
                    || name == "data-src"
                    || name == "data-srcset"
                    || constants::IS_IMAGE.is_match(value)
            });
            if !keep {
                img_node.unlink();
            }
        }

        // Next find noscript and try to extract its image
        let noscript_nodes = Util::evaluate_xpath(ctx, "//noscript", false)?;
        for mut noscript_node in noscript_nodes {
            // Parse content of noscript and make sure it only contains image
            if !Util::is_single_image(&noscript_node) {
                continue;
            }

            // If noscript has previous sibling and it only contains image,
            // replace it with noscript content. However we also keep old
            // attributes that might contains image.
            if let Some(prev) = noscript_node.get_prev_element_sibling() {
                if Util::is_single_image(&prev) {
                    {
                        let mut prev_img = prev.clone();

                        if prev_img.get_name().to_uppercase() != "IMG" {
                            if let Some(img_node) = Util::get_elements_by_tag_name(&prev_img, "img")
                                .into_iter()
                                .next()
                            {
                                prev_img = img_node;
                            }
                        }

                        let new_img = Util::get_elements_by_tag_name(&noscript_node, "img")
                            .into_iter()
                            .next();
                        if let Some(mut new_img) = new_img {
                            for (key, value) in prev_img.get_attributes() {
                                if value.is_empty() {
                                    continue;
                                }

                                if key == "src"
                                    || key == "srcset"
                                    || constants::IS_IMAGE.is_match(&value)
                                {
                                    if new_img.get_attribute(&key).as_deref() == Some(&value) {
                                        continue;
                                    }

                                    let mut attr_name = key;
                                    if new_img.has_attribute(&attr_name) {
                                        attr_name = format!("data-old-{attr_name}");
                                    }

                                    new_img.set_attribute(&attr_name, &value).map_err(|e| {
                                        log::error!("{e}");
                                        FullTextParserError::Xml
                                    })?;
                                }
                            }
                        }
                    }

                    if let Some(mut parent) = noscript_node.get_parent() {
                        if let Some(first_child) = noscript_node.get_first_child() {
                            parent.replace_child_node(first_child, prev).map_err(|e| {
                                log::error!("{e}");
                                FullTextParserError::Xml
                            })?;
                            noscript_node.unlink();
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn extract_body(
        context: &Context,
        root: &mut Node,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
    ) -> Result<bool, FullTextParserError> {
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
        }

        Ok(found_something)
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
                if let Ok(next_page_string) = Util::get_attribute(context, next_page_xpath, "href")
                {
                    if let Ok(next_page_url) = url::Url::parse(&next_page_string) {
                        return Some(next_page_url);
                    }
                }
            }
        } else if let Some(next_page_xpath) = global_config.next_page_link.as_deref() {
            if let Ok(next_page_string) = Util::get_attribute(context, next_page_xpath, "href") {
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

    pub(crate) fn post_process_content(document: &Document) -> Result<(), FullTextParserError> {
        let context = Context::new(document).map_err(|()| {
            error!("Creating xpath context failed for article HTML");
            FullTextParserError::Xml
        })?;

        // replace H1 with H2 as H1 should be only title that is displayed separately
        let h1_nodes = Util::evaluate_xpath(&context, "//h1", false)?;
        for mut h1_node in h1_nodes {
            h1_node.set_name("h2").map_err(|e| {
                log::error!("{e}");
                FullTextParserError::Xml
            })?;
        }

        // Remove extra paragraphs & divs
        let mut nodes = Util::evaluate_xpath(&context, "//p", false)?;
        nodes.append(&mut Util::evaluate_xpath(&context, "//P", false)?);
        nodes.append(&mut Util::evaluate_xpath(&context, "//div", false)?);
        for mut node in nodes {
            let img_count = Util::get_elements_by_tag_name(&node, "img").len();
            let embed_count = Util::get_elements_by_tag_name(&node, "embed").len();
            let object_count = Util::get_elements_by_tag_name(&node, "object").len();
            let iframe_count = Util::get_elements_by_tag_name(&node, "iframe").len();
            let total_count = img_count + embed_count + object_count + iframe_count;

            if total_count == 0 && Util::get_inner_text(&node, false).trim().is_empty() {
                node.unlink();
            }
        }

        Util::mark_data_tables(&context)?;

        if let Some(mut root) = document.get_root_element() {
            Util::clean_conditionally(&mut root, "form")?;
            Util::clean_conditionally(&mut root, "fieldset")?;
            Util::clean_conditionally(&mut root, "table")?;
            Util::clean_conditionally(&mut root, "ul")?;
            Util::clean_conditionally(&mut root, "div")?;

            Self::clean_classes(&mut root)?;
            Self::simplify_nested_elements(&mut root)?;
        }

        Ok(())
    }

    fn clean_classes(root: &mut Node) -> Result<(), FullTextParserError> {
        let mut node_iter = Some(root.clone());

        while let Some(mut node) = node_iter {
            node.remove_attribute("class").map_err(|e| {
                log::error!("{e}");
                FullTextParserError::Xml
            })?;

            node.remove_attribute(constants::SCORE_ATTR).map_err(|e| {
                log::error!("{e}");
                FullTextParserError::Xml
            })?;

            node.remove_attribute(constants::DATA_TABLE_ATTR)
                .map_err(|e| {
                    log::error!("{e}");
                    FullTextParserError::Xml
                })?;

            node_iter = Util::next_node(&node, false);
        }
        Ok(())
    }

    fn simplify_nested_elements(root: &mut Node) -> Result<(), FullTextParserError> {
        let mut node_iter = Some(root.clone());

        while let Some(mut node) = node_iter {
            let tag_name = node.get_name().to_uppercase();
            if tag_name != "ARTICLE"
                && node.get_parent().is_some()
                && (tag_name == "DIV" || tag_name == "SECTION")
            {
                if Util::is_element_without_content(&node) {
                    node_iter = Util::remove_and_next(&mut node);
                    continue;
                } else if Util::has_single_tag_inside_element(&node, "DIV")
                    || Util::has_single_tag_inside_element(&node, "SECTION")
                {
                    if let Some(mut parent) = node.get_parent() {
                        if let Some(mut child) = node.get_child_nodes().into_iter().next() {
                            for (k, v) in node.get_attributes().into_iter() {
                                child.set_attribute(&k, &v).map_err(|e| {
                                    log::error!("{e}");
                                    FullTextParserError::Xml
                                })?;
                            }
                            parent
                                .replace_child_node(child, node.clone())
                                .map_err(|e| {
                                    log::error!("{e}");
                                    FullTextParserError::Xml
                                })?;
                        }
                    }
                }
            }

            node_iter = Util::next_node(&node, false);
        }
        Ok(())
    }
}
