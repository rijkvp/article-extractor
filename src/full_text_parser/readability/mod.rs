mod state;

#[cfg(test)]
mod tests;

use std::cmp::Ordering;

use libxml::tree::{Document, Node, NodeType};

use self::state::State;
use super::error::FullTextParserError;
use crate::{constants, util::Util};

pub struct Readability;

impl Readability {
    pub fn extract_body(
        document: Document,
        root: &mut Node,
        title: Option<&str>,
    ) -> Result<bool, FullTextParserError> {
        let mut state = State::default();
        let mut document = document;
        let mut attempts: Vec<(Node, usize, Document)> = Vec::new();
        let document_cache = document
            .dup()
            .map_err(|()| FullTextParserError::Readability)?;

        loop {
            let mut elements_to_score = Vec::new();
            let mut node: Option<Node> = document.clone().get_root_element();

            while let Some(node_ref) = node.as_mut() {
                let tag_name = node_ref.get_name().to_uppercase();

                if tag_name == "TEXT" && node_ref.get_content().trim().is_empty() {
                    node = Util::remove_and_next(node_ref);
                    continue;
                }

                let match_string = node_ref
                    .get_class_names()
                    .iter()
                    .fold(String::new(), |a, b| format!("{a} {b}"));
                let match_string = match node_ref.get_property("id") {
                    Some(id) => format!("{match_string} {id}"),
                    None => match_string,
                };

                if !Util::is_probably_visible(node_ref) {
                    node = Util::remove_and_next(node_ref);
                    continue;
                }

                if Self::check_byline(node_ref, &match_string, &mut state) {
                    node = Util::remove_and_next(node_ref);
                    continue;
                }

                if state.should_remove_title_header
                    && Self::header_duplicates_title(node_ref, title)
                {
                    state.should_remove_title_header = false;
                    node = Util::remove_and_next(node_ref);
                    continue;
                }

                // Remove unlikely candidates
                if state.strip_unlikely {
                    if constants::UNLIELY_CANDIDATES.is_match(&match_string)
                        && !constants::OKAY_MAYBE_ITS_A_CANDIDATE.is_match(&match_string)
                        && !Util::has_ancestor_tag(
                            node_ref,
                            "table",
                            None,
                            None::<fn(&Node) -> bool>,
                        )
                        && !Util::has_ancestor_tag(
                            node_ref,
                            "code",
                            None,
                            None::<fn(&Node) -> bool>,
                        )
                        && tag_name != "BODY"
                        && tag_name != "A"
                    {
                        node = Util::remove_and_next(node_ref);
                        continue;
                    }

                    if let Some(role) = node_ref.get_attribute("role") {
                        if constants::UNLIKELY_ROLES.contains(&role.as_str()) {
                            node = Util::remove_and_next(node_ref);
                            continue;
                        }
                    }
                }

                // Remove DIV, SECTION, and HEADER nodes without any content(e.g. text, image, video, or iframe).
                if (tag_name == "DIV"
                    || tag_name == "SECTION"
                    || tag_name == "HEADER"
                    || tag_name == "H1"
                    || tag_name == "H2"
                    || tag_name == "H3"
                    || tag_name == "H4"
                    || tag_name == "H5"
                    || tag_name == "H6")
                    && Util::is_element_without_content(node_ref)
                {
                    node = Util::remove_and_next(node_ref);
                    continue;
                }

                if constants::DEFAULT_TAGS_TO_SCORE.contains(&tag_name.as_str()) {
                    elements_to_score.push(node_ref.clone());
                }

                // Turn all divs that don't have children block level elements into p's
                if tag_name == "DIV" {
                    // Put phrasing content into paragraphs.
                    let mut p: Option<Node> = None;
                    for mut child in node_ref.get_child_nodes().into_iter() {
                        if Self::is_phrasing_content(&child) {
                            if let Some(p) = p.as_mut() {
                                child.unlink();
                                p.add_child(&mut child).map_err(|error| {
                                    log::error!("{error}");
                                    FullTextParserError::Readability
                                })?;
                            } else if !Util::is_whitespace(&child) {
                                let mut new_node = Node::new("p", None, &document)
                                    .map_err(|()| FullTextParserError::Readability)?;
                                let mut old_node = node_ref
                                    .replace_child_node(new_node.clone(), child)
                                    .map_err(|error| {
                                        log::error!("{error}");
                                        FullTextParserError::Readability
                                    })?;

                                new_node.add_child(&mut old_node).map_err(|error| {
                                    log::error!("{error}");
                                    FullTextParserError::Readability
                                })?;
                                p.replace(new_node);
                            }
                        } else if p.is_some() {
                            if let Some(p) = p.as_mut() {
                                for mut r_node in p.get_child_nodes().into_iter().rev() {
                                    if Util::is_whitespace(&r_node) {
                                        r_node.unlink();
                                        continue;
                                    }
                                    break;
                                }
                            }
                            _ = p.take();
                        }
                    }

                    // Sites like http://mobile.slate.com encloses each paragraph with a DIV
                    // element. DIVs with only a P element inside and no text content can be
                    // safely converted into plain P elements to avoid confusing the scoring
                    // algorithm with DIVs with are, in practice, paragraphs.
                    if Util::has_single_tag_inside_element(node_ref, "P")
                        && Util::get_link_density(node_ref) < 0.25
                    {
                        if let Some(new_node) = node_ref.get_child_nodes().first() {
                            if let Some(mut parent) = node_ref.get_parent() {
                                parent
                                    .replace_child_node(new_node.clone(), node_ref.clone())
                                    .map_err(|error| {
                                        log::error!("{error}");
                                        FullTextParserError::Readability
                                    })?;
                                node = Some(new_node.clone());
                                elements_to_score.push(new_node.clone());
                                continue;
                            }
                        }
                    } else if !Util::has_child_block_element(node_ref)
                        && node_ref.set_name("P").is_ok()
                    {
                        elements_to_score.push(node_ref.clone());
                    }
                }

                node = Util::next_node(node_ref, false);
            }

            let mut candidates = Vec::new();
            // Loop through all paragraphs, and assign a score to them based on how content-y they look.
            // Then add their score to their parent node.
            // A score is determined by things like number of commas, class names, etc. Maybe eventually link density.
            for element_to_score in elements_to_score.drain(..) {
                if element_to_score.get_parent().is_none() {
                    continue;
                }

                let inner_text = Util::get_inner_text(&element_to_score, true);

                // If this paragraph is less than 25 characters, don't even count it.
                if inner_text.len() < 25 {
                    continue;
                }

                // Exclude nodes with no ancestor.
                let ancestors = Util::get_node_ancestors(&element_to_score, Some(5));
                if ancestors.is_empty() {
                    continue;
                }

                let mut content_score = 0.0;

                // Add a point for the paragraph itself as a base.
                content_score += 1.0;

                // Add points for any commas within this paragraph.
                content_score += inner_text.split(',').count() as f64;

                // For every 100 characters in this paragraph, add another point. Up to 3 points.
                content_score += f64::min(f64::floor(inner_text.len() as f64 / 100.0), 3.0);

                // Initialize and score ancestors.
                for (level, mut ancestor) in ancestors.into_iter().enumerate() {
                    if ancestor.get_parent().is_none() {
                        continue;
                    }

                    if Self::get_content_score(&ancestor).is_none() {
                        Self::initialize_node(&mut ancestor, &state)?;
                        candidates.push(ancestor.clone());
                    }

                    // Node score divider:
                    // - parent:             1 (no division)
                    // - grandparent:        2
                    // - great grandparent+: ancestor level * 3
                    let score_divider = if level == 0 {
                        1.0
                    } else if level == 1 {
                        2.0
                    } else {
                        level as f64 * 3.0
                    };

                    if let Some(mut score) = Self::get_content_score(&ancestor) {
                        score += content_score / score_divider;
                        Self::set_content_score(&mut ancestor, score)?;
                    }
                }
            }

            // After we've calculated scores, loop through all of the possible
            // candidate nodes we found and find the one with the highest score.
            for candidate in candidates.iter_mut() {
                // Scale the final candidates score based on link density. Good content
                // should have a relatively small link density (5% or less) and be mostly
                // unaffected by this operation.
                if let Some(content_score) = Self::get_content_score(candidate) {
                    let candidate_score = content_score * (1.0 - Util::get_link_density(candidate));
                    Self::set_content_score(candidate, candidate_score)?;
                }
            }

            candidates.sort_by(|a, b| {
                if let (Some(a), Some(b)) = (Self::get_content_score(a), Self::get_content_score(b))
                {
                    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
                } else {
                    Ordering::Equal
                }
            });

            let top_candidates = candidates.into_iter().take(5).collect::<Vec<_>>();
            // for candidate in top_candidates.iter() {
            //     println!("candidate: {} {:?}", candidate.get_name(), candidate.get_attributes());
            // }
            let mut needed_to_create_top_candidate = false;
            let mut top_candidate = top_candidates.first().cloned().unwrap_or_else(|| {
                // If we still have no top candidate, just use the body as a last resort.
                // We also have to copy the body node so it is something we can modify.
                let mut rt = document.get_root_element().expect("doc should have root");
                Self::initialize_node(&mut rt, &state).expect("init should not fail");
                needed_to_create_top_candidate = true;
                rt
            });

            let mut alternative_candidate_ancestors = Vec::new();
            // Find a better top candidate node if it contains (at least three) nodes which belong to `topCandidates` array
            // and whose scores are quite closed with current `topCandidate` node.
            if let Some(top_score) = Self::get_content_score(&top_candidate) {
                for candidate in top_candidates.iter().skip(1) {
                    let score = Self::get_content_score(candidate).unwrap_or(0.0);
                    if score / top_score >= 0.75 {
                        alternative_candidate_ancestors
                            .push(Util::get_node_ancestors(candidate, None));
                    }
                }
            }

            if alternative_candidate_ancestors.len() >= constants::MINIMUM_TOPCANDIDATES {
                let mut parent_of_top_candidate = top_candidate.get_parent();

                while let Some(parent) = &parent_of_top_candidate {
                    if parent.get_name().to_uppercase() == "BODY" {
                        break;
                    }

                    let mut lists_containing_this_ancestor = 0;
                    let tmp = usize::min(
                        alternative_candidate_ancestors.len(),
                        constants::MINIMUM_TOPCANDIDATES,
                    );
                    for ancestors in alternative_candidate_ancestors.iter().take(tmp) {
                        lists_containing_this_ancestor +=
                            ancestors.into_iter().filter(|n| n == &parent).count();
                    }

                    if lists_containing_this_ancestor >= constants::MINIMUM_TOPCANDIDATES {
                        top_candidate = parent.clone();
                        break;
                    }

                    parent_of_top_candidate = parent_of_top_candidate.and_then(|n| n.get_parent());
                }
            }

            if Self::get_content_score(&top_candidate).is_none() {
                Self::initialize_node(&mut top_candidate, &state)?;
            }

            // Because of our bonus system, parents of candidates might have scores
            // themselves. They get half of the node. There won't be nodes with higher
            // scores than our topCandidate, but if we see the score going *up* in the first
            // few steps up the tree, that's a decent sign that there might be more content
            // lurking in other places that we want to unify in. The sibling stuff
            // below does some of that - but only if we've looked high enough up the DOM
            // tree.
            let mut parent_of_top_candidate = top_candidate.get_parent();
            let mut last_score = Self::get_content_score(&top_candidate).unwrap_or(0.0);

            // let html = document.to_string_with_options(libxml::tree::SaveOptions {
            //     format: true,
            //     no_declaration: false,
            //     no_empty_tags: true,
            //     no_xhtml: false,
            //     xhtml: false,
            //     as_xml: false,
            //     as_html: true,
            //     non_significant_whitespace: false,
            // });
            // std::fs::write("doc.html", &html).unwrap();

            // The scores shouldn't get too low.
            let score_threshold = last_score / 3.0;

            while !Util::has_tag_name(parent_of_top_candidate.as_ref(), "BODY") {
                if parent_of_top_candidate
                    .as_ref()
                    .map(|n| Self::get_content_score(n).is_none())
                    .unwrap_or(false)
                {
                    parent_of_top_candidate = parent_of_top_candidate.and_then(|n| n.get_parent());
                    continue;
                }

                let parent_score = parent_of_top_candidate
                    .as_ref()
                    .and_then(Self::get_content_score)
                    .unwrap_or(0.0);
                if parent_score < score_threshold {
                    break;
                }

                if parent_score > last_score {
                    // Alright! We found a better parent to use.
                    if let Some(parent) = parent_of_top_candidate {
                        top_candidate = parent;
                    }
                    break;
                }

                last_score = parent_of_top_candidate
                    .as_ref()
                    .and_then(Self::get_content_score)
                    .unwrap_or(0.0);
                parent_of_top_candidate = parent_of_top_candidate.and_then(|n| n.get_parent());
            }

            // If the top candidate is the only child, use parent instead. This will help sibling
            // joining logic when adjacent content is actually located in parent's sibling node.
            parent_of_top_candidate = top_candidate.get_parent();

            while Util::has_tag_name(parent_of_top_candidate.as_ref(), "BODY")
                && parent_of_top_candidate
                    .as_ref()
                    .map(|n| n.get_child_elements().len() == 1)
                    .unwrap_or(false)
            {
                top_candidate = parent_of_top_candidate.ok_or(FullTextParserError::Readability)?;
                parent_of_top_candidate = top_candidate.get_parent();
            }

            if Self::get_content_score(&top_candidate).is_none() {
                Self::initialize_node(&mut top_candidate, &state)?;
            }

            // Now that we have the top candidate, look through its siblings for content
            // that might also be related. Things like preambles, content split by ads
            // that we removed, etc.
            let mut article_content =
                Node::new("DIV", None, &document).map_err(|()| FullTextParserError::Readability)?;

            let sibling_score_threshold = f64::max(
                10.0,
                Self::get_content_score(&top_candidate).unwrap_or(0.0) * 0.2,
            );
            // Keep potential top candidate's parent node to try to get text direction of it later.
            parent_of_top_candidate = top_candidate.get_parent();
            let siblings = parent_of_top_candidate
                .as_ref()
                .map(|n| n.get_child_nodes());

            if let Some(siblings) = siblings {
                for mut sibling in siblings {
                    let mut append = false;

                    let score = Self::get_content_score(&sibling).unwrap_or(0.0);
                    log::debug!("Looking at sibling node: {sibling:?} with score {score}");

                    if top_candidate == sibling {
                        append = true;
                    } else {
                        let mut content_bonus = 0.0;

                        // Give a bonus if sibling nodes and top candidates have the example same classname
                        let sibling_classes = sibling.get_class_names();
                        let tc_classes = top_candidate.get_class_names();

                        if !tc_classes.is_empty()
                            && !sibling_classes.is_empty()
                            && sibling_classes
                                .iter()
                                .all(|class| tc_classes.contains(class))
                        {
                            content_bonus +=
                                Self::get_content_score(&top_candidate).unwrap_or(0.0) * 0.2;
                        }

                        if score + content_bonus >= sibling_score_threshold {
                            append = true;
                        } else if sibling.get_name().to_uppercase() == "P" {
                            let link_density = Util::get_link_density(&sibling);
                            let node_content = Util::get_inner_text(&sibling, false);
                            let node_length = node_content.len();

                            if node_length > 80
                                && (link_density < 0.25
                                    || (node_length > 0
                                        && link_density == 0.0
                                        && constants::SIBLING_CONTENT.is_match(&node_content)))
                            {
                                append = true;
                            }
                        }
                    }

                    if append {
                        log::debug!("Appending node: {sibling:?}");

                        if !constants::ALTER_TO_DIV_EXCEPTIONS
                            .contains(sibling.get_name().to_uppercase().as_str())
                        {
                            // We have a node that isn't a common block level element, like a form or td tag.
                            // Turn it into a div so it doesn't get filtered out later by accident.
                            log::debug!("Altering sibling: {sibling:?} to div.");

                            sibling.set_name("DIV").map_err(|error| {
                                log::error!("{error}");
                                FullTextParserError::Readability
                            })?;
                        }

                        sibling.unlink();
                        article_content.add_child(&mut sibling).map_err(|error| {
                            log::error!("{error}");
                            FullTextParserError::Readability
                        })?;
                    }
                }
            }

            if needed_to_create_top_candidate {
                // We already created a fake div thing, and there wouldn't have been any siblings left
                // for the previous loop, so there's no point trying to create a new div, and then
                // move all the children over. Just assign IDs and class names here. No need to append
                // because that already happened anyway.
                top_candidate
                    .set_property("id", "readability-page-1")
                    .map_err(|error| {
                        log::error!("{error}");
                        FullTextParserError::Readability
                    })?;
            } else {
                let mut div = Node::new("DIV", None, &document)
                    .map_err(|()| FullTextParserError::Readability)?;
                div.set_property("id", "readability-page-1")
                    .map_err(|error| {
                        log::error!("{error}");
                        FullTextParserError::Readability
                    })?;

                for mut child in article_content.get_child_nodes() {
                    child.unlink();
                    div.add_child(&mut child).map_err(|error| {
                        log::error!("{error}");
                        FullTextParserError::Readability
                    })?;
                }
                article_content.add_child(&mut div).map_err(|error| {
                    log::error!("{error}");
                    FullTextParserError::Readability
                })?;
            }

            let mut parse_successful = true;

            // Now that we've gone through the full algorithm, check to see if
            // we got any meaningful content. If we didn't, we may need to re-run
            // grabArticle with different flags set. This gives us a higher likelihood of
            // finding the content, and the sieve approach gives us a higher likelihood of
            // finding the -right- content.
            let text = Util::get_inner_text(&article_content, true);
            let text_length = text.len();

            if text_length < constants::DEFAULT_CHAR_THRESHOLD {
                parse_successful = false;

                if state.strip_unlikely {
                    state.strip_unlikely = false;
                    attempts.push((article_content, text_length, document));
                } else if state.weigh_classes {
                    state.weigh_classes = false;
                    attempts.push((article_content, text_length, document));
                } else if state.clean_conditionally {
                    state.clean_conditionally = false;
                    attempts.push((article_content, text_length, document));
                } else {
                    attempts.push((article_content, text_length, document));
                    // No luck after removing flags, just return the longest text we found during the different loops

                    attempts.sort_by(|(_, size_a, _), (_, size_b, _)| size_a.cmp(size_b));

                    // But first check if we actually have something
                    if let Some((best_attempt, _len, _document)) = attempts.pop() {
                        for mut child in best_attempt.get_child_nodes() {
                            child.unlink();
                            root.add_child(&mut child).map_err(|error| {
                                log::error!("{error}");
                                FullTextParserError::Readability
                            })?;
                        }
                        parse_successful = true;
                    }

                    return Ok(parse_successful);
                }

                document = document_cache
                    .dup()
                    .map_err(|()| FullTextParserError::Readability)?;
            } else {
                for mut child in article_content.get_child_nodes() {
                    child.unlink();
                    root.add_child(&mut child).map_err(|error| {
                        log::error!("{error}");
                        FullTextParserError::Readability
                    })?;
                }
                return Ok(parse_successful);
            }
        }
    }

