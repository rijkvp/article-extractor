use super::super::ScraperErrorKind;
use failure::{Backtrace, Context, Error, Fail};
use std::fmt;

#[derive(Debug)]
pub struct ImageDownloadError {
    inner: Context<ImageDownloadErrorKind>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum ImageDownloadErrorKind {
    #[fail(display = "Parsing the supplied html string failed")]
    HtmlParse,
    #[fail(display = "Scaling down a downloaded image failed")]
    ImageScale,
    #[fail(display = "Downloading the parent element of an image failed")]
    ParentDownload,
    #[fail(display = "Generating image name failed")]
    ImageName,
    #[fail(display = "Getting the content-length property failed")]
    ContentLenght,
    #[fail(display = "Content-type suggest no image")]
    ContentType,
    #[fail(display = "Http error")]
    Http,
    #[fail(display = "IO error")]
    IO,
    #[fail(display = "Invalid URL")]
    InvalidUrl,
    #[fail(display = "Unknown Error")]
    Unknown,
}

impl Fail for ImageDownloadError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for ImageDownloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl ImageDownloadError {
    pub fn kind(&self) -> ImageDownloadErrorKind {
        *self.inner.get_context()
    }
}

impl From<ImageDownloadErrorKind> for ImageDownloadError {
    fn from(kind: ImageDownloadErrorKind) -> ImageDownloadError {
        ImageDownloadError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<ImageDownloadErrorKind>> for ImageDownloadError {
    fn from(inner: Context<ImageDownloadErrorKind>) -> ImageDownloadError {
        ImageDownloadError { inner }
    }
}

impl From<ScraperErrorKind> for ImageDownloadError {
    fn from(kind: ScraperErrorKind) -> ImageDownloadError {
        let kind = match kind {
            ScraperErrorKind::Xml => ImageDownloadErrorKind::HtmlParse,
            _ => ImageDownloadErrorKind::Unknown,
        };

        ImageDownloadError {
            inner: Context::new(kind),
        }
    }
}

impl From<Error> for ImageDownloadError {
    fn from(_: Error) -> ImageDownloadError {
        ImageDownloadError {
            inner: Context::new(ImageDownloadErrorKind::Unknown),
        }
    }
}
