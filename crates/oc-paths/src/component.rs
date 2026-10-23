//! The stored path form: a root-relative path is a list of byte-string components,
//! stored as one BLOB joined by NUL. NUL cannot occur in a component on any
//! supported system, so the form is unambiguous and independent of the separator.

use std::ffi::{OsStr, OsString};

use thiserror::Error;

/// Which separators a volume treats as structure inside a component name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Posix,
    Windows,
}

impl Flavor {
    pub const HOST: Self = if cfg!(windows) {
        Self::Windows
    } else {
        Self::Posix
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ComponentFault {
    #[error("is empty")]
    Empty,
    #[error("is `.` or `..`")]
    Dot,
    #[error("contains a NUL byte")]
    Nul,
    #[error("contains a path separator")]
    Separator,
}

pub fn check_component(bytes: &[u8], flavor: Flavor) -> Result<(), ComponentFault> {
    if bytes.is_empty() {
        Err(ComponentFault::Empty)
    } else if bytes == b"." || bytes == b".." {
        Err(ComponentFault::Dot)
    } else if bytes.contains(&0) {
        Err(ComponentFault::Nul)
    } else if bytes.contains(&b'/') || (flavor == Flavor::Windows && bytes.contains(&b'\\')) {
        Err(ComponentFault::Separator)
    } else {
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PathError {
    #[error("a path needs at least one component")]
    NoComponents,
    #[error("component {index} {fault}")]
    Component { index: usize, fault: ComponentFault },
}

/// A validated root-relative path in stored form.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(Vec<u8>);

impl RelPath {
    pub fn from_components<I, C>(components: I, flavor: Flavor) -> Result<Self, PathError>
    where
        I: IntoIterator<Item = C>,
        C: AsRef<[u8]>,
    {
        let mut stored = Vec::new();
        let mut any = false;
        for (index, component) in components.into_iter().enumerate() {
            let component = component.as_ref();
            check_component(component, flavor)
                .map_err(|fault| PathError::Component { index, fault })?;
            if index > 0 {
                stored.push(0);
            }
            stored.extend_from_slice(component);
            any = true;
        }
        if any {
            Ok(Self(stored))
        } else {
            Err(PathError::NoComponents)
        }
    }

    /// Re-validate bytes read back from the database.
    pub fn from_stored(stored: &[u8], flavor: Flavor) -> Result<Self, PathError> {
        if stored.is_empty() {
            return Err(PathError::NoComponents);
        }
        Self::from_components(stored.split(|byte| *byte == 0), flavor)
    }

    pub fn as_stored(&self) -> &[u8] {
        &self.0
    }

    pub fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.0.split(|byte| *byte == 0)
    }

    pub fn component_count(&self) -> usize {
        self.components().count()
    }

    /// For display only. Never compare or key on this text.
    pub fn display_lossy(&self) -> String {
        self.components()
            .map(|component| String::from_utf8_lossy(component).into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("name cannot be represented on this platform")]
pub struct OsNameError;

/// Raw bytes on Unix; WTF-8 of the UTF-16 name on Windows.
#[cfg(unix)]
#[must_use]
pub fn os_str_to_bytes(name: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes().to_vec()
}

#[cfg(windows)]
#[must_use]
pub fn os_str_to_bytes(name: &OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    let units: Vec<u16> = name.encode_wide().collect();
    crate::wtf8::encode(&units)
}

#[cfg(not(any(unix, windows)))]
#[must_use]
pub fn os_str_to_bytes(name: &OsStr) -> Vec<u8> {
    name.to_string_lossy().into_owned().into_bytes()
}

#[cfg(unix)]
pub fn bytes_to_os_string(bytes: &[u8]) -> Result<OsString, OsNameError> {
    use std::os::unix::ffi::OsStringExt;
    Ok(OsString::from_vec(bytes.to_vec()))
}

#[cfg(windows)]
pub fn bytes_to_os_string(bytes: &[u8]) -> Result<OsString, OsNameError> {
    use std::os::windows::ffi::OsStringExt;
    let units = crate::wtf8::decode(bytes).map_err(|_| OsNameError)?;
    Ok(OsString::from_wide(&units))
}

#[cfg(not(any(unix, windows)))]
pub fn bytes_to_os_string(bytes: &[u8]) -> Result<OsString, OsNameError> {
    String::from_utf8(bytes.to_vec())
        .map(OsString::from)
        .map_err(|_| OsNameError)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(parts: &[&[u8]]) -> RelPath {
        RelPath::from_components(parts, Flavor::Posix).unwrap()
    }

    #[test]
    fn stored_form_joins_components_with_nul_and_round_trips() {
        let path = rel(&[b"Artist", b"Release #1 +x%20", b"01 - Track.aif"]);
        assert_eq!(
            path.as_stored(),
            b"Artist\0Release #1 +x%20\x0001 - Track.aif"
        );
        assert_eq!(path.component_count(), 3);
        assert_eq!(
            RelPath::from_stored(path.as_stored(), Flavor::Posix).unwrap(),
            path
        );
        assert_eq!(
            path.display_lossy(),
            "Artist/Release #1 +x%20/01 - Track.aif"
        );
    }

    #[test]
    fn invalid_utf8_names_are_kept_not_lost() {
        let raw: &[u8] = &[b'A', 0xFF, 0xFE, b'.', b'a', b'i', b'f'];
        let path = rel(&[b"dir", raw]);
        assert_eq!(path.components().nth(1).unwrap(), raw);
        assert!(path.display_lossy().contains('\u{FFFD}'));
    }

    #[test]
    fn components_are_validated() {
        let bad = |parts: &[&[u8]], flavor| RelPath::from_components(parts, flavor).unwrap_err();
        assert_eq!(bad(&[], Flavor::Posix), PathError::NoComponents);
        let fault = |index, fault| PathError::Component { index, fault };
        assert_eq!(
            bad(&[b"a", b""], Flavor::Posix),
            fault(1, ComponentFault::Empty)
        );
        assert_eq!(
            bad(&[b"..", b"x"], Flavor::Posix),
            fault(0, ComponentFault::Dot)
        );
        assert_eq!(
            bad(&[b"a", b"."], Flavor::Windows),
            fault(1, ComponentFault::Dot)
        );
        assert_eq!(
            bad(&[b"a\0b"], Flavor::Posix),
            fault(0, ComponentFault::Nul)
        );
        assert_eq!(
            bad(&[b"a/b"], Flavor::Posix),
            fault(0, ComponentFault::Separator)
        );
    }

    #[test]
    fn backslash_is_a_separator_only_on_windows_volumes() {
        assert!(RelPath::from_components([b"a\\b"], Flavor::Posix).is_ok());
        assert_eq!(
            RelPath::from_components([b"a\\b"], Flavor::Windows),
            Err(PathError::Component {
                index: 0,
                fault: ComponentFault::Separator
            })
        );
    }

    #[test]
    fn stored_bytes_are_revalidated() {
        assert!(RelPath::from_stored(b"", Flavor::Posix).is_err());
        assert!(RelPath::from_stored(b"a\0\0b", Flavor::Posix).is_err());
        assert!(RelPath::from_stored(b"a\0b\0", Flavor::Posix).is_err());
    }

    #[test]
    fn os_names_round_trip_through_bytes() {
        let name = OsString::from("Caf\u{e9} - 01.aif");
        let bytes = os_str_to_bytes(&name);
        assert_eq!(bytes_to_os_string(&bytes).unwrap(), name);
    }

    #[cfg(unix)]
    #[test]
    fn unix_names_that_are_not_text_round_trip() {
        use std::os::unix::ffi::OsStringExt;
        let name = OsString::from_vec(vec![b'x', 0xFF, 0xC3]);
        let bytes = os_str_to_bytes(&name);
        assert_eq!(bytes, [b'x', 0xFF, 0xC3]);
        assert_eq!(bytes_to_os_string(&bytes).unwrap(), name);
    }
}
