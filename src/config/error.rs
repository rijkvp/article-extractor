use failure::{Context, Fail, Backtrace, Error};
use std::fmt;

#[derive(Debug)]
pub struct ConfigError {
    inner: Context<ConfigErrorKind>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum ConfigErrorKind {
    #[fail(display = "IO Error")]
    IO,
    #[fail(display = "Config does not contain body xpath")]
    BadConfig,
    #[fail(display = "Unknown Error")]
    Unknown,
}

impl Fail for ConfigError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

// impl ConfigError {
//     pub fn kind(&self) -> ConfigErrorKind {
//         *self.inner.get_context()
//     }
// }

impl From<ConfigErrorKind> for ConfigError {
    fn from(kind: ConfigErrorKind) -> ConfigError {
        ConfigError { inner: Context::new(kind) }
    }
}

impl From<Context<ConfigErrorKind>> for ConfigError {
    fn from(inner: Context<ConfigErrorKind>) -> ConfigError {
        ConfigError { inner: inner }
    }
}

impl From<Error> for ConfigError {
    fn from(_: Error) -> ConfigError {
        ConfigError { inner: Context::new(ConfigErrorKind::Unknown) }
    }
}