mod article;
mod constants;
mod error;
mod full_text_parser;
pub mod images;
mod util;

use crate::images::Progress;
use article::Article;
use error::ScraperError;
pub use full_text_parser::config::ConfigEntry as FtrConfigEntry;
pub use full_text_parser::FullTextParser;
pub use full_text_parser::Readability;
use images::ImageDownloader;
use reqwest::Client;
use std::path::Path;
use tokio::sync::mpsc::Sender;

pub struct ArticleScraper {
    full_text_parser: FullTextParser,
    image_downloader: ImageDownloader,
}

impl ArticleScraper {
    pub async fn new(user_configs: Option<&Path>) -> Self {
        Self {
            full_text_parser: FullTextParser::new(user_configs).await,
            image_downloader: ImageDownloader::new((2048, 2048)),
        }
    }

    pub async fn parse(
        &self,
        url: &url::Url,
        download_images: bool,
        client: &Client,
        progress: Option<Sender<Progress>>,
    ) -> Result<Article, ScraperError> {
        let res = self.full_text_parser.parse(url, client).await?;

        if download_images {
            if let Some(document) = res.document.as_ref() {
                let _image_res = self
                    .image_downloader
                    .download_images_from_document(document, client, progress)
                    .await;
            }
        }

        Ok(res)
    }
}
