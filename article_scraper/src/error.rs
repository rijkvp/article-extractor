use crate::{
    full_text_parser::{config::ConfigError, error::FullTextParserError},
    images::ImageDownloadError,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScraperError {
    #[error("")]
    Config(#[from] ConfigError),
    #[error("")]
    Image(#[from] ImageDownloadError),
    #[error("")]
    Scrap(#[from] FullTextParserError),
}
