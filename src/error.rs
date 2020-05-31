use failure::{Backtrace, Context, Error, Fail};
use std::fmt;

#[derive(Debug)]
pub struct ScraperError {
    inner: Context<ScraperErrorKind>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum ScraperErrorKind {
    #[fail(display = "libXml Error")]
    Xml,
    #[fail(display = "No content found")]
    Scrape,
    #[fail(display = "Url Error")]
    Url,
    #[fail(display = "Http request failed")]
    Http,
    #[fail(display = "Config Error")]
    Config,
    #[fail(display = "IO Error")]
    IO,
    #[fail(display = "Content-type suggest no html")]
    ContentType,
    #[fail(display = "Unknown Error")]
    Unknown,
}

impl Fail for ScraperError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for ScraperError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl ScraperError {
    pub fn kind(&self) -> ScraperErrorKind {
        *self.inner.get_context()
    }
}

impl From<ScraperErrorKind> for ScraperError {
    fn from(kind: ScraperErrorKind) -> ScraperError {
        ScraperError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<ScraperErrorKind>> for ScraperError {
    fn from(inner: Context<ScraperErrorKind>) -> ScraperError {
        ScraperError { inner }
    }
}

impl From<Error> for ScraperError {
    fn from(_: Error) -> ScraperError {
        ScraperError {
            inner: Context::new(ScraperErrorKind::Unknown),
        }
    }
}
