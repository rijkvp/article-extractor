use libxml::tree::Node;
use reqwest::{header::CONTENT_TYPE, Client, Response, Url};

use super::ImageDownloadError;

pub struct ImageRequest {
    node: Node,
    http_response: Option<Response>,
    content_length: u64,
    content_type: String,
}

impl ImageRequest {
    pub async fn new(node: Node, url: &Url, client: &Client) -> Result<Self, ImageDownloadError> {
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|_| ImageDownloadError::Http)?;

        let content_type = Self::get_content_type(&response)?;
        let content_length = Self::get_content_length(&response)?;

        if !content_type.contains("image") {
            return Err(ImageDownloadError::ContentType);
        }

        Ok(Self {
            node,
            http_response: Some(response),
            content_length,
            content_type,
        })
    }

    pub async fn download(&mut self) -> Result<Vec<u8>, ImageDownloadError> {
        if let Some(http_response) = self.http_response.take() {
            let result = http_response
                .bytes()
                .await
                .map_err(|_| ImageDownloadError::Http)?
                .as_ref()
                .to_vec();
            Ok(result)
        } else {
            log::warn!("imagerequest already consumed");
            Err(ImageDownloadError::Http)
        }
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn content_length(&self) -> u64 {
        self.content_length
    }

    pub fn write_image_to_property(&mut self, prop_name: &str, data: &str) {
        _ = self.node.set_property(prop_name, data);
    }

    fn get_content_length(response: &Response) -> Result<u64, ImageDownloadError> {
        let status_code = response.status();

        if !status_code.is_success() {
            log::warn!("response: {status_code}");
            return Err(ImageDownloadError::Http);
        }

        response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|content_length| content_length.to_str().ok())
            .and_then(|content_length| content_length.parse::<u64>().ok())
            .ok_or(ImageDownloadError::ContentLength)
    }

    fn get_content_type(response: &Response) -> Result<String, ImageDownloadError> {
        if response.status().is_success() {
            response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|val| val.to_str().ok())
                .map(|val| val.to_string())
                .ok_or(ImageDownloadError::ContentType)
        } else {
            Err(ImageDownloadError::ContentType)
        }
    }
}
