use thiserror::Error;

#[derive(Error, Debug)]
pub enum FullTextParserError {
    #[error("libXml Error")]
    Xml,
    #[error("No content found")]
    Scrape,
    #[error("Url Error")]
    Url(#[from] url::ParseError),
    #[error("Http request failed")]
    Http,
    #[error("Config Error")]
    Config,
    #[error("IO Error")]
    IO,
    #[error("Content-type suggest no html")]
    ContentType,
    #[error("Unknown Error")]
    Unknown,
}
