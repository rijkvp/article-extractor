use crate::full_text_parser::{config::ConfigError, error::FullTextParserError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScraperError {
    #[error("")]
    Config(#[from] ConfigError),
    #[error("")]
    Scrap(#[from] FullTextParserError),
}
