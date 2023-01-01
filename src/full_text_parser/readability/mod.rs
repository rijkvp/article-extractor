mod constants;
mod state;

use libxml::tree::{Document, Node, NodeType};

use self::state::State;
use super::error::FullTextParserError;

pub struct Readability;

impl Readability {
    pub fn extract_body_readability(
        document: &Document,
        _root: &mut Node,
    ) -> Result<bool, FullTextParserError> {
        let mut state = State::default();
        let mut elements_to_score = Vec::new();
        let mut node: Option<Node> = document.clone().get_root_element();

        while let Some(node_ref) = node.as_mut() {
            let tag_name = node_ref.get_name().to_uppercase();
            let match_string = node_ref
                .get_class_names()
                .iter()
                .fold(String::new(), |a, b| format!("{a} {b}"));
            let match_string = match node_ref.get_property("id") {
                Some(id) => format!("{match_string} {id}"),
                None => match_string,
            };

            if !Self::is_probably_visible(node_ref) {
                node = Self::remove_and_next(node_ref);
                continue;
            }

            if Self::check_byline(node_ref, &match_string, &mut state) {
                node = Self::remove_and_next(node_ref);
                continue;
            }

            if state.should_remove_title_header && Self::header_duplicates_title(node_ref) {
                state.should_remove_title_header = false;
                node = Self::remove_and_next(node_ref);
                continue;
            }

            // Remove unlikely candidates
            if state.strip_unlikely {
                if constants::UNLIELY_CANDIDATES.is_match(&match_string)
                    && !constants::OKAY_MAYBE_ITS_A_CANDIDATE.is_match(&match_string)
                    && !Self::has_ancestor_tag(node_ref, "table", None)
                    && !Self::has_ancestor_tag(node_ref, "code", None)
                    && tag_name != "BODY"
                    && tag_name != "A"
                {
                    node = Self::remove_and_next(node_ref);
                    continue;
                }

                if let Some(role) = node_ref.get_attribute("role") {
                    if constants::UNLIKELY_ROLES.contains(&role.as_str()) {
                        node = Self::remove_and_next(node_ref);
                        continue;
                    }
                }
            }

            // Remove DIV, SECTION, and HEADER nodes without any content(e.g. text, image, video, or iframe).
            if tag_name == "DIV"
                || tag_name == "SECTION"
                || tag_name == "HEADER"
                || tag_name == "H1"
                || tag_name == "H2"
                || tag_name == "H3"
                || tag_name == "H4"
                || tag_name == "H5"
                || tag_name == "H6" && Self::is_element_without_content(node_ref)
            {
                node = Self::remove_and_next(node_ref);
                continue;
            }

            if constants::DEFAULT_TAGS_TO_SCORE.contains(&tag_name.as_str()) {
                elements_to_score.push(node_ref.clone());
            }

            // Turn all divs that don't have children block level elements into p's
            if tag_name == "DIV" {
                // Put phrasing content into paragraphs.
                let mut p: Option<Node> = None;
                for mut child_node in node_ref.get_child_nodes().into_iter() {
                    if Self::is_phrasing_content(&child_node) {
                        if let Some(p) = p.as_mut() {
                            let _ = p.add_child(&mut child_node);
                        } else if !Self::is_whitespace(&child_node) {
                            let mut new_node = Node::new("p", None, document).unwrap();
                            node_ref
                                .replace_child_node(new_node.clone(), child_node.clone())
                                .unwrap();
                            new_node.add_child(&mut child_node).unwrap();
                            p.replace(new_node);
                        }
                    } else if let Some(p) = p.as_mut() {
                        for mut r_node in p.get_child_nodes().into_iter().rev() {
                            if Self::is_whitespace(&r_node) {
                                r_node.unlink();
                            }
                        }
                    }
                }

                // Sites like http://mobile.slate.com encloses each paragraph with a DIV
                // element. DIVs with only a P element inside and no text content can be
                // safely converted into plain P elements to avoid confusing the scoring
                // algorithm with DIVs with are, in practice, paragraphs.
                if Self::has_single_tag_inside_element(node_ref, "P")
                    && Self::get_link_density(node_ref) < 0.25
                {}
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

    fn is_whitespace(node: &Node) -> bool {
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

    fn remove_and_next(node: &mut Node) -> Option<Node> {
        let next_node = Self::next_node(node, true);
        node.unlink();
        next_node
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

    fn check_byline(node: &Node, matchstring: &str, state: &mut State) -> bool {
        if state.byline.is_some() {
            return false;
        }

        let rel = node
            .get_attribute("rel")
            .map(|rel| rel == "author")
            .unwrap_or(false);
        let itemprop = node
            .get_attribute("itemprop")
            .map(|prop| prop.contains("author"))
            .unwrap_or(false);

        let content = node.get_content();
        if rel
            || itemprop
            || constants::BYLINE.is_match(matchstring) && Self::is_valid_byline(&content)
        {
            state.byline = Some(content.trim().into());
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
            constants::NORMALIZE.replace(&content, " ").into()
        } else {
            content
        }
    }

    fn text_similarity(a: &str, b: &str) -> f64 {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let tokens_a = constants::TOKENIZE.split(&a).collect::<Vec<_>>();
        let tokens_b = constants::TOKENIZE.split(&b).collect::<Vec<_>>();
        if tokens_a.is_empty() || tokens_b.is_empty() {
            return 0.0;
        }

        let tokens_b_total: f64 = tokens_b
            .iter()
            .map(|t| t.len())
            .fold(0.0, |a, b| a + b as f64);
        let uniq_tokens_b = tokens_b
            .into_iter()
            .filter(|token| !tokens_a.iter().any(|t| t == token))
            .collect::<Vec<_>>();
        let uniq_tokens_b_total: f64 = uniq_tokens_b
            .iter()
            .map(|t| t.len())
            .fold(0.0, |a, b| a + b as f64);

        let distance_b = uniq_tokens_b_total / tokens_b_total;
        1.0 - distance_b
    }

    fn has_ancestor_tag(node: &Node, tag_name: &str, max_depth: Option<u64>) -> bool {
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

    fn has_single_tag_inside_element(node: &Node, tag: &str) -> bool {
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

    fn is_element_without_content(node: &Node) -> bool {
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

    fn get_elements_by_tag_name(node: &Node, tag: &str) -> Vec<Node> {
        let tag = tag.to_uppercase();
        let all_tags = tag == "*";
        let mut vec = Vec::new();

        fn get_elems(node: &Node, tag: &str, vec: &mut Vec<Node>, all_tags: bool) {
            for child in node.get_child_elements() {
                if all_tags || child.get_name() == tag {
                    vec.push(child);
                }
                get_elems(node, tag, vec, all_tags);
            }
        }

        get_elems(node, &tag, &mut vec, all_tags);
        vec
    }

    fn is_phrasing_content(node: &Node) -> bool {
        let tag_name = node.get_name().to_uppercase();
        let is_text_node = node
            .get_type()
            .map(|t| t == NodeType::TextNode)
            .unwrap_or(false);

        is_text_node
            || constants::PHRASING_ELEMS.contains(&tag_name.as_str())
            || (tag_name == "A" || tag_name == "DEL" || tag_name == "INS")
                && node
                    .get_child_nodes()
                    .iter()
                    .map(Self::is_phrasing_content)
                    .all(|val| val)
    }

    fn get_link_density(node: &Node) -> f64 {
        let text_length = Self::get_inner_text(node, false).len();
        if text_length == 0 {
            return 0.0;
        }

        let mut link_length = 0.0;

        // XXX implement _reduceNodeList?
        let link_nodes = Self::get_elements_by_tag_name(node, "A");
        for link_node in link_nodes {
            if let Some(href) = link_node.get_attribute("href") {
                let coefficient = if constants::HASH_URL.is_match(&href) {
                    0.3
                } else {
                    1.0
                };
                link_length += Self::get_inner_text(&link_node, false).len() as f64 * coefficient;
            }
        }

        link_length / text_length as f64
    }
}
