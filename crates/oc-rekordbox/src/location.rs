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
    use super::{decode_location, LocationError};

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
}
