use std::{fmt, io};

macro_rules! format_err {
    ($($tt:tt)*) => {
        crate::Error::new(format!($($tt)*))
    };
}

macro_rules! bail {
    ($($tt:tt)*) => {
        return Err(format_err!($($tt)*))
    };
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// An error that occurred during parsing the Dependabot configuration.
#[derive(Debug)]
pub struct Error(ErrorKind);

// Hiding error variants from a library's public error type to prevent
// dependency updates from becoming breaking changes.
// We can add `is_*` methods that indicate the kind of error if needed, but
// don't expose dependencies' types directly in the public API.
#[derive(Debug)]
pub(crate) enum ErrorKind {
    Env(std::env::VarError),
    Io(io::Error),
    Bool(std::str::ParseBoolError),
    Int(std::num::ParseIntError),
    String(std::string::FromUtf8Error),

    Process(crate::process::ProcessError),

    Toml(toml_edit::de::Error),
    CfgExprParse(cfg_expr::error::ParseError),
    CfgExprTargetHasAtomic(cfg_expr::error::HasAtomicParseError),

    Other(String),
    WithContext(String, Option<Box<Error>>),
}

impl Error {
    pub(crate) fn new(e: impl Into<ErrorKind>) -> Self {
        Self(e.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ErrorKind::Env(e) => fmt::Display::fmt(e, f),
            ErrorKind::Io(e) => fmt::Display::fmt(e, f),
            ErrorKind::Bool(e) => fmt::Display::fmt(e, f),
            ErrorKind::Int(e) => fmt::Display::fmt(e, f),
            ErrorKind::String(e) => fmt::Display::fmt(e, f),
            ErrorKind::Process(e) => fmt::Display::fmt(e, f),
            ErrorKind::Toml(e) => fmt::Display::fmt(e, f),
            ErrorKind::CfgExprParse(e) => fmt::Display::fmt(e, f),
            ErrorKind::CfgExprTargetHasAtomic(e) => fmt::Display::fmt(e, f),
            ErrorKind::Other(e) | ErrorKind::WithContext(e, ..) => fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            ErrorKind::Env(e) => Some(e),
            ErrorKind::Io(e) => Some(e),
            ErrorKind::Bool(e) => Some(e),
            ErrorKind::Int(e) => Some(e),
            ErrorKind::String(e) => Some(e),
            ErrorKind::Process(e) => Some(e),
            ErrorKind::Toml(e) => Some(e),
            ErrorKind::CfgExprParse(e) => Some(e),
            ErrorKind::CfgExprTargetHasAtomic(e) => Some(e),
            ErrorKind::Other(_) => None,
            ErrorKind::WithContext(_, e) => Some(e.as_ref()?),
        }
    }
}

impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        match e.0 {
            ErrorKind::Env(e) => Self::new(io::ErrorKind::Other, e),
            ErrorKind::Io(e) => e,
            ErrorKind::Bool(e) => Self::new(io::ErrorKind::Other, e),
            ErrorKind::Int(e) => Self::new(io::ErrorKind::Other, e),
            ErrorKind::String(e) => Self::new(io::ErrorKind::Other, e),
            ErrorKind::Process(e) => Self::new(io::ErrorKind::Other, e),
            ErrorKind::Toml(e) => Self::new(io::ErrorKind::InvalidData, e),
            ErrorKind::CfgExprParse(e) => Self::new(io::ErrorKind::Other, e),
            ErrorKind::CfgExprTargetHasAtomic(e) => Self::new(io::ErrorKind::Other, e),
            ErrorKind::Other(e) | ErrorKind::WithContext(e, ..) => {
                Self::new(io::ErrorKind::Other, e)
            }
        }
    }
}

impl From<Error> for ErrorKind {
    fn from(e: Error) -> Self {
        e.0
    }
}
impl From<String> for ErrorKind {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}
impl From<&str> for ErrorKind {
    fn from(s: &str) -> Self {
        Self::Other(s.to_owned())
    }
}
impl From<std::env::VarError> for ErrorKind {
    fn from(e: std::env::VarError) -> Self {
        Self::Env(e)
    }
}
impl From<io::Error> for ErrorKind {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<std::str::ParseBoolError> for ErrorKind {
    fn from(e: std::str::ParseBoolError) -> Self {
        Self::Bool(e)
    }
}
impl From<std::num::ParseIntError> for ErrorKind {
    fn from(e: std::num::ParseIntError) -> Self {
        Self::Int(e)
    }
}
impl From<std::string::FromUtf8Error> for ErrorKind {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Self::String(e)
    }
}
impl From<crate::process::ProcessError> for ErrorKind {
    fn from(e: crate::process::ProcessError) -> Self {
        Self::Process(e)
    }
}
impl From<toml_edit::de::Error> for ErrorKind {
    fn from(e: toml_edit::de::Error) -> Self {
        Self::Toml(e)
    }
}
impl From<cfg_expr::error::ParseError> for ErrorKind {
    fn from(e: cfg_expr::error::ParseError) -> Self {
        Self::CfgExprParse(e)
    }
}
impl From<cfg_expr::error::HasAtomicParseError> for ErrorKind {
    fn from(e: cfg_expr::error::HasAtomicParseError) -> Self {
        Self::CfgExprTargetHasAtomic(e)
    }
}

impl From<std::env::VarError> for Error {
    fn from(e: std::env::VarError) -> Self {
        Self::new(e)
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::new(e)
    }
}

// Note: These implementations are intentionally not-exist to prevent dependency
// updates from becoming breaking changes.
// impl From<toml_edit::de::Error> for Error
// impl From<cfg_expr::error::ParseError> for Error
// impl From<cfg_expr::error::HasAtomicParseError> for Error

// Inspired by anyhow::Context.
pub(crate) trait Context<T, E> {
    fn context<C>(self, context: C) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static;
    fn with_context<C, F>(self, context: F) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}
impl<T, E> Context<T, E> for Result<T, E>
where
    E: Into<ErrorKind>,
{
    fn context<C>(self, context: C) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        match self {
            Ok(ok) => Ok(ok),
            Err(e) => Err(Error(ErrorKind::WithContext(
                context.to_string(),
                Some(Box::new(Error(e.into()))),
            ))),
        }
    }
    fn with_context<C, F>(self, context: F) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        match self {
            Ok(ok) => Ok(ok),
            Err(e) => Err(Error(ErrorKind::WithContext(
                context().to_string(),
                Some(Box::new(Error(e.into()))),
            ))),
        }
    }
}
impl<T> Context<T, std::convert::Infallible> for Option<T> {
    fn context<C>(self, context: C) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        match self {
            Some(ok) => Ok(ok),
            None => Err(Error(ErrorKind::WithContext(context.to_string(), None))),
        }
    }
    fn with_context<C, F>(self, context: F) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        match self {
            Some(ok) => Ok(ok),
            None => Err(Error(ErrorKind::WithContext(context().to_string(), None))),
        }
    }
}
