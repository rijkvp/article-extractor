use futures::StreamExt;
use libxml::tree::Node;
use reqwest::{header::CONTENT_TYPE, Client, Response, Url};
use tokio::sync::mpsc::Sender;

use super::ImageDownloadError;

pub struct ImageRequest {
    node: Node,
    http_response: Option<Response>,
    content_length: usize,
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

    pub async fn download(&mut self, tx: &Sender<usize>) -> Result<Vec<u8>, ImageDownloadError> {
        if let Some(http_response) = self.http_response.take() {
            let mut stream = http_response.bytes_stream();

            let mut result = Vec::with_capacity(self.content_length);
            while let Some(item) = stream.next().await {
                let chunk = item.map_err(|_| ImageDownloadError::Http)?;
                _ = tx.send(chunk.len()).await;
                for byte in chunk {
                    result.push(byte);
                }
            }

            Ok(result)
        } else {
            log::warn!("imagerequest already consumed");
            Err(ImageDownloadError::Http)
        }
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn content_length(&self) -> usize {
        self.content_length
    }

    pub fn write_image_to_property(&mut self, prop_name: &str, data: &str) {
        _ = self.node.set_property(prop_name, data);
    }

    fn get_content_length(response: &Response) -> Result<usize, ImageDownloadError> {
        let status_code = response.status();

        if !status_code.is_success() {
            log::warn!("response: {status_code}");
            return Err(ImageDownloadError::Http);
        }

        response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|content_length| content_length.to_str().ok())
            .and_then(|content_length| content_length.parse::<usize>().ok())
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
