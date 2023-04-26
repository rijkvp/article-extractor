pub mod config;
pub mod error;
mod fingerprints;
mod metadata;
mod readability;

#[cfg(test)]
mod tests;

use self::config::{ConfigCollection, ConfigEntry};
use self::error::FullTextParserError;
pub use self::readability::Readability;
use crate::article::Article;
use crate::constants;
use crate::util::Util;

use encoding_rs::Encoding;
use fingerprints::Fingerprints;
use libxml::parser::Parser;
use libxml::tree::{Document, Node, NodeType};
use libxml::xpath::Context;
use reqwest::header::HeaderMap;
use reqwest::{Client, Response, Url};
use std::collections::HashSet;
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

    pub async fn parse_offline(
        &self,
        html: &str,
        config: Option<&ConfigEntry>,
        base_url: Option<Url>,
    ) -> Result<String, FullTextParserError> {
        libxml::tree::node::set_node_rc_guard(10);

        let global_config = self
            .config_files
            .get("global.txt")
            .ok_or(FullTextParserError::Config)?;

        let url =
            base_url.unwrap_or_else(|| url::Url::parse("http://fakehost/test/base/").unwrap());

        let mut article = Article {
            title: None,
            author: None,
            url,
            date: None,
            thumbnail_url: None,
            document: None,
            root_node: None,
        };

        let mut new_document = Document::new().map_err(|()| FullTextParserError::Xml)?;
        let mut root =
            Node::new("article", None, &new_document).map_err(|()| FullTextParserError::Xml)?;
        new_document.set_root_element(&root);

        Self::generate_head(&mut root, &new_document)?;

        let old_document = Self::parse_html(html, config, global_config)?;
        let xpath_ctx = Self::get_xpath_ctx(&old_document)?;

        if let Some(url) = self.check_for_next_page(&xpath_ctx, config, global_config, &article.url)
        {
            log::info!("Next page url: {url}");
        }

        metadata::extract(&xpath_ctx, config, Some(global_config), &mut article);
        if article.thumbnail_url.is_none() {
            Self::check_for_thumbnail(&xpath_ctx, &mut article);
        }
        Self::prep_content(
            &xpath_ctx,
            config,
            global_config,
            &article.url,
            &old_document,
        );
        let found_body = Self::extract_body(&xpath_ctx, &mut root, config, global_config)?;
        if !found_body {
            log::error!("Ftr failed to find content");
            return Err(FullTextParserError::Scrape);
        }

        if let Err(error) = Self::prevent_self_closing_tags(&xpath_ctx) {
            log::error!("Preventing self closing tags failed - '{error}'");
            return Err(error);
        }

        Self::post_process_document(&new_document)?;

        article.document = Some(new_document);
        article.root_node = Some(root);
        let html = article.get_content().ok_or(FullTextParserError::Scrape)?;
        Ok(html)
    }

    pub(crate) async fn parse(
        &self,
        url: &url::Url,
        client: &Client,
    ) -> Result<Article, FullTextParserError> {
        libxml::tree::node::set_node_rc_guard(10);

        log::debug!("Scraping article: '{url}'");

        // check if we have a config for the url
        let config = self.get_grabber_config(url);
        let global_config = self
            .config_files
            .get("global.txt")
            .ok_or(FullTextParserError::Config)?;

        let headers = Util::generate_headers(config, global_config)?;
        let response = Self::get_response(url, client, headers).await?;

        // check if url redirects and we need to pick up the new url
        let url = if let Some(new_url) = Util::check_redirect(&response, url) {
            log::debug!("Url '{url}' redirects to '{new_url}'");
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
            root_node: None,
        };

        let mut document = Document::new().map_err(|()| FullTextParserError::Xml)?;
        let mut root =
            Node::new("article", None, &document).map_err(|()| FullTextParserError::Xml)?;
        document.set_root_element(&root);

        Self::generate_head(&mut root, &document)?;

        let html = Self::get_body(response).await?;

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
            log::error!("Failed to create xpath context for extracted article");
            FullTextParserError::Xml
        })?;

        if let Err(error) = Self::prevent_self_closing_tags(&context) {
            log::error!("Preventing self closing tags failed - '{error}'");
            return Err(error);
        }

        Self::post_process_document(&document)?;

        article.document = Some(document);
        article.root_node = Some(root);

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
            log::debug!(
                "Single page link xpath specified in config '{}'",
                xpath_single_page_link
            );

            if let Some(single_page_url) = Util::find_page_url(&xpath_ctx, xpath_single_page_link) {
                // parse again with single page url
                log::debug!("Single page link found '{}'", single_page_url);

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
                    log::warn!("Single Page parsing: {error}");
                    log::info!("Continuing with regular parser.");
                }
            }
        }

        metadata::extract(&xpath_ctx, config, Some(global_config), article);
        let mut next_page_url =
            self.check_for_next_page(&xpath_ctx, config, global_config, &article.url);

        if article.thumbnail_url.is_none() {
            Self::check_for_thumbnail(&xpath_ctx, article);
        }
        Self::prep_content(&xpath_ctx, config, global_config, &article.url, &document);
        let found_body = Self::extract_body(&xpath_ctx, root, config, global_config)?;

        if !found_body {
            if let Err(error) = Readability::extract_body(document, root, article.title.as_deref())
            {
                log::error!("Both ftr and readability failed to find content: {error}");
                return Err(error);
            }
        }

        while let Some(url) = next_page_url.take() {
            log::debug!("next page");

            let headers = Util::generate_headers(config, global_config)?;
            let html = Self::download(&url, client, headers).await?;
            document = Self::parse_html(&html, config, global_config)?;
            xpath_ctx = Self::get_xpath_ctx(&document)?;
            if let Some(url) =
                self.check_for_next_page(&xpath_ctx, config, global_config, &article.url)
            {
                next_page_url.replace(url);
            }
            Self::prep_content(&xpath_ctx, config, global_config, &article.url, &document);
            let found_body = Self::extract_body(&xpath_ctx, root, config, global_config)?;

            if !found_body {
                if let Err(error) =
                    Readability::extract_body(document, root, article.title.as_deref())
                {
                    log::error!("Both ftr and readability failed to find content: {error}");
                    return Err(error);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn parse_html(
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
            log::error!("Parsing HTML failed for downloaded HTML {:?}", err);
            FullTextParserError::Xml
        })
    }

    pub(crate) fn get_xpath_ctx(doc: &Document) -> Result<Context, FullTextParserError> {
        Context::new(doc).map_err(|()| {
            log::error!("Creating xpath context failed for downloaded HTML");
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
        Self::prep_content(&xpath_ctx, config, global_config, url, &document);
        Self::extract_body(&xpath_ctx, root, config, global_config)?;

        Ok(())
    }

    async fn get_response(
        url: &url::Url,
        client: &Client,
        headers: HeaderMap,
    ) -> Result<Response, FullTextParserError> {
        let response = client
            .get(url.as_str())
            .headers(headers)
            .send()
            .await
            .map_err(|err| {
                log::error!(
                    "Downloading HTML failed: GET '{}' - '{}'",
                    url.as_str(),
                    err
                );
                FullTextParserError::Http
            })?;
        Ok(response)
    }

    async fn get_body(response: Response) -> Result<String, FullTextParserError> {
        if response.status().is_success() {
            let headers = response.headers().clone();
            let bytes = response
                .bytes()
                .await
                .map_err(|_| FullTextParserError::Http)?;

            match from_utf8(&bytes) {
                Ok(utf8_str) => {
                    log::debug!("Valid utf-8 string");
                    return Ok(utf8_str.into());
                }
                Err(error) => {
                    log::debug!("Invalid utf-8 string");
                    let lossy_string = std::string::String::from_utf8_lossy(&bytes);

                    if let Some(encoding) = Self::get_encoding_from_html(&lossy_string) {
                        log::debug!("Encoding extracted from HTML: '{}'", encoding);
                        if let Some(decoded_html) = Self::decode_html(&bytes, encoding) {
                            let decoded_html = decoded_html.replacen(
                                &format!("charset=\"{encoding}\""),
                                "charset=\"utf-8\"",
                                1,
                            );
                            return Ok(decoded_html);
                        }
                    }

                    if let Some(encoding) = Self::get_encoding_from_http_header(&headers) {
                        log::debug!("Encoding extracted from headers: '{}'", encoding);
                        if let Some(decoded_html) = Self::decode_html(&bytes, encoding) {
                            let decoded_html = decoded_html.replacen(
                                &format!("charset=\"{encoding}\""),
                                "charset=\"utf-8\"",
                                1,
                            );
                            return Ok(decoded_html);
                        }
                    }

                    return Err(FullTextParserError::Utf8(error));
                }
            }
        }

        Err(FullTextParserError::Http)
    }

    pub async fn download(
        url: &url::Url,
        client: &Client,
        headers: HeaderMap,
    ) -> Result<String, FullTextParserError> {
        let response = Self::get_response(url, client, headers).await?;
        let body = Self::get_body(response).await?;
        Ok(body)
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
        log::warn!("Could not decode HTML. Encoding: '{}'", encoding);
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
                log::error!("Getting config failed due to bad Url");
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

    fn fix_lazy_images(context: &Context, doc: &Document) -> Result<(), FullTextParserError> {
        let mut img_nodes = Util::evaluate_xpath(context, "//img", false)?;
        let pic_nodes = Util::evaluate_xpath(context, "//picture", false)?;
        let fig_nodes = Util::evaluate_xpath(context, "//figure", false)?;

        img_nodes.extend(pic_nodes);
        img_nodes.extend(fig_nodes);

        for mut node in img_nodes {
            let tag_name = node.get_name().to_uppercase();

            // In some sites (e.g. Kotaku), they put 1px square image as base64 data uri in the src attribute.
            // So, here we check if the data uri is too short, just might as well remove it.
            if let Some(src) = node.get_attribute("src") {
                // Make sure it's not SVG, because SVG can have a meaningful image in under 133 bytes.
                if let Some(mime) = constants::BASE64_DATA_URL
                    .captures(&src)
                    .and_then(|c| c.get(1).map(|c| c.as_str()))
                {
                    if mime == "image/svg+xml" {
                        continue;
                    }
                }

                // Make sure this element has other attributes which contains image.
                // If it doesn't, then this src is important and shouldn't be removed.
                let mut src_could_be_removed = false;
                for (name, val) in node.get_attributes() {
                    if name == "src" {
                        continue;
                    }

                    if constants::IS_IMAGE.is_match(&val) {
                        src_could_be_removed = true;
                        break;
                    }
                }

                // Here we assume if image is less than 100 bytes (or 133B after encoded to base64)
                // it will be too small, therefore it might be placeholder image.
                if src_could_be_removed {
                    if let Some(_match) = constants::IS_BASE64.find(&src) {
                        let b64starts = _match.start() + 7;
                        let b64length = src.len() - b64starts;
                        if b64length < 133 {
                            _ = node.remove_attribute("src");
                        }
                    }
                }
            }

            let class_contains_lazy = node
                .get_attribute("class")
                .map(|c| c.to_lowercase().contains("lazy"))
                .unwrap_or(false);
            let has_scr = node.has_attribute("src");
            let has_srcset = node.has_attribute("srcset");

            if (has_scr || has_srcset) && !class_contains_lazy {
                continue;
            }

            for (name, val) in node.get_attributes() {
                if name == "src" || name == "srcset" || name == "alt" {
                    continue;
                }

                let mut copy_to: Option<&str> = None;
                if constants::COPY_TO_SRCSET.is_match(&val) {
                    copy_to = Some("srcset");
                } else if constants::COPY_TO_SRC.is_match(&val) {
                    copy_to = Some("src");
                }

                if let Some(copy_to) = copy_to {
                    //if this is an img or picture, set the attribute directly
                    if tag_name == "IMG" || tag_name == "PICTURE" {
                        _ = node.set_attribute(copy_to, &val);
                    } else if tag_name == "FIGURE"
                        && !Util::has_any_descendent_tag(&node, &HashSet::from(["IMG", "PICTURE"]))
                    {
                        //if the item is a <figure> that does not contain an image or picture, create one and place it inside the figure
                        //see the nytimes-3 testcase for an example
                        let mut img = Node::new("img", None, doc).unwrap();
                        _ = img.set_attribute(copy_to, &val);
                        _ = node.add_child(&mut img);
                    }
                }
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
                    log::warn!("Failed to add iframe as child of video wrapper <div>");
                }
            } else {
                log::warn!("Failed to get parent of iframe");
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

    fn repair_urls(
        context: &Context,
        xpath: &str,
        attribute: &str,
        article_url: &url::Url,
        document: &Document,
    ) -> Result<(), FullTextParserError> {
        let node_vec = Util::evaluate_xpath(context, xpath, false)?;
        for mut node in node_vec {
            if let Some(url) = node.get_attribute(attribute) {
                let trimmed_url = url.trim();

                let is_hash_url = url.starts_with('#');
                let is_relative_url = url::Url::parse(&url)
                    .err()
                    .map(|err| err == url::ParseError::RelativeUrlWithoutBase)
                    .unwrap_or(false);
                let is_javascript = trimmed_url.contains("javascript:");

                if !is_hash_url && node.get_name().to_uppercase() == "A" {
                    _ = node.set_attribute("target", "_blank");
                }

                if let Some(srcset) = node.get_attribute("srcset") {
                    let res = constants::SRC_SET_URL
                        .captures_iter(&srcset)
                        .map(|cap| {
                            let cap0 = cap.get(0).map_or("", |m| m.as_str());
                            let cap1 = cap.get(1).map_or("", |m| m.as_str());
                            let cap2 = cap.get(2).map_or("", |m| m.as_str());
                            let cap3 = cap.get(3).map_or("", |m| m.as_str());

                            let is_relative_url = url::Url::parse(cap1)
                                .err()
                                .map(|err| err == url::ParseError::RelativeUrlWithoutBase)
                                .unwrap_or(false);

                            if is_relative_url {
                                let completed_url = article_url
                                    .join(cap1)
                                    .map(|u| u.as_str().to_owned())
                                    .unwrap_or_default();
                                format!("{completed_url}{cap2}{cap3}")
                            } else {
                                cap0.to_string()
                            }
                        })
                        .collect::<Vec<String>>()
                        .join(" ");

                    _ = node.set_attribute("srcset", res.as_str());
                }

                if is_hash_url {
                    _ = node.set_attribute(attribute, trimmed_url);
                } else if is_relative_url {
                    let completed_url = match article_url.join(trimmed_url) {
                        Ok(joined_url) => joined_url,
                        Err(_) => continue,
                    };
                    _ = node.set_attribute(attribute, completed_url.as_str());
                } else if is_javascript {
                    // if the link only contains simple text content, it can be converted to a text node
                    let mut child_nodes = node.get_child_nodes();
                    let child_count = child_nodes.len();
                    let first_child_is_text = child_nodes
                        .first()
                        .and_then(|n| n.get_type())
                        .map(|t| t == NodeType::TextNode)
                        .unwrap_or(false);
                    if let Some(mut parent) = node.get_parent() {
                        let new_node = if child_count == 1 && first_child_is_text {
                            let link_content = node.get_content();
                            Node::new_text(&link_content, document)
                                .expect("Failed to create new text node")
                        } else {
                            let mut container = Node::new("span", None, document)
                                .expect("Failed to create new span container node");
                            for mut child in child_nodes.drain(..) {
                                child.unlink();
                                _ = container.add_child(&mut child);
                            }
                            container
                        };

                        _ = parent.replace_child_node(new_node, node);
                    }
                } else if let Ok(parsed_url) = Url::parse(trimmed_url) {
                    _ = node.set_attribute(attribute, parsed_url.as_str());
                } else {
                    _ = node.set_attribute(attribute, trimmed_url);
                };
            }
        }
        Ok(())
    }

    fn fix_urls(context: &Context, url: &Url, document: &Document) {
        _ = Self::repair_urls(context, "//img", "src", url, document);
        _ = Self::repair_urls(context, "//a", "src", url, document);
        _ = Self::repair_urls(context, "//a", "href", url, document);
        _ = Self::repair_urls(context, "//object", "data", url, document);
        _ = Self::repair_urls(context, "//iframe", "src", url, document);
    }

    pub(crate) fn prep_content(
        context: &Context,
        config: Option<&ConfigEntry>,
        global_config: &ConfigEntry,
        url: &Url,
        document: &Document,
    ) {
        // replace H1 with H2 as H1 should be only title that is displayed separately
        if let Ok(h1_nodes) = Util::evaluate_xpath(context, "//h1", false) {
            for mut h1_node in h1_nodes {
                _ = h1_node.set_name("h2");
            }
        }

        // rename all font nodes to span
        if let Ok(font_nodes) = Util::evaluate_xpath(context, "//font", false) {
            for mut font_node in font_nodes {
                _ = font_node.set_name("span");
            }
        }

        _ = Util::mark_data_tables(context);

        // strip specified xpath
        if let Some(config) = config {
            for xpath_strip in &config.xpath_strip {
                _ = Util::strip_node(context, xpath_strip);
            }
        }

        for xpath_strip in &global_config.xpath_strip {
            _ = Util::strip_node(context, xpath_strip);
        }

        // strip everything with specified 'id' or 'class'
        if let Some(config) = config {
            for xpaht_strip_class in &config.strip_id_or_class {
                _ = Util::strip_id_or_class(context, xpaht_strip_class);
            }
        }

        for xpaht_strip_class in &global_config.strip_id_or_class {
            _ = Util::strip_id_or_class(context, xpaht_strip_class);
        }

        // strip any <img> element where @src attribute contains this substring
        if let Some(config) = config {
            for xpath_strip_img_src in &config.strip_image_src {
                _ = Util::strip_node(
                    context,
                    &format!("//img[contains(@src,'{}')]", xpath_strip_img_src),
                );
            }
        }

        for xpath_strip_img_src in &global_config.strip_image_src {
            _ = Util::strip_node(
                context,
                &format!("//img[contains(@src,'{}')]", xpath_strip_img_src),
            );
        }

        _ = Self::unwrap_noscript_images(context);
        _ = Util::strip_node(context, "//noscript");

        _ = Self::fix_lazy_images(context, document);
        _ = Self::fix_iframe_size(context, "youtube.com");
        _ = Self::remove_attribute(context, Some("a"), "onclick");

        // strip elements using Readability.com and Instapaper.com ignore class names
        // .entry-unrelated and .instapaper_ignore
        // See http://blog.instapaper.com/post/730281947
        _ = Util::strip_node(
            context,
            "//*[contains(@class,' entry-unrelated ') or contains(@class,' instapaper_ignore ')]",
        );

        // strip elements that contain style="display: none;"
        _ = Util::strip_node(context, "//*[contains(@style,'display:none')]");
        _ = Util::strip_node(context, "//*[contains(@style,'display: none')]");
        _ = Self::remove_attribute(context, None, "style");

        // strip all input elements
        _ = Util::strip_node(context, "//form");
        _ = Util::strip_node(context, "//input");
        _ = Util::strip_node(context, "//textarea");
        _ = Util::strip_node(context, "//select");
        _ = Util::strip_node(context, "//button");

        // strip all comments
        _ = Util::strip_node(context, "//comment()");

        // strip all scripts
        _ = Util::strip_node(context, "//script");

        // strip all styles
        _ = Util::strip_node(context, "//style");

        // strip all empty url-tags <a/>
        _ = Util::strip_node(context, "//a[not(node())]");

        // strip all external css and fonts
        _ = Util::strip_node(context, "//*[@type='text/css']");

        // other junk
        _ = Util::strip_node(context, "//iframe");
        _ = Util::strip_node(context, "//object");
        _ = Util::strip_node(context, "//embed");
        _ = Util::strip_node(context, "//footer");
        _ = Util::strip_node(context, "//link");
        _ = Util::strip_node(context, "//aside");

        Self::fix_urls(context, url, document);
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
                        if let Some(first_child) = noscript_node.get_first_element_child() {
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

                Self::post_process_page(&mut node)?;

                node.unlink();
                if root.add_child(&mut node).is_ok() {
                    found_something = true;
                } else {
                    log::error!("Failed to add body to prepared document");
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
        article_url: &url::Url,
    ) -> Option<url::Url> {
        if let Some(config) = config {
            if let Some(next_page_xpath) = config.next_page_link.as_deref() {
                if let Ok(next_page_string) = Util::get_attribute(context, next_page_xpath, "href")
                {
                    if let Some(next_page_url) = Self::parse_url(&next_page_string, article_url) {
                        return Some(next_page_url);
                    }
                }
            }
        } else if let Some(next_page_xpath) = global_config.next_page_link.as_deref() {
            if let Ok(next_page_string) = Util::get_attribute(context, next_page_xpath, "href") {
                if let Some(next_page_url) = Self::parse_url(&next_page_string, article_url) {
                    return Some(next_page_url);
                }
            }
        }

        // last page reached
        None
    }

    fn parse_url(url: &str, article_url: &url::Url) -> Option<url::Url> {
        let is_relative_url = url::Url::parse(url)
            .err()
            .map(|err| err == url::ParseError::RelativeUrlWithoutBase)
            .unwrap_or(false);

        if is_relative_url {
            article_url.join(url.trim()).ok()
        } else {
            url::Url::parse(url).ok()
        }
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

            _ = node.add_text_child(None, "empty", "");
        }

        Ok(())
    }

    pub(crate) fn post_process_document(document: &Document) -> Result<(), FullTextParserError> {
        if let Some(mut root) = document.get_root_element() {
            Self::simplify_nested_elements(&mut root)?;
            Self::clean_attributes(&mut root)?;
            Self::remove_single_cell_tables(&mut root);
            Self::remove_extra_p_and_div(&mut root);
        }

        Ok(())
    }

    pub(crate) fn post_process_page(node: &mut Node) -> Result<(), FullTextParserError> {
        Util::clean_headers(node);
        Util::clean_conditionally(node, "fieldset");
        Util::clean_conditionally(node, "table");
        Util::clean_conditionally(node, "ul");
        Util::clean_conditionally(node, "div");

        Self::remove_share_elements(node);
        Self::clean_attributes(node)?;
        Self::remove_single_cell_tables(node);
        Self::remove_extra_p_and_div(node);

        Ok(())
    }

    fn remove_single_cell_tables(root: &mut Node) {
        let mut node_iter = Some(root.clone());

        while let Some(node) = node_iter {
            let tag_name = node.get_name().to_uppercase();
            if tag_name == "TABLE" {
                let t_body = if Util::has_single_tag_inside_element(&node, "TBODY") {
                    node.get_child_elements().drain(..).next().unwrap()
                } else {
                    node.clone()
                };
                if Util::has_single_tag_inside_element(&t_body, "TR") {
                    let row = t_body.get_child_elements().first().cloned();
                    if let Some(row) = row {
                        if Util::has_single_tag_inside_element(&row, "TD") {
                            let cell = row.get_child_elements().first().cloned();
                            if let Some(mut cell) = cell {
                                let all_phrasing_content = cell
                                    .get_child_elements()
                                    .into_iter()
                                    .all(|child| Util::is_phrasing_content(&child));
                                cell.set_name(if all_phrasing_content { "P" } else { "DIV" })
                                    .unwrap();
                                if let Some(mut parent) = node.get_parent() {
                                    node_iter = Util::next_node(&node, true);
                                    parent.replace_child_node(cell, node.clone()).unwrap();
                                    continue;
                                }
                            }
                        }
                    }
                }
            }

            node_iter = Util::next_node(&node, false);
        }
    }

    fn remove_extra_p_and_div(root: &mut Node) {
        let mut node_iter = Some(root.clone());

        while let Some(mut node) = node_iter {
            let tag_name = node.get_name().to_uppercase();
            if tag_name == "P" || tag_name == "DIV" {
                let img_count = Util::get_elements_by_tag_name(&node, "img").len();
                let embed_count = Util::get_elements_by_tag_name(&node, "embed").len();
                let object_count = Util::get_elements_by_tag_name(&node, "object").len();
                let iframe_count = Util::get_elements_by_tag_name(&node, "iframe").len();
                let total_count = img_count + embed_count + object_count + iframe_count;

                if total_count == 0 && Util::get_inner_text(&node, false).trim().is_empty() {
                    node_iter = Util::remove_and_next(&mut node);
                    continue;
                }
            }

            node_iter = Util::next_node(&node, false);
        }
    }

    fn remove_share_elements(root: &mut Node) {
        let mut node_iter = Some(root.clone());

        while let Some(mut node) = node_iter {
            let match_string = format!(
                "{} {}",
                node.get_attribute("class").unwrap_or_default(),
                node.get_attribute("id").unwrap_or_default()
            );

            if constants::SHARE_ELEMENTS.is_match(&match_string)
                && node.get_content().len() < constants::DEFAULT_CHAR_THRESHOLD
            {
                node_iter = Util::remove_and_next(&mut node);
            } else {
                node_iter = Util::next_node(&node, false);
            }
        }
    }

    fn clean_attributes(root: &mut Node) -> Result<(), FullTextParserError> {
        let mut node_iter = Some(root.clone());

        while let Some(mut node) = node_iter {
            let tag_name = node.get_name().to_uppercase();

            for attr in constants::PRESENTATIONAL_ATTRIBUTES {
                _ = node.remove_attribute(attr);
            }

            if constants::DEPRECATED_SIZE_ATTRIBUTE_ELEMS.contains(tag_name.as_str()) {
                _ = node.remove_attribute("width");
                _ = node.remove_attribute("height");
            }

            node.remove_attribute("class").map_err(|e| {
                log::error!("{e}");
                FullTextParserError::Xml
            })?;

            node.remove_attribute("align").map_err(|e| {
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
                        if let Some(mut child) = node.get_child_elements().into_iter().next() {
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

                            node_iter = Util::next_node(&parent, false);
                            continue;
                        }
                    }
                }
            }

            node_iter = Util::next_node(&node, false);
        }
        Ok(())
    }
}
