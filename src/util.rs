use libxml::{
    tree::{Node, NodeType},
    xpath::Context,
};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Response,
};
use tokio::fs::DirEntry;

use crate::{
    constants,
    full_text_parser::{config::ConfigEntry, error::FullTextParserError},
};

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
                node.get_attribute("href")
                    .expect("already checked for href")
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

    pub fn get_attribute(
        context: &Context,
        xpath: &str,
        attribute: &str,
    ) -> Result<String, FullTextParserError> {
        Util::evaluate_xpath(context, xpath, false)?
            .iter()
            .find_map(|node| node.get_attribute(attribute))
            .ok_or(FullTextParserError::Xml)
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

    pub fn is_probably_visible(node: &Node) -> bool {
        let display_none = node
            .get_attribute("display")
            .map(|display| display == "none")
            .unwrap_or(false);
        let is_hidden = node.has_attribute("hidden");
        let aria_hidden = node
            .get_attribute("aria-hidden")
            .map(|attr| attr == "true")
            .unwrap_or(false);
        let has_fallback_image = node.get_class_names().contains("fallback-image");

        !display_none && !is_hidden && !aria_hidden || has_fallback_image
    }

    pub fn is_whitespace(node: &Node) -> bool {
        let is_text_node = node
            .get_type()
            .map(|t| t == NodeType::TextNode)
            .unwrap_or(false);
        let is_element_node = node
            .get_type()
            .map(|t| t == NodeType::ElementNode)
            .unwrap_or(false);

        (is_text_node && node.get_content().trim().is_empty())
            || (is_element_node && node.get_name().to_uppercase() == "BR")
    }

    pub fn remove_and_next(node: &mut Node) -> Option<Node> {
        let next_node = Self::next_node(node, true);
        node.unlink();
        next_node
    }

    pub fn next_node(node: &Node, ignore_self_and_kids: bool) -> Option<Node> {
        let mut node = node.clone();

        // First check for kids if those aren't being ignored
        let first_child = node.get_first_child();
        if !ignore_self_and_kids && first_child.is_some() {
            return first_child;
        }

        // Then for siblings...
        let next_sibling = node.get_next_sibling();
        if next_sibling.is_some() {
            return next_sibling;
        }

        // And finally, move up the parent chain *and* find a sibling
        // (because this is depth-first traversal, we will have already
        // seen the parent nodes themselves).
        loop {
            let parent = node.get_parent();
            if parent.is_none() {
                break;
            }

            if let Some(parent) = parent {
                let parent_name = parent.get_name().to_uppercase();
                if parent_name == "HTML" {
                    break;
                }

                let next_sibling = parent.get_next_sibling();
                if next_sibling.is_some() {
                    return next_sibling;
                } else {
                    node = parent;
                }
            }
        }

        None
    }

    pub fn get_inner_text(node: &Node, normalize_spaces: bool) -> String {
        let content = node.get_content().trim().to_owned();
        if normalize_spaces {
            constants::NORMALIZE.replace(&content, " ").into()
        } else {
            content
        }
    }

    pub fn text_similarity(a: &str, b: &str) -> f64 {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let tokens_a = constants::TOKENIZE.split(&a).collect::<Vec<_>>();
        let tokens_b = constants::TOKENIZE.split(&b).collect::<Vec<_>>();
        if tokens_a.is_empty() || tokens_b.is_empty() {
            return 0.0;
        }

        let tokens_b_total = tokens_b.join(" ").len() as f64;
        let uniq_tokens_b = tokens_b
            .into_iter()
            .filter(|token| !tokens_a.iter().any(|t| t == token))
            .collect::<Vec<_>>();
        let uniq_tokens_b_total = uniq_tokens_b.join(" ").len() as f64;

        let distance_b = uniq_tokens_b_total / tokens_b_total;
        1.0 - distance_b
    }

    pub fn has_ancestor_tag(node: &Node, tag_name: &str, max_depth: Option<u64>) -> bool {
        let max_depth = max_depth.unwrap_or(3);
        let tag_name = tag_name.to_uppercase();
        let mut depth = 0;
        let mut node = node.get_parent();

        loop {
            if depth > max_depth {
                return false;
            }

            let tmp_node = match node {
                Some(node) => node,
                None => return false,
            };

            if tmp_node.get_name() == tag_name {
                return true;
            }

            node = tmp_node.get_parent();
            depth += 1;
        }
    }

    pub fn has_single_tag_inside_element(node: &Node, tag: &str) -> bool {
        // There should be exactly 1 element child with given tag
        if node.get_child_nodes().len() == 1
            || node
                .get_child_nodes()
                .first()
                .map(|n| n.get_name().to_uppercase() == tag)
                .unwrap_or(false)
        {
            return false;
        }

        // And there should be no text nodes with real content
        node.get_child_nodes().iter().any(|n| {
            n.get_type()
                .map(|t| t == NodeType::TextNode)
                .unwrap_or(false)
                && constants::HAS_CONTENT.is_match(&n.get_content())
        })
    }

    pub fn is_element_without_content(node: &Node) -> bool {
        if let Some(node_type) = node.get_type() {
            let len = node.get_child_nodes().len();

            return node_type == NodeType::ElementNode
                && node.get_content().trim().is_empty()
                && (len == 0
                    || len
                        == Self::get_elements_by_tag_name(node, "br").len()
                            + Self::get_elements_by_tag_name(node, "hr").len());
        }

        false
    }

    pub fn get_elements_by_tag_name(node: &Node, tag: &str) -> Vec<Node> {
        let tag = tag.to_uppercase();
        let all_tags = tag == "*";
        let mut vec = Vec::new();

        fn get_elems(node: &Node, tag: &str, vec: &mut Vec<Node>, all_tags: bool) {
            for child in node.get_child_elements() {
                if all_tags || child.get_name().to_uppercase() == tag {
                    vec.push(child.clone());
                }
                get_elems(&child, tag, vec, all_tags);
            }
        }

        get_elems(node, &tag, &mut vec, all_tags);
        vec
    }

    pub fn get_link_density(node: &Node) -> f64 {
        let text_length = Util::get_inner_text(node, false).len();
        if text_length == 0 {
            return 0.0;
        }

        let mut link_length = 0.0;

        // XXX implement _reduceNodeList?
        let link_nodes = Util::get_elements_by_tag_name(node, "A");
        for link_node in link_nodes {
            if let Some(href) = link_node.get_attribute("href") {
                let coefficient = if constants::HASH_URL.is_match(&href) {
                    0.3
                } else {
                    1.0
                };
                link_length += Util::get_inner_text(&link_node, false).len() as f64 * coefficient;
            }
        }

        link_length / text_length as f64
    }

    // Determine whether element has any children block level elements.
    pub fn has_child_block_element(node: &Node) -> bool {
        node.get_child_elements().iter().any(|node| {
            constants::DIV_TO_P_ELEMS.contains(node.get_name().as_str())
                || Self::has_child_block_element(node)
        })
    }

    pub fn get_node_ancestors(node: &Node, max_depth: u64) -> Vec<Node> {
        let mut ancestors = Vec::new();
        let mut node = node.clone();

        for _ in 0..=max_depth {
            let parent = node.get_parent();
            match parent {
                Some(parent) => {
                    ancestors.push(parent.clone());
                    node = parent;
                }
                None => return ancestors,
            }
        }

        ancestors
    }

    pub fn has_tag_name(node: Option<&Node>, tag_name: &str) -> bool {
        node.map(|n| n.get_name().to_uppercase() == tag_name.to_uppercase())
            .unwrap_or(false)
    }

    // Check if node is image, or if node contains exactly only one image
    // whether as a direct child or as its descendants.
    pub fn is_single_image(node: &Node) -> bool {
        if node.get_name().to_uppercase() == "IMG" {
            true
        } else if node.get_child_nodes().len() != 1 || node.get_content().trim() != "" {
            false
        } else if let Some(first_child) = node.get_child_nodes().first() {
            Self::is_single_image(first_child)
        } else {
            false
        }
    }
}
