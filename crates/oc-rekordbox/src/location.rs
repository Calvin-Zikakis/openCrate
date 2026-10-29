use percent_encoding::percent_decode_str;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LocationError {
    #[error("expected a file://localhost/ location")]
    UnsupportedPrefix,
    #[error("invalid percent escape at path byte {offset}")]
    InvalidEscape { offset: usize },
    #[error("a location cannot contain a NUL byte")]
    NullByte,
}

/// Bytes rekordbox leaves unencoded. Research 002 saw `,`, `#`, `+` and `$` raw and
/// parentheses never encoded; the rest of the punctuation here is assumed.
fn kept(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"/:,#+$-._()!~;=@*".contains(&byte)
}

/// A drive path such as `C:/x`, `C:\x` or the decoded form `/C:/x`.
fn drive_form(path: &[u8]) -> bool {
    let rest = path.strip_prefix(b"/").unwrap_or(path);
    rest.len() >= 2 && rest[0].is_ascii_alphabetic() && rest[1] == b':'
}

/// The `Location` rekordbox would write for an absolute path: `file://localhost/`, then the
/// path with forward slashes, percent-encoded with lowercase hex, as rekordbox's own exports show.
/// Backslashes are separators only in a drive path; elsewhere they are part of a name.
pub fn encode_location(path: &[u8]) -> String {
    use std::fmt::Write as _;
    let windows = drive_form(path);
    let rest = path.strip_prefix(b"/").unwrap_or(path);
    let mut out = String::with_capacity(rest.len() + 24);
    out.push_str("file://localhost/");
    for &byte in rest {
        let byte = if windows && byte == b'\\' { b'/' } else { byte };
        if kept(byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02x}");
        }
    }
    out
}

pub fn decode_location(location: &str) -> Result<Vec<u8>, LocationError> {
    let suffix = location
        .strip_prefix("file://localhost/")
        .ok_or(LocationError::UnsupportedPrefix)?;
    let bytes = suffix.as_bytes();
    for (offset, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && !bytes
                .get(offset + 1..offset + 3)
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
        {
            return Err(LocationError::InvalidEscape { offset });
        }
    }
    let mut decoded = vec![b'/'];
    decoded.extend(percent_decode_str(suffix));
    if decoded.contains(&0) {
        return Err(LocationError::NullByte);
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::{decode_location, encode_location, LocationError};

    #[test]
    fn preserves_literal_url_delimiters_and_punctuation() {
        let decoded = decode_location("file://localhost/G:/Music/A-B/Track#1+2,$.aif");
        assert_eq!(decoded.unwrap(), b"/G:/Music/A-B/Track#1+2,$.aif");
        assert_ne!(
            decode_location("file://localhost/G:/A-B.aif").unwrap(),
            decode_location("file://localhost/G:/AB.aif").unwrap()
        );
    }

    #[test]
    fn decodes_both_hex_cases_exactly_once() {
        assert_eq!(
            decode_location("file://localhost/G:/A%20%27%26%5b%5D%2520.aif").unwrap(),
            b"/G:/A '&[]%20.aif"
        );
    }

    #[test]
    fn preserves_non_text_bytes() {
        assert_eq!(
            decode_location("file://localhost/G:/%ff.aif").unwrap(),
            b"/G:/\xff.aif"
        );
    }

    #[test]
    fn preserves_case_and_unicode_form() {
        let composed = decode_location("file://localhost/Volumes/Music/Caf%c3%a9.aif");
        let decomposed = decode_location("file://localhost/Volumes/Music/Cafe%cc%81.aif");
        assert_eq!(composed.unwrap(), b"/Volumes/Music/Caf\xc3\xa9.aif");
        assert_eq!(decomposed.unwrap(), b"/Volumes/Music/Cafe\xcc\x81.aif");
        assert_ne!(
            decode_location("file://localhost/G:/A.aif").unwrap(),
            decode_location("file://localhost/G:/a.aif").unwrap()
        );
    }

    #[test]
    fn preserves_unc_and_posix_path_shapes() {
        assert_eq!(
            decode_location("file://localhost//server/share/track.aif").unwrap(),
            b"//server/share/track.aif"
        );
        assert_eq!(
            decode_location("file://localhost/Volumes/Music/track.aif").unwrap(),
            b"/Volumes/Music/track.aif"
        );
    }

    #[test]
    fn rejects_malformed_escapes_and_nul_without_leaking_paths() {
        for suffix in ["%", "%2", "%gg", "%0z"] {
            assert_eq!(
                decode_location(&format!("file://localhost/{suffix}")),
                Err(LocationError::InvalidEscape { offset: 0 })
            );
        }
        for suffix in ["%00", "\0"] {
            assert_eq!(
                decode_location(&format!("file://localhost/{suffix}")),
                Err(LocationError::NullByte)
            );
        }
    }

    #[test]
    fn rejects_unsupported_schemes_and_authorities() {
        for location in [
            "https://example.invalid/track.aif",
            "file://server/share/track.aif",
            "file://localhost.invalid/track.aif",
            "file://localhost",
            "G:/Music/track.aif",
        ] {
            assert_eq!(
                decode_location(location),
                Err(LocationError::UnsupportedPrefix)
            );
        }
    }

    #[test]
    fn locations_are_spelled_like_rekordbox() {
        assert_eq!(
            encode_location("G:/Music/A & B/It's [VIP] 100% Café, #1 + $.aif".as_bytes()),
            "file://localhost/G:/Music/A%20%26%20B/It%27s%20%5bVIP%5d%20100%25%20Caf%c3%a9,%20#1%20+%20$.aif"
        );
        assert_eq!(
            encode_location(b"/Users/dj/Music/x y (Original Mix).mp3"),
            "file://localhost/Users/dj/Music/x%20y%20(Original%20Mix).mp3"
        );
        assert_eq!(
            encode_location(b"C:\\Music\\a.wav"),
            "file://localhost/C:/Music/a.wav"
        );
        assert_eq!(
            encode_location(b"/G:/Music/a.wav"),
            "file://localhost/G:/Music/a.wav"
        );
        assert_eq!(
            encode_location(b"/Users/dj/back\\slash \"quoted\" <x>.mp3"),
            "file://localhost/Users/dj/back%5cslash%20%22quoted%22%20%3cx%3e.mp3"
        );
    }

    #[test]
    fn encoded_locations_decode_back() {
        for path in [
            &b"/Users/dj/Music/A & B/It's [1].aif"[..],
            b"/G:/Music/100%/Caf\xc3\xa9;=@!~*.aif",
            b"/Volumes/Music/\xff\x01 tab\t.aif",
            b"/Volumes/Music/{a}^`|.mp3",
        ] {
            assert_eq!(
                decode_location(&encode_location(path)).unwrap(),
                path,
                "{path:?}"
            );
        }
    }
}
