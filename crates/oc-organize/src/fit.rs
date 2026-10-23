//! Keeping a destination inside Windows' path limit.
//!
//! The limit counts UTF-16 code units over the whole path, root included, so a character
//! outside the Basic Multilingual Plane (an emoji) counts twice. Only the file stem can give
//! way: shortening a name cannot repair a root or parent folder that is already too long, and
//! then the answer is an error, not a quietly broken path.

use thiserror::Error;

use crate::sanitize::{sanitize_file_name, MAX_COMPONENT_CHARS};

/// Classic `MAX_PATH` is 260 including the terminating NUL.
pub const CLASSIC_LIMIT: usize = 259;
/// Room for the temporary suffix a copy-then-rename uses while a file is being written.
pub const TEMP_SUFFIX_RESERVE: usize = 16;
/// Windows with long paths enabled, counted in the same units.
pub const LONG_PATH_LIMIT: usize = 32_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathBudget {
    pub limit: usize,
    pub reserve: usize,
    /// The shortest stem worth keeping, in characters.
    pub min_stem_chars: usize,
}

impl PathBudget {
    /// The default: classic limit with room for the temporary suffix (A15).
    #[must_use]
    pub const fn classic() -> Self {
        Self {
            limit: CLASSIC_LIMIT,
            reserve: TEMP_SUFFIX_RESERVE,
            min_stem_chars: 8,
        }
    }

    #[must_use]
    pub const fn long_paths() -> Self {
        Self {
            limit: LONG_PATH_LIMIT,
            reserve: TEMP_SUFFIX_RESERVE,
            min_stem_chars: 8,
        }
    }

