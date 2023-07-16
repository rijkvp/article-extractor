use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImageDownloadError {
    #[error("Parsing the supplied html string failed")]
    HtmlParse,
    #[error("Scaling down a downloaded image failed")]
    ImageScale,
    #[error("Downloading the parent element of an image failed")]
    ParentDownload,
    #[error("Generating image name failed")]
    ImageName,
    #[error("Getting the content-length property failed")]
    ContentLength,
    #[error("Content-type suggest no image")]
    ContentType,
    #[error("Http error")]
    Http,
    #[error("IO error")]
    IO,
    #[error("Invalid URL")]
    InvalidUrl(#[from] url::ParseError),
    #[error("Unknown Error")]
    Unknown,
}

impl From<reqwest::Error> for ImageDownloadError {
    fn from(_value: reqwest::Error) -> Self {
        Self::Http
    }
}