    fn get_content_score(node: &Node) -> Option<f64> {
        node.get_attribute(constants::SCORE_ATTR)
            .and_then(|a| a.parse::<f64>().ok())
    }

    fn set_content_score(node: &mut Node, score: f64) -> Result<(), FullTextParserError> {
        node.set_attribute(constants::SCORE_ATTR, &score.to_string())
            .map_err(|err| {
                log::error!("failed to set content score: {err}");
                FullTextParserError::Readability
            })
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
    fn header_duplicates_title(node: &Node, title: Option<&str>) -> bool {
        let name = node.get_name().to_lowercase();
        if name != "h1" && name != "h2" {
            return false;
        }
        let heading = Util::get_inner_text(node, false);

        if let Some(title) = title {
            Util::text_similarity(&heading, title) > 0.75
        } else {
            false
        }
    }

    fn is_phrasing_content(node: &Node) -> bool {
        let tag_name = node.get_name().to_uppercase();
        let is_text_node = node
            .get_type()
            .map(|t| t == NodeType::TextNode)
            .unwrap_or(false);

        is_text_node
            || constants::PHRASING_ELEMS.contains(&tag_name.as_str())
            || ((tag_name == "A" || tag_name == "DEL" || tag_name == "INS")
                && node.get_child_nodes().iter().all(Self::is_phrasing_content))
    }

    // Initialize a node with the readability object. Also checks the
    // className/id for special names to add to its score.
    fn initialize_node(node: &mut Node, state: &State) -> Result<(), FullTextParserError> {
        let score = match node.get_name().to_uppercase().as_str() {
            "DIV" => 5,
            "PRE" | "TD" | "BLOCKQUITE" => 3,
            "ADDRESS" | "OL" | "UL" | "DL" | "DD" | "DT" | "LI" | "FORM" => -3,
            "H1" | "H2" | "H3" | "H4" | "H5" | "H6" | "TH" => -5,
            _ => 0,
        };
        let class_weight = if state.weigh_classes {
            Util::get_class_weight(node)
        } else {
            0
        };
        let score = score + class_weight;
        Self::set_content_score(node, score as f64)?;
        Ok(())
    }
}
