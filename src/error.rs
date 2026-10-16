//qompassai/qompass-hold/src/error.rs
use std::{
    fmt,
    io::{self, ErrorKind},
};

use zbus::{
    DBusError, fdo,
    message::{Header, Message},
    names::ErrorName,
};

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

// zbus 5 requires method error types to implement DBusError.
// The trait delegates to the hand-written mapping above so the
// error names clients see (including the NotFound → NoSuchObject
// special case) are exactly the ones dbus_error_name has always
// produced; the reply body is the error's Display text.
impl DBusError for Error {
    fn create_reply(&self, msg: &Header<'_>) -> zbus::Result<Message> {
        Message::error(msg, self.name())?.build(&self.to_string())
    }

    fn name(&self) -> ErrorName<'_> {
        self.dbus_error_name()
    }

    fn description(&self) -> Option<&str> {
        // call the inherent method, not this trait method
        Error::description(self)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn name(err: &Error) -> String {
        err.dbus_error_name().to_string()
    }

    // Validation: every variant maps to its documented D-Bus name.
    // These strings are the wire contract Secret Service clients see.
    #[test]
    fn dbus_error_names_for_simple_variants() {
        assert_eq!(
            name(&Error::NotInitialized),
            "me.grimsteel.PassSecretService.PassNotInitialized"
        );
        assert_eq!(
            name(&Error::InvalidSession),
            "org.freedesktop.Secret.Error.NoSession"
        );
        assert_eq!(
            name(&Error::PermissionDenied),
            "org.freedesktop.DBus.Error.AccessDenied"
        );
        assert_eq!(
            name(&Error::GpgError("boom".to_owned())),
            "me.grimsteel.PassSecretService.GPGError"
        );
    }

    // Validation + adversarial: the I/O mapping distinguishes
    // NotFound (a missing secret is a normal lookup outcome) from
    // every other I/O failure — conflating them would misreport
    // real errors to clients as "no such object".
    #[test]
    fn dbus_error_name_distinguishes_not_found() {
        let not_found: Error = io::Error::from(io::ErrorKind::NotFound).into();
        assert_eq!(
            name(&not_found),
            "org.freedesktop.Secret.Error.NoSuchObject"
        );
        let other: Error = io::Error::from(io::ErrorKind::PermissionDenied).into();
        assert_eq!(name(&other), "org.freedesktop.DBus.Error.IOError");
    }

    // Validation: Display strings stay human-meaningful per variant.
    #[test]
    fn display_includes_variant_context() {
        assert_eq!(Error::NotInitialized.to_string(), "Pass is not initialized");
        assert_eq!(Error::PermissionDenied.to_string(), "Access denied");
        assert!(
            Error::GpgError("bad key".to_owned())
                .to_string()
                .contains("bad key")
        );
    }

    // Adversarial: description() must not invent text for variants
    // that carry none, and must surface the GPG message it does carry.
    #[test]
    fn description_only_where_carried() {
        assert_eq!(Error::NotInitialized.description(), None);
        assert_eq!(Error::PermissionDenied.description(), None);
        assert_eq!(
            Error::GpgError("expired".to_owned()).description(),
            Some("expired")
        );
    }

    // Validation: Some passes through into_not_found unchanged.
    // Adversarial: None becomes the NotFound error (not a panic,
    // not a generic failure), with the NoSuchObject D-Bus name.
    #[test]
    fn option_into_not_found() {
        assert_eq!(Some(7).into_not_found().unwrap(), 7);
        let err = None::<u8>.into_not_found().unwrap_err();
        assert_eq!(
            err.dbus_error_name().to_string(),
            "org.freedesktop.Secret.Error.NoSuchObject"
        );
    }

    // Validation: the DBusError trait surface (what zbus 5 actually
    // sends) reports the same name as dbus_error_name and the same
    // description as the inherent method — the two must never drift.
    #[test]
    fn dbus_error_trait_delegates_to_inherent_mapping() {
        let err = Error::GpgError("expired".to_owned());
        assert_eq!(
            DBusError::name(&err).as_str(),
            "me.grimsteel.PassSecretService.GPGError"
        );
        assert_eq!(DBusError::description(&err), Some("expired"));
        let err = Error::NotInitialized;
        assert_eq!(
            DBusError::name(&err).as_str(),
            "me.grimsteel.PassSecretService.PassNotInitialized"
        );
        assert_eq!(DBusError::description(&err), None);
    }

    // Validation: the fdo conversion keeps PermissionDenied as the
    // distinct AccessDenied error clients check for.
    #[test]
    fn fdo_conversion_preserves_access_denied() {
        let fdo_err = fdo::Error::from(Error::PermissionDenied);
        assert!(matches!(fdo_err, fdo::Error::AccessDenied(_)));
        let fdo_err = fdo::Error::from(Error::InvalidSession);
        assert!(matches!(fdo_err, fdo::Error::Failed(_)));
    }
}
