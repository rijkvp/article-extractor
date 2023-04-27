//! # article scraper
//!
//! The `article_scraper` crate provides a simple way to extract meaningful content from the web.
//! It contains two ways of locating the desired content
//!
//! ## 1. Rust implementation of [Full-Text RSS](https://www.fivefilters.org/full-text-rss/)
//!
//! This makes use of website specific extraction rules. Which has the advantage of fast & accurate results.
//! The disadvantages however are: the config needs to be updated as the website changes and a new extraction rule is needed for every website.
//!
//! A central repository of extraction rules and information about writing your own rules can be found here: [ftr-site-config](https://github.com/fivefilters/ftr-site-config).
//! Please consider contributing new rules or updates to it.
//!
//! `article_scraper` embeds all the rules in the ftr-site-config repository for convenience. Custom and updated rules can be loaded from a `user_configs` path.
//!
//! ## 2. Mozilla Readability
//!
//! In case the ftr-config based extraction fails the [mozilla Readability](https://github.com/mozilla/readability) algorithm will be used as a fall-back.
//! This re-implementation tries to mimic the original as closely as possible.
//!
//! # Example
//!
//! ```
//! use article_scraper::ArticleScraper;
//! use url::Url;
//! use reqwest::Client;
//!
//! async fn demo() {
//!     let scraper = ArticleScraper::new(None).await;
//!     let url = Url::parse("https://www.nytimes.com/interactive/2023/04/21/science/parrots-video-chat-facetime.html").unwrap();
//!     let client = Client::new();
//!     let article = scraper.parse(&url, false, &client, None).await.unwrap();
//!     let html = article.get_doc_content();
//! }
//! ```

mod article;
pub mod clean;
mod constants;
mod error;
mod full_text_parser;
#[doc(hidden)]
pub mod images;
mod util;
mod video_object;

use crate::images::Progress;
use article::Article;
use error::ScraperError;
#[doc(hidden)]
pub use full_text_parser::config::ConfigEntry as FtrConfigEntry;
#[doc(hidden)]
pub use full_text_parser::FullTextParser;
pub use full_text_parser::Readability;
use images::ImageDownloader;
use reqwest::Client;
use std::path::Path;
use tokio::sync::mpsc::Sender;

/// Download & extract meaningful content from websites
///
/// Rust implementation of [Full-Text RSS](https://www.fivefilters.org/full-text-rss/) with an additional fallback
/// of mozilla Readability.
///
/// For detailed information about extraction rules and how to contribute new rules please see
/// [ftr-site-config](https://github.com/fivefilters/ftr-site-config).
pub struct ArticleScraper {
    full_text_parser: FullTextParser,
    image_downloader: ImageDownloader,
}

impl ArticleScraper {
    /// Crate a new ArticleScraper
    ///
    /// # Arguments
    ///
    /// * `user_configs` - optional path to a folder containing additional ftr config files
    ///
    pub async fn new(user_configs: Option<&Path>) -> Self {
        Self {
            full_text_parser: FullTextParser::new(user_configs).await,
            image_downloader: ImageDownloader::new((2048, 2048)),
        }
    }

    /// Download & extract content of a website
    ///
    /// # Arguments
    ///
    /// * `url` - Url to an article
    /// * `download_images` - if images should be downloaded & embedded into the HTML
    /// * `client` - reqwest HTTP client to use
    /// * `progress` - optional progress notifications (only for image downloads)
    ///
    /// # Examples
    ///
    /// ```
    /// use article_scraper::ArticleScraper;
    /// use url::Url;
    /// use reqwest::Client;
    ///
    /// async fn demo() {
    ///     let scraper = ArticleScraper::new(None).await;
    ///     let url = Url::parse("https://www.nytimes.com/interactive/2023/04/21/science/parrots-video-chat-facetime.html").unwrap();
    ///     let client = Client::new();
    ///     let article = scraper.parse(&url, false, &client, None).await.unwrap();
    ///     let html = article.get_doc_content();
    /// }
    /// ```
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
