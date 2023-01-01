mod regex;
mod state;

use libxml::tree::{Document, Node};

use self::state::State;
use super::error::FullTextParserError;

pub struct Readability;

impl Readability {
    pub fn extract_body_readability(
        document: &Document,
        root: &mut Node,
    ) -> Result<bool, FullTextParserError> {
        let mut state = State::default();
        let mut node: Option<Node> = document.clone().get_root_element();

        while let Some(node_ref) = node.as_mut() {

            let match_string = node_ref.get_class_names().iter().fold(String::new(), |a, b| format!("{a} {b}"));

            if !Self::is_probably_visible(node_ref) {
                node = Self::remove_and_next(node_ref);
                continue;
            }

            if Self::check_byline(node_ref, &match_string) {
                node = Self::remove_and_next(node_ref);
                continue;
            }

            if state.should_remove_title_header && Self::header_duplicates_title(node_ref) {
                state.should_remove_title_header = false;
                node = Self::remove_and_next(node_ref);
                continue;
            }

            if state.strip_unlikely {
                
            }

            node = Self::next_node(node_ref, false);
        }

        unimplemented!()
    }

    fn is_probably_visible(node: &Node) -> bool {
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

    fn remove_and_next(node: &mut Node) -> Option<Node> {
        let next_node = Self::next_node(node, true);
        node.unlink();
        return next_node;
    }

    fn next_node(node: &Node, ignore_self_and_kids: bool) -> Option<Node> {
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
                let next_sibling = parent.get_next_sibling();
                if next_sibling.is_some() {
                    return next_sibling;
                }
            }
        }

        None
    }

    fn check_byline(node: &Node, matchstring: &str) -> bool {
        let rel = node
            .get_attribute("rel")
            .map(|rel| rel == "author")
            .unwrap_or(false);
        let itemprop = node
            .get_attribute("itemprop")
            .map(|prop| prop.contains("author"))
            .unwrap_or(false);

        let content = node.get_content();
        if rel || itemprop || regex::BYLINE.is_match(matchstring) && Self::is_valid_byline(&content) {
            // FIXME
            true
        } else {
            false
        }
    }

    // Check whether the input string could be a byline.
    // This verifies that the input length is less than 100 chars.
    fn is_valid_byline(line: &str) -> bool {
        let len = line.trim().len();
        len > 0 && len < 100
    }

    // Check if this node is an H1 or H2 element whose content is mostly
    // the same as the article title.
    fn header_duplicates_title(node: &Node) -> bool {
        let name = node.get_name().to_lowercase();
        if name != "h1" || name != "h2" {
            return false;
        }
        let heading = Self::get_inner_text(node, false);
        Self::text_similarity(&heading, "FIXME") > 0.75
    }

    fn get_inner_text(node: &Node, normalize_spaces: bool) -> String {
        let content = node.get_content().trim().to_owned();
        if normalize_spaces {
            regex::NORMALIZE.replace(&content, " ").into()
        } else {
            content
        }
    }

    fn text_similarity(a: &str, b: &str) -> f64 {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let tokens_a = regex::TOKENIZE.split(&a).collect::<Vec<_>>();
        let tokens_b = regex::TOKENIZE.split(&b).collect::<Vec<_>>();
        if tokens_a.iter().count() == 0 || tokens_b.iter().count() == 0 {
            return 0.0;
        }

        let tokens_b_total: f64 = tokens_b.iter().map(|t| t.len()).fold(0.0, |a, b| a + b as f64);
        let uniq_tokens_b = tokens_b.into_iter().filter(|token| !tokens_a.iter().any(|t| t == token)).collect::<Vec<_>>();
        let uniq_tokens_b_total: f64 = uniq_tokens_b.iter().map(|t| t.len()).fold(0.0, |a, b| a + b as f64);
        
        let distance_b = uniq_tokens_b_total / tokens_b_total;
        1.0 - distance_b
    }
}
