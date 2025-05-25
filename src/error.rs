//qompassai/qompass-hold/src/error.rs
use std::{
    fmt,
    io::{self, ErrorKind},
};

use zbus::{fdo, names::ErrorName};

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    DbusError(zbus::Error),
    RedbError(redb::Error),
    GpgError(String),
    // pass is not initialized
    NotInitialized,
    InvalidSession,
    PermissionDenied,
}

// Implement Display for Error (required for std::error::Error)
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::IoError(e) => write!(f, "I/O Error: {e}"),
            Error::DbusError(e) => write!(f, "D-Bus Error: {e}"),
            Error::RedbError(e) => write!(f, "ReDB Error: {e}"),
            Error::GpgError(e) => write!(f, "GPG Error: {e}"),
            Error::NotInitialized => write!(f, "Pass is not initialized"),
            Error::InvalidSession => write!(f, "Invalid secret service session"),
            Error::PermissionDenied => write!(f, "Access denied"),
        }
    }
}

impl std::error::Error for Error {}

// Conversion traits for ergonomic error handling
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::IoError(value)
    }
}

impl From<zbus::Error> for Error {
    fn from(value: zbus::Error) -> Self {
        Self::DbusError(value)
    }
}

impl From<redb::Error> for Error {
    fn from(value: redb::Error) -> Self {
        Self::RedbError(value)
    }
}

// Allow using `?` in D-Bus handlers returning zbus::fdo::Error
impl From<Error> for fdo::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::IoError(e) => fdo::Error::IOError(e.to_string()),
            Error::DbusError(e) => fdo::Error::Failed(e.to_string()),
            Error::RedbError(e) => fdo::Error::Failed(e.to_string()),
            Error::GpgError(e) => fdo::Error::Failed(e),
            Error::NotInitialized => fdo::Error::Failed("Pass is not initialized".to_string()),
            Error::InvalidSession => fdo::Error::Failed("Invalid session".to_string()),
            Error::PermissionDenied => fdo::Error::AccessDenied("Access denied".to_string()),
        }
    }
}

impl Error {
    pub fn dbus_error_name(&self) -> ErrorName<'_> {
        ErrorName::from_static_str_unchecked(match self {
            Error::IoError(e) if e.kind() == ErrorKind::NotFound => {
                "org.freedesktop.Secret.Error.NoSuchObject"
            }
            Error::IoError(_) => "org.freedesktop.DBus.Error.IOError",
            Error::DbusError(_) => "org.freedesktop.zbus.Error",
            Error::RedbError(_) => "me.grimsteel.PassSecretService.ReDBError",
            Error::GpgError(_) => "me.grimsteel.PassSecretService.GPGError",
            Error::NotInitialized => "me.grimsteel.PassSecretService.PassNotInitialized",
            Error::InvalidSession => "org.freedesktop.Secret.Error.NoSession",
            Error::PermissionDenied => "org.freedesktop.DBus.Error.AccessDenied",
        })
    }
    pub fn description(&self) -> Option<&str> {
        match self {
            Error::DbusError(zbus::Error::MethodError(_, desc, _)) => desc.as_deref(),
            Error::GpgError(e) => Some(e.as_str()),
            _ => None,
        }
    }
}

// Your project-wide Result alias
pub type Result<T = ()> = std::result::Result<T, Error>;

// Utility traits/macros
pub trait IntoResult<T> {
    fn into_result(self) -> Result<T>;
}

impl<T, E: Into<redb::Error>> IntoResult<T> for std::result::Result<T, E> {
    fn into_result(self) -> Result<T> {
        self.map_err(|e| Into::<redb::Error>::into(e).into())
    }
}

pub trait OptionNoneNotFound<T> {
    fn into_not_found(self) -> Result<T>;
}

impl<T> OptionNoneNotFound<T> for Option<T> {
    fn into_not_found(self) -> Result<T> {
        self.ok_or(io::Error::from(io::ErrorKind::NotFound).into())
    }
}

macro_rules! raise_nonexistent_table {
    ($expression:expr) => {
        raise_nonexistent_table!(
            $expression,
            Err(io::Error::from(io::ErrorKind::NotFound).into())
        )
    };
    ($expression:expr, $default:expr) => {
        match $expression {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => {
                return $default;
            }
            Err(e) => return Err(e).into_result(),
        }
    };
}
pub(crate) use raise_nonexistent_table;
