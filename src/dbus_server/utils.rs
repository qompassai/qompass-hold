use std::{fmt::Display, io, time::SystemTime};

use serde::{Deserialize, Serialize};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Type};

pub const EMPTY_PATH: ObjectPath = ObjectPath::from_static_str_unchecked("/");

pub fn collection_path<T: Display>(collection_id: T) -> Option<ObjectPath<'static>> {
    ObjectPath::try_from(format!(
        "/org/freedesktop/secrets/collection/{collection_id}"
    ))
    .ok()
}
pub fn secret_path<T: Display>(collection_id: T, secret_id: T) -> Option<ObjectPath<'static>> {
    ObjectPath::try_from(format!(
        "/org/freedesktop/secrets/collection/{collection_id}/{secret_id}"
    ))
    .ok()
}
pub fn secret_alias_path<T: Display>(alias: T, secret_id: T) -> Option<ObjectPath<'static>> {
    ObjectPath::try_from(format!(
        "/org/freedesktop/secrets/aliases/{alias}/{secret_id}"
    ))
    .ok()
}
pub fn alias_path<T: Display>(alias: T) -> Option<ObjectPath<'static>> {
    ObjectPath::try_from(format!("/org/freedesktop/secrets/aliases/{alias}")).ok()
}
pub fn session_path<T: Display>(session_id: T) -> Option<ObjectPath<'static>> {
    ObjectPath::try_from(format!("/org/freedesktop/secrets/session/{session_id}")).ok()
}
pub fn try_interface<T>(result: zbus::Result<T>) -> zbus::Result<Option<T>> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(zbus::Error::InterfaceNotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn time_to_int(time: io::Result<SystemTime>) -> u64 {
    time.ok()
        // return 0 for times before the epoch or for platforms where this isn't supported
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|t| t.as_secs())
        .unwrap_or_default()
}

#[derive(Type, Debug, Deserialize, Serialize, PartialEq)]
pub struct Secret {
    pub session: OwnedObjectPath,
    pub parameters: Vec<u8>,
    pub value: Vec<u8>,
    pub content_type: String,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    // Validation: path builders produce the Secret Service layout.
    #[test]
    fn path_builders_match_secret_service_layout() {
        assert_eq!(
            collection_path("default").unwrap().as_str(),
            "/org/freedesktop/secrets/collection/default"
        );
        assert_eq!(
            secret_path("default", "s3cr3t").unwrap().as_str(),
            "/org/freedesktop/secrets/collection/default/s3cr3t"
        );
        assert_eq!(
            alias_path("login").unwrap().as_str(),
            "/org/freedesktop/secrets/aliases/login"
        );
        assert_eq!(
            secret_alias_path("login", "s3cr3t").unwrap().as_str(),
            "/org/freedesktop/secrets/aliases/login/s3cr3t"
        );
        assert_eq!(
            session_path("abc").unwrap().as_str(),
            "/org/freedesktop/secrets/session/abc"
        );
    }

    // Adversarial: identifiers containing characters D-Bus forbids
    // in object paths (spaces, dashes, empty) must yield None,
    // never a malformed path another component trusts.
    #[test]
    fn path_builders_reject_invalid_identifiers() {
        assert!(collection_path("has space").is_none());
        assert!(collection_path("").is_none());
        assert!(secret_path("ok", "bad-id").is_none());
        assert!(session_path("bad id").is_none());
    }

    // Validation: a post-epoch time converts to its seconds value;
    // the epoch itself is 0.
    // Adversarial: an Err time and a pre-epoch time both degrade
    // to 0 rather than panicking or wrapping.
    #[test]
    fn time_to_int_cases() {
        assert_eq!(time_to_int(Ok(SystemTime::UNIX_EPOCH)), 0);
        assert_eq!(
            time_to_int(Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(90))),
            90
        );
        assert_eq!(
            time_to_int(Ok(SystemTime::UNIX_EPOCH - Duration::from_secs(5))),
            0
        );
        assert_eq!(time_to_int(Err(io::Error::from(io::ErrorKind::Other))), 0);
    }

    // Validation: try_interface passes values through and maps
    // only InterfaceNotFound to None; other errors stay errors.
    #[test]
    fn try_interface_cases() {
        assert_eq!(try_interface(Ok(3)).unwrap(), Some(3));
        let missing: zbus::Result<u8> = Err(zbus::Error::InterfaceNotFound);
        assert_eq!(try_interface(missing).unwrap(), None);
        let other: zbus::Result<u8> = Err(zbus::Error::Failure("boom".to_owned()));
        assert!(try_interface(other).is_err());
    }
}
