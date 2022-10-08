use failure::{Backtrace, Context, Error, Fail};
use std::fmt;

#[derive(Debug)]
pub struct ScraperError {
    inner: Context<ScraperErrorKind>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum ScraperErrorKind {
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
