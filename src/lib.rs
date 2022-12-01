mod article;
mod error;
mod full_text_parser;
pub mod images;
mod readability;
mod util;

use article::Article;
use error::ScraperError;
use full_text_parser::FullTextParser;
use images::ImageDownloader;
use readability::Readability;
use reqwest::Client;
use std::path::Path;

pub struct ArticleScraper {
    full_text_parser: FullTextParser,
    readability: Readability,
    image_downloader: ImageDownloader,
}

impl ArticleScraper {
    pub async fn new(user_configs: Option<&Path>) -> Self {
        Self {
            full_text_parser: FullTextParser::new(user_configs).await,
            readability: Readability::new(),
            image_downloader: ImageDownloader::new((2048, 2048)),
        }
    }

    pub async fn parse(
        &self,
        url: &url::Url,
        download_images: bool,
        client: &Client,
    ) -> Result<Article, ScraperError> {
        let res = self.full_text_parser.parse(url, client).await;

        if download_images {
            // if let Err(error) = self
            //     .image_downloader
            //     .download_images_from_context(&context, client)
            //     .await
            // {
            //     log::error!("Downloading images failed: '{}'", error);
            // }
        }

        unimplemented!()
    }
}
