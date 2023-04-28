use crate::{full_text_parser::error::FullTextParserError, util::Util};
use libxml::tree::Node;
use url::Url;

pub struct VideoObject {
    thumbnail_url: Option<String>,
    content_url: Option<Url>,
    embed_url: Option<Url>,
    description: Option<String>,
    name: Option<String>,
}

impl VideoObject {
    pub fn parse_node(node: &Node) -> Option<Self> {
        if node.get_name().to_uppercase() != "DIV" {
            return None;
        }

        let item_prop_video = node
            .get_attribute("itemprop")
            .map(|prop| prop == "video")
            .unwrap_or(false);
        let item_type_video = node
            .get_attribute("itemtype")
            .map(|attr| attr == "http://schema.org/VideoObject")
            .unwrap_or(false);

        if !item_prop_video && !item_type_video {
            return None;
        }

        let meta_nodes = Util::get_elements_by_tag_name(node, "meta");

        let mut thumbnail_url = None;
        let mut content_url = None;
        let mut embed_url = None;
        let mut description = None;
        let mut name = None;

        for meta_node in meta_nodes {
            let item_prop = meta_node.get_attribute("itemprop");
            let content_prop = meta_node.get_attribute("content");

            if let (Some(item_prop), Some(content_prop)) = (item_prop, content_prop) {
                if item_prop == "thumbnailUrl" {
                    thumbnail_url = Some(content_prop);
                } else if item_prop == "contentURL" {
                    content_url = Self::parse_url(&content_prop);
                } else if item_prop == "embedURL" {
                    embed_url = Self::parse_url(&content_prop);
                } else if item_prop == "description" {
                    description = Some(content_prop);
                } else if item_prop == "name" {
                    name = Some(content_prop);
                }
            }
        }

        if thumbnail_url.is_none()
            && content_url.is_none()
            && embed_url.is_none()
            && description.is_none()
            && name.is_none()
        {
            return None;
        }

        Some(Self {
            thumbnail_url,
            content_url,
            embed_url,
            description,
            name,
        })
    }

    fn parse_url(url: &str) -> Option<Url> {
        let url = if url.starts_with("//") {
            format!("https:{url}")
        } else {
            url.into()
        };

        Url::parse(&url)
            .map_err(|err| log::error!("parse video object url: {err}"))
            .ok()
    }

    pub fn replace(&self, node: &mut Node) -> Result<(), FullTextParserError> {
        let mut parent = node.get_parent().ok_or(FullTextParserError::Xml)?;
        node.unlink();

        let mut root = parent
            .new_child(None, "videoobject")
            .map_err(|_| FullTextParserError::Xml)?;

        if let Some(name) = self.name.as_deref() {
            let mut title = root
                .new_child(None, "h3")
                .map_err(|_| FullTextParserError::Xml)?;
            _ = title.set_content(name);
        }

        if self.name != self.description {
            if let Some(description) = self.description.as_deref() {
                let mut desc = root
                    .new_child(None, "p")
                    .map_err(|_| FullTextParserError::Xml)?;
                _ = desc.set_content(description);
            }
        }

        let mut a = root
            .new_child(None, "a")
            .map_err(|_| FullTextParserError::Xml)?;
        if let Some(embed_url) = self.embed_url.as_ref() {
            _ = a.set_attribute("href", embed_url.as_str());
        } else if let Some(content_url) = self.content_url.as_ref() {
            _ = a.set_attribute("href", content_url.as_str());
        }

        let mut img = a
            .new_child(None, "img")
            .map_err(|_| FullTextParserError::Xml)?;
        if let Some(thumbnail_url) = self.thumbnail_url.as_deref() {
            _ = img.set_attribute("src", thumbnail_url);
        }

        Ok(())
    }
}
