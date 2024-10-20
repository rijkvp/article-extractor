use crate::{full_text_parser::error::FullTextParserError, util::Util};
use libxml::tree::Node;
use url::Url;

#[derive(Debug, Clone)]
pub struct ImageObject {
    width: Option<u32>,
    height: Option<u32>,
    url: Option<Url>,
    description: Option<String>,
    name: Option<String>,
}

impl ImageObject {
    pub fn parse_node(node: &Node) -> Option<Self> {
        if node.get_name().to_uppercase() != "DIV" {
            return None;
        }

        let item_prop_image = node
            .get_attribute("itemprop")
            .map(|prop| prop == "image")
            .unwrap_or(false);
        let item_type_image = node
            .get_attribute("itemtype")
            .map(|attr| attr == "https://schema.org/ImageObject")
            .unwrap_or(false);

        if !item_prop_image && !item_type_image {
            return None;
        }

        let meta_nodes = Util::get_elements_by_tag_name(node, "meta");

        let mut width = None;
        let mut height = None;
        let mut url = None;
        let mut description = None;
        let mut name = None;

        for meta_node in meta_nodes {
            let item_prop = meta_node.get_attribute("itemprop");
            let content_prop = meta_node.get_attribute("content");

            if let (Some(item_prop), Some(content_prop)) = (item_prop, content_prop) {
                if item_prop == "width" {
                    width = content_prop.parse::<u32>().ok();
                } else if item_prop == "height" {
                    height = content_prop.parse::<u32>().ok();
                } else if item_prop == "url" {
                    url = Url::parse(&content_prop).ok();
                } else if item_prop == "description" {
                    description = Some(content_prop);
                } else if item_prop == "name" {
                    name = Some(content_prop);
                }
            }
        }

        url.as_ref()?;

        Some(Self {
            width,
            height,
            url,
            description,
            name,
        })
    }

    pub fn replace(&self, node: &mut Node) -> Result<(), FullTextParserError> {
        if node.is_null() {
            return Err(FullTextParserError::Xml);
        }

        let mut parent = node.get_parent().ok_or(FullTextParserError::Xml)?;

        if parent.get_name().to_uppercase() == "A" {
            return self.replace(&mut parent);
        }

        node.unlink();

        let mut root = parent
            .new_child(None, "imageobject")
            .map_err(|_| FullTextParserError::Xml)?;

        let mut a = root
            .new_child(None, "a")
            .map_err(|_| FullTextParserError::Xml)?;

        let mut img = a
            .new_child(None, "img")
            .map_err(|_| FullTextParserError::Xml)?;

        if let Some(width) = self.width {
            _ = img.set_attribute("width", &width.to_string());
        }

        if let Some(height) = self.height {
            _ = img.set_attribute("height", &height.to_string());
        }

        if let Some(description) = self.description.as_deref() {
            _ = img.set_attribute("alt", description);
        }

        if let Some(name) = self.name.as_deref() {
            _ = img.set_attribute("title", name);
        }

        if let Some(url) = self.url.as_ref() {
            _ = a.set_attribute("href", url.as_str());
            _ = img.set_attribute("src", url.as_str());
        }

        Ok(())
    }
}
