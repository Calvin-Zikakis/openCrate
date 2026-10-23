//! WTF-8: UTF-8 that also encodes unpaired UTF-16 surrogates, so any Windows file
//! name round-trips as bytes and well-formed names are ordinary UTF-8.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Wtf8Error {
    #[error("invalid byte sequence at offset {offset}")]
    Invalid { offset: usize },
    #[error("surrogate pair at offset {offset} must use the four-byte form")]
    NotCanonical { offset: usize },
}

fn is_high(code: u32) -> bool {
    (0xD800..0xDC00).contains(&code)
}

fn is_low(code: u32) -> bool {
    (0xDC00..0xE000).contains(&code)
}

#[must_use]
pub fn encode(units: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(units.len());
    let mut index = 0;
    while let Some(&unit) = units.get(index) {
        index += 1;
        let mut code = u32::from(unit);
        if is_high(code) {
            if let Some(&next) = units.get(index) {
                let low = u32::from(next);
                if is_low(low) {
                    code = 0x1_0000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                    index += 1;
                }
            }
        }
        push_code(&mut out, code);
    }
    out
}

// Every shifted and masked value below fits in a byte.
fn push_code(out: &mut Vec<u8>, code: u32) {
    if code < 0x80 {
        out.push((code & 0x7F) as u8);
    } else if code < 0x800 {
        out.push(0xC0 | ((code >> 6) & 0x1F) as u8);
        out.push(0x80 | (code & 0x3F) as u8);
    } else if code < 0x1_0000 {
        out.push(0xE0 | ((code >> 12) & 0x0F) as u8);
        out.push(0x80 | ((code >> 6) & 0x3F) as u8);
        out.push(0x80 | (code & 0x3F) as u8);
    } else {
        out.push(0xF0 | ((code >> 18) & 0x07) as u8);
        out.push(0x80 | ((code >> 12) & 0x3F) as u8);
        out.push(0x80 | ((code >> 6) & 0x3F) as u8);
        out.push(0x80 | (code & 0x3F) as u8);
    }
}

fn continuation(bytes: &[u8], at: usize) -> Result<u32, Wtf8Error> {
    match bytes.get(at) {
        Some(&byte) if (0x80..=0xBF).contains(&byte) => Ok(u32::from(byte & 0x3F)),
        _ => Err(Wtf8Error::Invalid { offset: at }),
    }
}

pub fn decode(bytes: &[u8]) -> Result<Vec<u16>, Wtf8Error> {
    let mut units = Vec::with_capacity(bytes.len());
    let mut offset = 0;
    let mut after_high = false;
    while let Some(&lead) = bytes.get(offset) {
        let invalid = Wtf8Error::Invalid { offset };
        let (code, length) = match lead {
            0x00..=0x7F => (u32::from(lead), 1),
            0xC2..=0xDF => (
                (u32::from(lead & 0x1F) << 6) | continuation(bytes, offset + 1)?,
                2,
            ),
            0xE0..=0xEF => {
                let code = (u32::from(lead & 0x0F) << 12)
                    | (continuation(bytes, offset + 1)? << 6)
                    | continuation(bytes, offset + 2)?;
                if code < 0x800 {
                    return Err(invalid);
                }
                (code, 3)
            }
            0xF0..=0xF4 => {
                let code = (u32::from(lead & 0x07) << 18)
                    | (continuation(bytes, offset + 1)? << 12)
                    | (continuation(bytes, offset + 2)? << 6)
                    | continuation(bytes, offset + 3)?;
                if !(0x1_0000..=0x10_FFFF).contains(&code) {
                    return Err(invalid);
                }
                (code, 4)
            }
            _ => return Err(invalid),
        };
        if after_high && is_low(code) {
            return Err(Wtf8Error::NotCanonical { offset });
        }
        after_high = is_high(code);
        push_units(&mut units, code, offset)?;
        offset += length;
    }
    Ok(units)
}

fn push_units(units: &mut Vec<u16>, code: u32, offset: usize) -> Result<(), Wtf8Error> {
    let invalid = || Wtf8Error::Invalid { offset };
    if code >= 0x1_0000 {
        let value = code - 0x1_0000;
        units.push(u16::try_from(0xD800 + (value >> 10)).map_err(|_| invalid())?);
        units.push(u16::try_from(0xDC00 + (value & 0x3FF)).map_err(|_| invalid())?);
    } else {
        units.push(u16::try_from(code).map_err(|_| invalid())?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *seed >> 33
    }

    #[test]
    fn well_formed_text_is_plain_utf8() {
        for text in ["", "Track 01.aif", "Caf\u{e9}", "\u{1F3B5} mix", "\u{FB01}"] {
            let units: Vec<u16> = text.encode_utf16().collect();
            assert_eq!(encode(&units), text.as_bytes());
            assert_eq!(decode(text.as_bytes()).unwrap(), units);
        }
    }

    #[test]
    fn unpaired_surrogates_round_trip() {
        for units in [
            vec![0xD800u16],
            vec![0xDC00],
            vec![0x61, 0xDC00, 0xD800, 0x62],
            vec![0xD800, 0xD800, 0xDC00],
        ] {
            let bytes = encode(&units);
            assert_eq!(decode(&bytes).unwrap(), units, "{units:x?}");
        }
        assert_eq!(encode(&[0xD800]), [0xED, 0xA0, 0x80]);
    }

    #[test]
    fn random_unit_sequences_round_trip() {
        let mut seed = 7;
        for _ in 0..2_000 {
            let length = usize::try_from(lcg(&mut seed) % 12).unwrap();
            let units: Vec<u16> = (0..length)
                .map(|_| match lcg(&mut seed) % 4 {
                    0 => u16::try_from(0xD800 + lcg(&mut seed) % 0x400).unwrap(),
                    1 => u16::try_from(0xDC00 + lcg(&mut seed) % 0x400).unwrap(),
                    2 => u16::try_from(lcg(&mut seed) % 0x80).unwrap(),
                    _ => u16::try_from(0x100 + lcg(&mut seed) % 0xD000).unwrap(),
                })
                .collect();
            assert_eq!(decode(&encode(&units)).unwrap(), units, "{units:x?}");
        }
    }

    #[test]
    fn malformed_and_non_canonical_input_is_refused() {
        assert!(decode(&[0xFF]).is_err());
        assert!(decode(&[0xC0, 0x80]).is_err(), "overlong two-byte");
        assert!(decode(&[0xE0, 0x80, 0x80]).is_err(), "overlong three-byte");
        assert!(decode(&[0xF4, 0x90, 0x80, 0x80]).is_err(), "above U+10FFFF");
        assert!(decode(&[0xC3]).is_err(), "truncated");
        assert!(decode(&[0xC3, 0x41]).is_err(), "bad continuation");
        // A high surrogate then a low surrogate, each in the three-byte form.
        assert_eq!(
            decode(&[0xED, 0xA0, 0x80, 0xED, 0xB0, 0x80]),
            Err(Wtf8Error::NotCanonical { offset: 3 })
        );
    }
}
