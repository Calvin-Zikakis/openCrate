//! File and folder names that are valid on Windows, macOS and Linux at once.
//!
//! Forbidden characters are replaced with `_`, never deleted, so words are not fused.
//! Names are NFC, a component is at most 100 characters and 240 bytes, Windows device
//! names are defused, and nothing ends in a dot or a space. The functions are idempotent:
//! sanitizing a sanitized name changes nothing.

use unicode_normalization::UnicodeNormalization;

pub const MAX_COMPONENT_CHARS: usize = 100;
pub const MAX_COMPONENT_BYTES: usize = 240;
pub const REPLACEMENT: char = '_';

/// Longest extension kept, in characters.
const MAX_EXTENSION_CHARS: usize = 8;

const WINDOWS_DEVICES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn forbidden(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        || c.is_control()
        // Line and paragraph separators, and the bidi controls used to disguise names.
        || matches!(c, '\u{2028}' | '\u{2029}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Windows reads `CON`, `CON.txt` and `COM1.anything` as devices, case-insensitively, and the
/// superscript digits count too.
fn is_device(stem: &str) -> bool {
    let normalized: String = stem
        .chars()
        .map(|c| match c {
            '\u{b9}' => '1',
            '\u{b2}' => '2',
            '\u{b3}' => '3',
            other => other,
        })
        .collect::<String>()
        .to_ascii_uppercase();
    WINDOWS_DEVICES.contains(&normalized.trim_end_matches([' ', '.']))
}

fn truncate(text: &str, max_chars: usize, max_bytes: usize) -> &str {
    let mut end = 0;
    for (count, (index, c)) in text.char_indices().enumerate() {
        let next = index + c.len_utf8();
        if count >= max_chars || next > max_bytes {
            break;
        }
        end = next;
    }
    &text[..end]
}

/// One pass of every rule; callers repeat it until nothing changes.
fn pass(name: &str, max_chars: usize, max_bytes: usize) -> String {
    let replaced: String = name
        .nfc()
        .map(|c| if forbidden(c) { REPLACEMENT } else { c })
        .collect();
    let trimmed = replaced.trim();
    // Trailing dots are replaced one for one (Windows would drop them and rename the file);
    // leading and trailing spaces are only padding and are trimmed.
    let dots = trimmed.chars().rev().take_while(|c| *c == '.').count();
    let keep = trimmed.chars().count() - dots;
    let kept: String = trimmed
        .chars()
        .take(keep)
        .chain(std::iter::repeat_n(REPLACEMENT, dots))
        .collect();
    let mut out = truncate(&kept, max_chars, max_bytes).trim_end().to_owned();
    if out.is_empty() || out.chars().all(|c| c == '.') {
        out = out.chars().map(|_| REPLACEMENT).collect();
        if out.is_empty() {
            out.push(REPLACEMENT);
        }
    }
    let stem_end = out.find('.').unwrap_or(out.len());
    if is_device(&out[..stem_end]) {
        // Defuse by adding one character to the stem, keeping within the caps.
        let stem = truncate(
            &out[..stem_end],
            max_chars.saturating_sub(1),
            max_bytes.saturating_sub(1),
        )
        .to_owned();
        out = format!("{stem}{REPLACEMENT}{}", &out[stem_end..]);
        out = truncate(&out, max_chars, max_bytes).to_owned();
    }
    out
}

fn settle(name: &str, max_chars: usize, max_bytes: usize) -> String {
    let mut current = pass(name, max_chars, max_bytes);
    for _ in 0..4 {
        let next = pass(&current, max_chars, max_bytes);
        if next == current {
            break;
        }
        current = next;
    }
    current
}

/// A folder name.
#[must_use]
pub fn sanitize_component(name: &str) -> String {
    settle(name, MAX_COMPONENT_CHARS, MAX_COMPONENT_BYTES)
}

/// An extension without its dot: ASCII letters and digits, lower case, at most 8 characters.
#[must_use]
pub fn sanitize_extension(extension: &str) -> String {
    extension
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(MAX_EXTENSION_CHARS)
        .collect()
}

/// A file name from a stem and an extension. The stem gives way when the whole name would
/// exceed the caps; the extension is always kept.
#[must_use]
pub fn sanitize_file_name(stem: &str, extension: &str) -> String {
    let extension = sanitize_extension(extension);
    if extension.is_empty() {
        return sanitize_component(stem);
    }
    let reserved = extension.len() + 1;
    let stem = settle(
        stem,
        MAX_COMPONENT_CHARS.saturating_sub(reserved),
        MAX_COMPONENT_BYTES.saturating_sub(reserved),
    );
    // The stem may now end in a dot or be a device name only with the extension attached,
    // so settle the whole name once more.
    let whole = format!("{stem}.{extension}");
    let settled = settle(&whole, MAX_COMPONENT_CHARS, MAX_COMPONENT_BYTES);
    if settled.ends_with(&format!(".{extension}")) {
        settled
    } else {
        whole
    }
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

    /// Every rule a name must satisfy to be safe on all three systems.
    fn assert_valid(name: &str) {
        assert!(!name.is_empty(), "empty");
        assert!(
            name.chars().count() <= MAX_COMPONENT_CHARS,
            "too many chars: {name:?}"
        );
        assert!(
            name.len() <= MAX_COMPONENT_BYTES,
            "too many bytes: {name:?}"
        );
        assert!(
            !name.chars().any(forbidden),
            "forbidden character in {name:?}"
        );
        assert!(
            !name.ends_with(['.', ' ']),
            "ends in a dot or space: {name:?}"
        );
        assert!(!name.starts_with(' '), "starts with a space: {name:?}");
        assert!(name != "." && name != "..", "dot name");
        let stem_end = name.find('.').unwrap_or(name.len());
        assert!(!is_device(&name[..stem_end]), "device name: {name:?}");
        assert_eq!(name, name.nfc().collect::<String>(), "not NFC: {name:?}");
    }

    #[test]
    fn forbidden_characters_are_replaced_not_deleted() {
        assert_eq!(sanitize_component("AC/DC"), "AC_DC");
        assert_eq!(sanitize_component("What?"), "What_");
        assert_eq!(sanitize_component("a<b>c:d\"e|f*g\\h"), "a_b_c_d_e_f_g_h");
        assert_eq!(
            sanitize_component("tab\there\nnewline\0nul"),
            "tab_here_newline_nul"
        );
        assert_eq!(sanitize_component("one\u{2028}two"), "one_two");
        assert_eq!(sanitize_component("a\u{202E}b"), "a_b", "bidi override");
        assert_eq!(
            sanitize_component("Caf\u{e9} \u{30c8}\u{30e9}"),
            "Caf\u{e9} \u{30c8}\u{30e9}"
        );
        assert_eq!(sanitize_component("Cafe\u{301}"), "Caf\u{e9}", "NFC");
    }

    #[test]
    fn dots_spaces_and_empty_names() {
        assert_eq!(sanitize_component("Mr. Brown."), "Mr. Brown_");
        assert_eq!(sanitize_component("Trail..."), "Trail___");
        assert_eq!(sanitize_component("  padded  "), "padded");
        assert_eq!(sanitize_component(""), "_");
        assert_eq!(sanitize_component("   "), "_");
        assert_eq!(sanitize_component("."), "_");
        assert_eq!(sanitize_component(".."), "__");
        assert_eq!(
            sanitize_component(".hidden"),
            ".hidden",
            "a leading dot is fine"
        );
    }

    #[test]
    fn windows_device_names_are_defused_with_or_without_an_extension() {
        for (input, want) in [
            ("CON", "CON_"),
            ("con", "con_"),
            ("Prn.txt", "Prn_.txt"),
            ("AUX.tar.gz", "AUX_.tar.gz"),
            ("COM1", "COM1_"),
            ("lpt9.aif", "lpt9_.aif"),
            ("COM\u{b9}", "COM\u{b9}_"),
            ("NUL ", "NUL_"),
            ("Console", "Console"),
            ("COM10", "COM10"),
            ("CON.", "CON_"),
        ] {
            assert_eq!(sanitize_component(input), want, "{input:?}");
        }
    }

    #[test]
    fn long_names_are_cut_by_characters_and_by_bytes_on_a_character_boundary() {
        let wide = "a".repeat(300);
        let cut = sanitize_component(&wide);
        assert_eq!(cut.chars().count(), MAX_COMPONENT_CHARS);
        let cjk = "\u{30c8}".repeat(100); // 3 bytes each: 300 bytes, over the byte cap
        let cut = sanitize_component(&cjk);
        assert!(cut.len() <= MAX_COMPONENT_BYTES);
        assert_eq!(cut.chars().count(), 80);
        let emoji = "\u{1F3B5}".repeat(100); // 4 bytes each
        assert_eq!(sanitize_component(&emoji).chars().count(), 60);
        assert_valid(&sanitize_component(&format!("{}.", "x".repeat(99))));
    }

    #[test]
    fn file_names_keep_their_extension_when_the_stem_gives_way() {
        assert_eq!(
            sanitize_file_name("01 - Artist - Title", "aif"),
            "01 - Artist - Title.aif"
        );
        let long = sanitize_file_name(&"t".repeat(500), "flac");
        assert!(
            long.rsplit('.').next() == Some("flac") && long.chars().count() == MAX_COMPONENT_CHARS
        );
        assert_eq!(sanitize_file_name("What?", "MP3"), "What_.mp3");
        assert_eq!(sanitize_file_name("CON", "aif"), "CON_.aif");
        assert_eq!(
            sanitize_file_name("ends with dot.", "aif"),
            "ends with dot_.aif"
        );
        assert_eq!(sanitize_file_name("", "aif"), "_.aif");
        assert_eq!(sanitize_file_name("name", ""), "name");
        assert_eq!(sanitize_extension("..A.I-F!"), "aif");
        assert_eq!(sanitize_extension("averylongextension"), "averylon");
        assert_valid(&sanitize_file_name(&"\u{30c8}".repeat(200), "aiff"));
    }

    #[test]
    fn random_names_always_come_out_valid_and_stable() {
        let pool: Vec<&str> = vec![
            "a",
            "B",
            "z",
            "0",
            "9",
            " ",
            " ",
            ".",
            ".",
            "..",
            "/",
            "\\",
            ":",
            "*",
            "?",
            "\"",
            "<",
            ">",
            "|",
            "\0",
            "\t",
            "\n",
            "\u{7f}",
            "\u{85}",
            "\u{2028}",
            "\u{202e}",
            "e\u{301}",
            "\u{e9}",
            "\u{30c8}",
            "\u{1F3B5}",
            "\u{5d0}",
            "\u{200b}",
            "-",
            "_",
            "CON",
            "nul",
            "com1",
            "LPT3",
            "AUX.",
            "\u{b9}",
            "Straße",
            "Ａ",
            "\u{fb01}",
        ];
        let mut seed = 99;
        for _ in 0..5_000 {
            let length = usize::try_from(lcg(&mut seed) % 40).unwrap();
            let name: String = (0..length)
                .map(|_| pool[usize::try_from(lcg(&mut seed)).unwrap() % pool.len()])
                .collect();
            let once = sanitize_component(&name);
            assert_valid(&once);
            assert_eq!(
                sanitize_component(&once),
                once,
                "not idempotent for {name:?}"
            );
            let file = sanitize_file_name(&name, "aif");
            assert_valid(&file);
            assert_eq!(file.rsplit('.').next(), Some("aif"), "{file:?}");
            assert_eq!(
                sanitize_file_name(file.strip_suffix(".aif").unwrap(), "aif"),
                file,
                "file {name:?}"
            );
        }
    }

    #[test]
    fn names_that_are_already_fine_are_left_alone() {
        for name in [
            "Studio 001",
            "Between Spaces",
            "01 - Studio 001 - Night Drive",
            "Caf\u{e9}",
            "A & B",
            "100%",
            "#1 + #2",
            "it's",
        ] {
            assert_eq!(sanitize_component(name), name);
        }
    }
}
