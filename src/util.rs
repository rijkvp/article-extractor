use libxml::{tree::Node, xpath::Context};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Response,
};
use tokio::fs::DirEntry;

use crate::full_text_parser::{config::ConfigEntry, error::FullTextParserError};

pub struct Util;

impl Util {
    pub fn check_extension(path: &DirEntry, extension: &str) -> bool {
        if let Some(ext) = path.path().extension() {
            ext.to_str() == Some(extension)
        } else {
            false
        }
    }

    pub fn str_extract_value<'a>(identifier: &str, line: &'a str) -> &'a str {
        let value = &line[identifier.len()..];
        let value = value.trim();
        match value.find('#') {
            Some(pos) => &value[..pos],
            None => value,
        }
    }

    pub fn split_values(values: &str) -> Vec<&str> {
        values.split('|').map(|s| s.trim()).collect()
    }

    pub fn select_rule<'a>(
        site_specific_rule: Option<&'a str>,
        global_rule: Option<&'a str>,
    ) -> Option<&'a str> {
        if site_specific_rule.is_some() {
            site_specific_rule
        } else {
            global_rule
        }
    }

    pub fn generate_headers(
        site_specific_rule: Option<&ConfigEntry>,
        global_rule: &ConfigEntry,
    ) -> Result<HeaderMap, FullTextParserError> {
        let mut headers = HeaderMap::new();

        if let Some(config) = site_specific_rule {
            for header in &config.header {
                let name = HeaderName::from_bytes(header.name.as_bytes())
                    .map_err(|_| FullTextParserError::Config)?;
                let value = header
                    .value
                    .parse::<HeaderValue>()
                    .map_err(|_| FullTextParserError::Config)?;
                headers.insert(name, value);
            }
        }

        for header in &global_rule.header {
            let name = HeaderName::from_bytes(header.name.as_bytes())
                .map_err(|_| FullTextParserError::Config)?;
            let value = header
                .value
                .parse::<HeaderValue>()
                .map_err(|_| FullTextParserError::Config)?;
            headers.insert(name, value);
        }

        Ok(headers)
    }

    pub fn find_page_url(xpath_ctx: &Context, xpath_page_link: &str) -> Option<url::Url> {
        let res = Self::evaluate_xpath(xpath_ctx, xpath_page_link, false).ok()?;
        let mut url = None;

        for node in res {
            let content = node.get_content();
            let url_str = if content.trim().is_empty() && node.has_attribute("href") {
                node.get_attribute("href").unwrap()
            } else {
                content
            };

            if let Ok(parsed_url) = url::Url::parse(&url_str) {
                url = Some(parsed_url);
                break;
            }
        }

        url
    }

    pub fn evaluate_xpath(
        xpath_ctx: &Context,
        xpath: &str,
        thorw_if_empty: bool,
    ) -> Result<Vec<Node>, FullTextParserError> {
        let res = xpath_ctx.evaluate(xpath).map_err(|()| {
            log::debug!("Evaluation of xpath '{}' yielded no results", xpath);
            FullTextParserError::Xml
        })?;

        let node_vec = res.get_nodes_as_vec();

        if node_vec.is_empty() {
            log::debug!("Evaluation of xpath '{}' yielded no results", xpath);
            if thorw_if_empty {
                return Err(FullTextParserError::Xml);
            }
        }

        Ok(node_vec)
    }

    pub fn check_content_type(response: &Response) -> Result<bool, FullTextParserError> {
        if response.status().is_success() {
            if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
                if let Ok(content_type) = content_type.to_str() {
                    if content_type.contains("text/html") {
                        return Ok(true);
                    }
                }
            }

            log::error!("Content type is not text/HTML");
            return Ok(false);
        }

        log::error!("Failed to determine content type");
        Err(FullTextParserError::Http)
    }

    pub fn check_redirect(response: &Response, original_url: &url::Url) -> Option<url::Url> {
        if response.status() == reqwest::StatusCode::PERMANENT_REDIRECT {
            log::debug!("Article url redirects to '{}'", response.url().as_str());
            return Some(response.url().clone());
        } else if response.url() != original_url {
            return Some(response.url().clone());
        }

        None
    }

    pub fn extract_value(context: &Context, xpath: &str) -> Result<String, FullTextParserError> {
        let node_vec = Util::evaluate_xpath(context, xpath, false)?;
        if let Some(val) = node_vec.get(0) {
            return Ok(val.get_content());
        }

        Err(FullTextParserError::Xml)
    }

    pub fn extract_value_merge(
        context: &Context,
        xpath: &str,
    ) -> Result<String, FullTextParserError> {
        let node_vec = Util::evaluate_xpath(context, xpath, true)?;
        let mut val = String::new();
        for node in node_vec {
            let part = node
                .get_content()
                .split_whitespace()
                .map(|s| format!("{} ", s))
                .collect::<String>();
            val.push_str(&part);
            val.push(' ');
        }

        Ok(val.trim().to_string())
    }

    pub fn strip_node(context: &Context, xpath: &str) -> Result<(), FullTextParserError> {
        let mut ancestor = xpath.to_string();
        if ancestor.starts_with("//") {
            ancestor = ancestor.chars().skip(2).collect();
        }

        let query = &format!("{}[not(ancestor::{})]", xpath, ancestor);
        let node_vec = Util::evaluate_xpath(context, query, false)?;
        for mut node in node_vec {
            node.unlink();
        }
        Ok(())
    }

    pub fn strip_id_or_class(
        context: &Context,
        id_or_class: &str,
    ) -> Result<(), FullTextParserError> {
        let xpath = &format!(
            "//*[contains(@class, '{}') or contains(@id, '{}')]",
            id_or_class, id_or_class
        );

        let mut ancestor = xpath.clone();
        if ancestor.starts_with("//") {
            ancestor = ancestor.chars().skip(2).collect();
        }

        let query = &format!("{}[not(ancestor::{})]", xpath, ancestor);
        let node_vec = Util::evaluate_xpath(context, query, false)?;
        for mut node in node_vec {
            node.unlink();
        }
        Ok(())
    }
}
