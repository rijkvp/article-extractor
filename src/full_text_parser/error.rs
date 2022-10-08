use failure::{Backtrace, Context, Error, Fail};
use std::fmt;

#[derive(Debug)]
pub struct FullTextParserError {
    inner: Context<FullTextParserErrorKind>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum FullTextParserErrorKind {
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

impl Fail for FullTextParserError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for FullTextParserError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl FullTextParserError {
    pub fn kind(&self) -> FullTextParserErrorKind {
        *self.inner.get_context()
    }
}

impl From<FullTextParserErrorKind> for FullTextParserError {
    fn from(kind: FullTextParserErrorKind) -> FullTextParserError {
        FullTextParserError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<FullTextParserErrorKind>> for FullTextParserError {
    fn from(inner: Context<FullTextParserErrorKind>) -> FullTextParserError {
        FullTextParserError { inner }
    }
}

impl From<Error> for FullTextParserError {
    fn from(_: Error) -> FullTextParserError {
        FullTextParserError {
            inner: Context::new(FullTextParserErrorKind::Unknown),
        }
    }
}
