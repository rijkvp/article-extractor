use futures::StreamExt;
use reqwest::{Client, Response};
use tokio::sync::mpsc::Sender;

use crate::util::Util;

use super::{image_data::ImageData, ImageDownloadError};

#[derive(Debug)]
pub struct ImageRequest {
    url: String,
    response: Response,
    content_length: usize,
    content_type: String,
}

impl ImageRequest {
    pub async fn new(url: String, client: &Client) -> Result<Self, ImageDownloadError> {
        let response = client.get(&url).send().await?;

        let content_type = Util::get_content_type(&response)?;
        let content_length = Util::get_content_length(&response)?;

        if !content_type.contains("image") {
            return Err(ImageDownloadError::ContentType);
        }

        Ok(Self {
            url,
            response,
            content_length,
            content_type,
        })
    }

    pub async fn download(self, tx: &Sender<usize>) -> Result<ImageData, ImageDownloadError> {
        let mut stream = self.response.bytes_stream();

        let mut result = Vec::with_capacity(self.content_length);
        while let Some(item) = stream.next().await {
            let chunk = item?;
            _ = tx.send(chunk.len()).await;
            for byte in chunk {
                result.push(byte);
            }
        }

        Ok(ImageData {
            url: self.url,
            data: result,
            content_length: self.content_length,
            content_type: self.content_type,
        })
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn content_length(&self) -> usize {
        self.content_length
    }
}