    const fn usable(self) -> usize {
        self.limit.saturating_sub(self.reserve)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FitError {
    #[error("the folders alone leave no room: {over_by} characters over even with the shortest file name")]
    FoldersTooLong { over_by: usize },
    #[error("a destination needs at least a file name")]
    NoComponents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fitted {
    pub components: Vec<String>,
    /// The file stem was shortened, which the plan view must show.
    pub stem_shortened: bool,
    /// UTF-16 units of the whole path, root included.
    pub length: usize,
}

#[must_use]
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Total length of `root` plus every component, with one separator before each.
fn total(root: &str, components: &[String]) -> usize {
    utf16_len(root)
        + components
            .iter()
            .map(|part| 1 + utf16_len(part))
            .sum::<usize>()
}

/// Fit `components` (folders, then the file name) below `root`, which is the library root as
/// the operating system spells it, such as `G:\Library`. A root with a trailing separator
/// should be given without it.
pub fn fit_destination(
    root: &str,
    components: &[String],
    budget: &PathBudget,
) -> Result<Fitted, FitError> {
    let Some((file, folders)) = components.split_last() else {
        return Err(FitError::NoComponents);
    };
    let usable = budget.usable();
    let used = total(root, components);
    if used <= usable {
        return Ok(Fitted {
            components: components.to_vec(),
            stem_shortened: false,
            length: used,
        });
    }
    let (stem, extension) = file
        .rsplit_once('.')
        .map_or((file.as_str(), ""), |(s, e)| (s, e));
    let others = total(root, &components[..folders.len()])
        + 1
        + extension.chars().count()
        + usize::from(!extension.is_empty());
    let space = usable.saturating_sub(others);
    let floor = budget.min_stem_chars.min(stem.chars().count());
    // Cut characters off the end until the units fit; each character is one or two units.
    let mut kept = String::new();
    let mut units = 0;
    for c in stem.chars().take(MAX_COMPONENT_CHARS) {
        let width = c.len_utf16();
        if units + width > space {
            break;
        }
        kept.push(c);
        units += width;
    }
    if kept.chars().count() < floor {
        return Err(FitError::FoldersTooLong {
            over_by: used - usable,
        });
    }
    let shortened = sanitize_file_name(&kept, extension);
    let mut out = folders.to_vec();
    out.push(shortened);
    let length = total(root, &out);
    if length > usable {
        // Sanitizing can add a character (a defused device name); give one more back.
        return Err(FitError::FoldersTooLong {
            over_by: length - usable,
        });
    }
    Ok(Fitted {
        components: out,
        stem_shortened: true,
        length,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_aif(name: &str) -> bool {
        std::path::Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("aif"))
    }

    fn parts(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn a_short_destination_is_untouched() {
        let components = parts(&[
            "Studio 001",
            "Between Spaces",
            "03 - Studio 001 - Night Drive.aif",
        ]);
        let fitted = fit_destination(r"G:\Library", &components, &PathBudget::classic()).unwrap();
        assert!(!fitted.stem_shortened);
        assert_eq!(fitted.components, components);
        assert_eq!(fitted.length, 10 + 11 + 15 + 34);
    }

    #[test]
    fn only_the_file_stem_gives_way_and_the_extension_stays() {
        let root = r"G:\Library";
        let folders = ["A".repeat(80), "B".repeat(80)];
        let file = format!("{}.aif", "t".repeat(100));
        let components = vec![folders[0].clone(), folders[1].clone(), file];
        let fitted = fit_destination(root, &components, &PathBudget::classic()).unwrap();
        assert!(fitted.stem_shortened);
        assert_eq!(
            &fitted.components[..2],
            &components[..2],
            "folders are never touched"
        );
        assert!(is_aif(&fitted.components[2]));
        assert!(
            fitted.length <= CLASSIC_LIMIT - TEMP_SUFFIX_RESERVE,
            "{}",
            fitted.length
        );
        assert!(
            fitted.length >= CLASSIC_LIMIT - TEMP_SUFFIX_RESERVE - 1,
            "uses the room it has"
        );
    }

    #[test]
    fn characters_outside_the_bmp_count_twice() {
        let root = r"G:\Library";
        let components = parts(&[
            "A".repeat(100).as_str(),
            &format!("{}.aif", "\u{1F3B5}".repeat(100)),
        ]);
        let fitted = fit_destination(root, &components, &PathBudget::classic()).unwrap();
        assert_eq!(utf16_len("\u{1F3B5}"), 2);
        assert!(fitted.length <= CLASSIC_LIMIT - TEMP_SUFFIX_RESERVE);
        let kept = fitted.components[1]
            .trim_end_matches(".aif")
            .chars()
            .count();
        assert!(kept < 100 && kept > 50, "{kept}");
        assert_eq!(fitted.length, total(root, &fitted.components));
    }

    #[test]
    fn a_root_or_folder_that_is_too_long_is_an_error_not_a_bad_path() {
        let long_root = format!(r"G:\{}", "r".repeat(240));
        let components = parts(&["Artist", "Release", "03 - Artist - Title.aif"]);
        let error = fit_destination(&long_root, &components, &PathBudget::classic()).unwrap_err();
        assert!(matches!(error, FitError::FoldersTooLong { over_by } if over_by > 0));
        let deep = vec![
            "d".repeat(100),
            "e".repeat(100),
            "f".repeat(100),
            "x.aif".to_owned(),
        ];
        assert!(fit_destination(r"G:\L", &deep, &PathBudget::classic()).is_err());
        assert_eq!(
            fit_destination("G:", &[], &PathBudget::classic()),
            Err(FitError::NoComponents)
        );
    }

    #[test]
    fn long_paths_lift_the_limit_and_the_reserve_is_kept() {
        let components = vec![
            "f".repeat(100),
            "g".repeat(100),
            "h".repeat(100),
            format!("{}.aif", "t".repeat(90)),
        ];
        let classic = fit_destination(r"G:\Library", &components, &PathBudget::classic());
        assert!(classic.is_err() || classic.unwrap().stem_shortened);
        let relaxed =
            fit_destination(r"G:\Library", &components, &PathBudget::long_paths()).unwrap();
        assert!(!relaxed.stem_shortened);
        let snug = PathBudget {
            limit: 60,
            reserve: 10,
            min_stem_chars: 4,
        };
        let fitted = fit_destination(
            "G:",
            &parts(&["Artist", "A very long file name indeed.aif"]),
            &snug,
        )
        .unwrap();
        assert!(fitted.length <= 50);
    }

    #[test]
    fn a_shortened_stem_stays_a_valid_name() {
        let components = parts(&["x", &format!("{}.aif", "word ".repeat(40))]);
        let budget = PathBudget {
            limit: 60,
            reserve: 0,
            min_stem_chars: 4,
        };
        let fitted = fit_destination("G:", &components, &budget).unwrap();
        let name = &fitted.components[1];
        assert!(
            is_aif(name) && !name.trim_end_matches(".aif").ends_with(' '),
            "{name:?}"
        );
        assert_eq!(
            name,
            &sanitize_file_name(name.trim_end_matches(".aif"), "aif")
        );
    }
}
