//! A decoded rekordbox Location (bytes such as `/G:/Folder/File.aif`) to a root and
//! a root-relative path, through explicit aliases. Nothing here touches the disk.

use std::collections::HashMap;
use std::path::{Component, Path, Prefix};

use thiserror::Error;

use crate::component::{check_component, os_str_to_bytes, ComponentFault, Flavor, RelPath};
use crate::profile::NameProfile;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// `/X:/...`; the letter is stored uppercase.
    Drive(u8),
    /// `//server/share/...`. How rekordbox writes UNC paths is unverified.
    Unc,
    Posix,
}

impl Shape {
    /// Stored text: `posix`, `unc` or `drive:X`.
    #[must_use]
    pub fn to_text(self) -> String {
        match self {
            Self::Posix => "posix".to_owned(),
            Self::Unc => "unc".to_owned(),
            Self::Drive(letter) => format!("drive:{}", char::from(letter)),
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "posix" => Some(Self::Posix),
            "unc" => Some(Self::Unc),
            _ => match text.strip_prefix("drive:")?.as_bytes() {
                [letter] if letter.is_ascii_uppercase() => Some(Self::Drive(*letter)),
                _ => None,
            },
        }
    }

    #[must_use]
    pub const fn flavor(self) -> Flavor {
        match self {
            Self::Posix => Flavor::Posix,
            Self::Drive(_) | Self::Unc => Flavor::Windows,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LocationError {
    #[error("location is not an absolute path")]
    NotAbsolute,
    #[error("a UNC location needs a server and a share")]
    UncIncomplete,
    #[error("component {index} {fault}")]
    Component { index: usize, fault: ComponentFault },
}

/// A parsed absolute location: a shape plus validated components. Used both for
/// the Locations in an export and for a root's aliases.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocationPath {
    shape: Shape,
    components: Vec<Vec<u8>>,
}

fn drive_letter(rest: &[u8]) -> Option<u8> {
    match rest {
        [letter, b':'] | [letter, b':', b'/', ..] if letter.is_ascii_alphabetic() => {
            Some(letter.to_ascii_uppercase())
        }
        _ => None,
    }
}

impl LocationPath {
    /// Parse decoded Location bytes. Anything that is not a recognised shape, or
    /// that could escape its folder (`.`, `..`, empty parts, a backslash inside a
    /// Windows-shaped part), is refused rather than guessed.
    pub fn parse(decoded: &[u8]) -> Result<Self, LocationError> {
        let Some((b'/', rest)) = decoded.split_first() else {
            return Err(LocationError::NotAbsolute);
        };
        let (shape, tail) = if let Some(letter) = drive_letter(rest) {
            (Shape::Drive(letter), rest.get(3..).unwrap_or_default())
        } else if let Some(unc) = rest.strip_prefix(b"/") {
            (Shape::Unc, unc)
        } else {
            (Shape::Posix, rest)
        };
        let mut components = Vec::new();
        if !tail.is_empty() {
            for (index, part) in tail.split(|byte| *byte == b'/').enumerate() {
                check_component(part, shape.flavor())
                    .map_err(|fault| LocationError::Component { index, fault })?;
                components.push(part.to_vec());
            }
        }
        if shape == Shape::Unc && components.len() < 2 {
            return Err(LocationError::UncIncomplete);
        }
        Ok(Self { shape, components })
    }

    /// The alias a root's own path contributes (`G:\Music`, `/Volumes/Music`).
    pub fn from_os_path(path: &Path) -> Result<Self, LocationError> {
        let mut shape = None;
        let mut components: Vec<Vec<u8>> = Vec::new();
        for part in path.components() {
            match part {
                Component::Prefix(prefix) => match prefix.kind() {
                    Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                        shape = Some(Shape::Drive(letter.to_ascii_uppercase()));
                    }
                    Prefix::UNC(host, folder) | Prefix::VerbatimUNC(host, folder) => {
                        shape = Some(Shape::Unc);
                        components.push(os_str_to_bytes(host));
                        components.push(os_str_to_bytes(folder));
                    }
                    Prefix::Verbatim(_) | Prefix::DeviceNS(_) => {
                        return Err(LocationError::NotAbsolute);
                    }
                },
                Component::RootDir => {
                    shape.get_or_insert(Shape::Posix);
                }
                Component::Normal(name) => {
                    let bytes = os_str_to_bytes(name);
                    let flavor = shape.map_or(Flavor::HOST, Shape::flavor);
                    check_component(&bytes, flavor).map_err(|fault| LocationError::Component {
                        index: components.len(),
                        fault,
                    })?;
                    components.push(bytes);
                }
                Component::CurDir | Component::ParentDir => {
                    return Err(LocationError::Component {
                        index: components.len(),
                        fault: ComponentFault::Dot,
                    });
                }
            }
        }
        Ok(Self {
            shape: shape.ok_or(LocationError::NotAbsolute)?,
            components,
        })
    }

    /// Rebuild from stored parts, validating every component again.
    pub fn from_parts(shape: Shape, components: Vec<Vec<u8>>) -> Result<Self, LocationError> {
        for (index, part) in components.iter().enumerate() {
            check_component(part, shape.flavor())
                .map_err(|fault| LocationError::Component { index, fault })?;
        }
        if shape == Shape::Unc && components.len() < 2 {
            return Err(LocationError::UncIncomplete);
        }
        Ok(Self { shape, components })
    }

    /// The components joined by NUL, as stored; empty for a bare drive or root.
    #[must_use]
    pub fn components_stored(&self) -> Vec<u8> {
        self.components.join(&0)
    }

    /// The decoded Location form this was parsed from (`/G:/Music`, `//nas/share`,
    /// `/Users/dj`): the inverse of [`LocationPath::parse`].
    #[must_use]
    pub fn to_decoded(&self) -> Vec<u8> {
        let mut bytes = match self.shape {
            Shape::Drive(letter) => vec![b'/', letter, b':'],
            Shape::Unc => vec![b'/'],
            Shape::Posix => Vec::new(),
        };
        for component in &self.components {
            bytes.push(b'/');
            bytes.extend_from_slice(component);
        }
        if bytes.is_empty() {
            bytes.push(b'/');
        }
        bytes
    }

    pub fn shape(&self) -> Shape {
        self.shape
    }

    pub fn components(&self) -> &[Vec<u8>] {
        &self.components
    }
}

/// A root's profile and the prefixes a Location may start with to belong to it.
#[derive(Clone, Debug)]
pub struct RootAliases {
    pub root_id: i64,
    pub profile: NameProfile,
    pub aliases: Vec<LocationPath>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Resolution {
    Mapped {
        root_id: i64,
        alias_len: usize,
        remainder: RelPath,
    },
    /// The Location is a root or alias itself, so it names no file.
    NoFile {
        root_id: i64,
    },
    Unmapped,
    Ambiguous {
        root_ids: Vec<i64>,
    },
}

fn alias_prefix_len(
    location: &LocationPath,
    alias: &LocationPath,
    profile: NameProfile,
) -> Option<usize> {
    if location.shape != alias.shape || alias.components.len() > location.components.len() {
        return None;
    }
    alias
        .components
        .iter()
        .zip(&location.components)
        .all(|(wanted, found)| profile.components_equal(wanted, found))
        .then_some(alias.components.len())
}

/// Longest whole-component alias wins. A tie between different roots is
/// `Ambiguous` and matches neither.
#[must_use]
pub fn resolve(location: &LocationPath, roots: &[RootAliases]) -> Resolution {
    let mut best: Option<usize> = None;
    let mut root_ids: Vec<i64> = Vec::new();
    for root in roots {
        for alias in &root.aliases {
            let Some(len) = alias_prefix_len(location, alias, root.profile) else {
                continue;
            };
            match best {
                Some(current) if len < current => {}
                Some(current) if len == current => root_ids.push(root.root_id),
                _ => {
                    best = Some(len);
                    root_ids = vec![root.root_id];
                }
            }
        }
    }
    root_ids.sort_unstable();
    root_ids.dedup();
    let (Some(len), [root_id]) = (best, root_ids.as_slice()) else {
        return if best.is_some() {
            Resolution::Ambiguous { root_ids }
        } else {
            Resolution::Unmapped
        };
    };
    let rest = location.components.get(len..).unwrap_or_default();
    match RelPath::from_components(rest, location.shape.flavor()) {
        Ok(remainder) => Resolution::Mapped {
            root_id: *root_id,
            alias_len: len,
            remainder,
        },
        Err(_) => Resolution::NoFile { root_id: *root_id },
    }
}

/// Most common leading `depth` components among locations that have at least one
/// component after them (the file name). The owner picks which, if any, to accept
/// as an alias; nothing here accepts one.
#[must_use]
pub fn prefix_histogram(locations: &[LocationPath], depth: usize) -> Vec<(LocationPath, usize)> {
    let mut counts: HashMap<LocationPath, usize> = HashMap::new();
    for location in locations {
        if location.components.len() > depth {
            let prefix = LocationPath {
                shape: location.shape,
                components: location.components[..depth].to_vec(),
            };
            *counts.entry(prefix).or_default() += 1;
        }
    }
    let mut counted: Vec<_> = counts.into_iter().collect();
    counted.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.components.cmp(&right.0.components))
    });
    counted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(text: &str) -> LocationPath {
        LocationPath::parse(text.as_bytes()).unwrap()
    }

    fn root(root_id: i64, profile: NameProfile, aliases: &[&str]) -> RootAliases {
        RootAliases {
            root_id,
            profile,
            aliases: aliases.iter().map(|alias| loc(alias)).collect(),
        }
    }

    fn mapped(root_id: i64, remainder: &[&str], alias_len: usize) -> Resolution {
        Resolution::Mapped {
            root_id,
            alias_len,
            remainder: RelPath::from_components(
                remainder.iter().map(|part| part.as_bytes()),
                Flavor::Windows,
            )
            .unwrap(),
        }
    }

    #[test]
    fn shapes_are_recognised() {
        let drive = loc("/g:/Music/a b.aif");
        assert_eq!(drive.shape(), Shape::Drive(b'G'));
        assert_eq!(drive.components(), [b"Music".to_vec(), b"a b.aif".to_vec()]);
        assert_eq!(loc("/G:/").components().len(), 0);
        assert_eq!(loc("/G:").shape(), Shape::Drive(b'G'));
        let unc = loc("//nas/share/Music/x.aif");
        assert_eq!(unc.shape(), Shape::Unc);
        assert_eq!(unc.components().len(), 4);
        assert_eq!(loc("/Users/dj/Music/x.aif").shape(), Shape::Posix);
        assert_eq!(loc("/").components().len(), 0);
    }

    #[test]
    fn unsafe_or_unknown_shapes_are_refused() {
        let parse = |text: &str| LocationPath::parse(text.as_bytes());
        assert_eq!(parse("Music/x.aif"), Err(LocationError::NotAbsolute));
        assert_eq!(parse(""), Err(LocationError::NotAbsolute));
        assert_eq!(parse("//nas"), Err(LocationError::UncIncomplete));
        assert_eq!(parse("//nas/share/x").unwrap().components().len(), 3);
        for bad in [
            "/G:/a/../b",
            "/G:/a/./b",
            "/G:/a//b",
            "/G:/a/",
            "/Users/../x",
        ] {
            assert!(
                matches!(parse(bad), Err(LocationError::Component { .. })),
                "{bad}"
            );
        }
        assert!(
            parse("/G:/a\\b").is_err(),
            "backslash splits a Windows path"
        );
        assert!(parse("//nas/share\\x/y").is_err());
        assert!(parse("/Users/dj/a\\b.aif").is_ok(), "legal byte on POSIX");
        assert!(parse("/G:/a#b+c%d.aif").is_ok());
    }

    #[test]
    fn non_text_bytes_survive_parsing() {
        let bytes: &[u8] = b"/Users/dj/x\xFF.aif";
        let path = LocationPath::parse(bytes).unwrap();
        assert_eq!(path.components()[2], b"x\xFF.aif");
    }

    #[test]
    fn aliases_match_whole_components_longest_first() {
        let roots = [
            root(1, NameProfile::FoldCase, &["/G:/Music"]),
            root(2, NameProfile::FoldCase, &["/G:/Music/Sets"]),
        ];
        assert_eq!(
            resolve(&loc("/G:/Music/Artist/x.aif"), &roots),
            mapped(1, &["Artist", "x.aif"], 1)
        );
        assert_eq!(
            resolve(&loc("/G:/Music/Sets/x.aif"), &roots),
            mapped(2, &["x.aif"], 2)
        );
        assert_eq!(
            resolve(&loc("/G:/MusicOld/x.aif"), &roots),
            Resolution::Unmapped
        );
        assert_eq!(
            resolve(&loc("/D:/Music/x.aif"), &roots),
            Resolution::Unmapped
        );
        assert_eq!(resolve(&loc("/Music/x.aif"), &roots), Resolution::Unmapped);
    }

    #[test]
    fn drive_letter_and_case_follow_the_roots_profile() {
        let folded = [root(1, NameProfile::FoldCase, &["/G:/Music"])];
        let exact = [root(1, NameProfile::Exact, &["/G:/Music"])];
        let location = loc("/g:/MUSIC/x.aif");
        assert_eq!(resolve(&location, &folded), mapped(1, &["x.aif"], 1));
        assert_eq!(resolve(&location, &exact), Resolution::Unmapped);
    }

    #[test]
    fn equal_length_aliases_in_different_roots_are_ambiguous() {
        let roots = [
            root(5, NameProfile::Exact, &["/G:/Music"]),
            root(3, NameProfile::Exact, &["/G:/Music"]),
        ];
        assert_eq!(
            resolve(&loc("/G:/Music/x.aif"), &roots),
            Resolution::Ambiguous {
                root_ids: vec![3, 5]
            }
        );
        let one_root = [root(3, NameProfile::Exact, &["/G:/Music", "/G:/Music"])];
        assert_eq!(
            resolve(&loc("/G:/Music/x.aif"), &one_root),
            mapped(3, &["x.aif"], 1)
        );
    }

    #[test]
    fn an_old_prefix_keeps_matching_after_the_root_moves() {
        let roots = [root(
            1,
            NameProfile::FoldCase,
            &["/G:/Library", "/D:/Old/Library"],
        )];
        assert_eq!(
            resolve(&loc("/D:/Old/Library/A/x.aif"), &roots),
            mapped(1, &["A", "x.aif"], 2)
        );
        assert_eq!(
            resolve(&loc("/G:/Library/A/x.aif"), &roots),
            mapped(1, &["A", "x.aif"], 1)
        );
    }

    #[test]
    fn a_bare_drive_alias_covers_the_whole_drive() {
        let roots = [root(1, NameProfile::FoldCase, &["/G:"])];
        assert_eq!(resolve(&loc("/G:/x.aif"), &roots), mapped(1, &["x.aif"], 0));
    }

    #[test]
    fn the_root_itself_names_no_file() {
        let roots = [root(1, NameProfile::Exact, &["/G:/Music"])];
        assert_eq!(
            resolve(&loc("/G:/Music"), &roots),
            Resolution::NoFile { root_id: 1 }
        );
    }

    #[test]
    fn unc_and_posix_roots_resolve() {
        let roots = [
            root(1, NameProfile::FoldCase, &["//nas/share/Music"]),
            root(2, NameProfile::Exact, &["/Volumes/Music"]),
        ];
        assert_eq!(
            resolve(&loc("//NAS/Share/Music/x.aif"), &roots),
            mapped(1, &["x.aif"], 3)
        );
        assert_eq!(
            resolve(&loc("/Volumes/Music/x.aif"), &roots),
            Resolution::Mapped {
                root_id: 2,
                alias_len: 2,
                remainder: RelPath::from_components([b"x.aif"], Flavor::Posix).unwrap(),
            }
        );
    }

    #[test]
    fn histogram_suggests_common_prefixes_without_accepting_any() {
        let locations: Vec<_> = [
            "/D:/Library/A/1.aif",
            "/D:/Library/A/2.aif",
            "/D:/Library/B/3.aif",
            "/C:/Users/dj/s.wav",
            "/D:/x.aif",
        ]
        .iter()
        .map(|text| loc(text))
        .collect();
        let top = prefix_histogram(&locations, 1);
        assert_eq!(top[0], (loc("/D:/Library"), 3));
        let deeper = prefix_histogram(&locations, 2);
        assert_eq!(deeper[0], (loc("/D:/Library/A"), 2));
        assert_eq!(prefix_histogram(&locations, 9).len(), 0);
    }

    #[test]
    fn the_decoded_form_round_trips_through_parse() {
        for text in [
            "/G:/Music/a b",
            "/G:",
            "//nas/share/x",
            "/Users/dj/Music",
            "/",
            "/x",
        ] {
            let path = loc(text);
            assert_eq!(path.to_decoded(), text.as_bytes(), "{text}");
            assert_eq!(LocationPath::parse(&path.to_decoded()).unwrap(), path);
        }
        assert_eq!(
            loc("/g:/Music").to_decoded(),
            b"/G:/Music",
            "drive letters are stored uppercase"
        );
    }

    #[test]
    fn shapes_and_aliases_round_trip_through_storage() {
        for shape in [Shape::Posix, Shape::Unc, Shape::Drive(b'G')] {
            assert_eq!(Shape::parse(&shape.to_text()), Some(shape));
        }
        for bad in ["", "drive:", "drive:g", "drive:GG", "DRIVE:G", "windows"] {
            assert_eq!(Shape::parse(bad), None, "{bad:?}");
        }
        for text in ["/G:/Music/a", "/G:", "//nas/share/x", "/Users/dj", "/"] {
            let alias = loc(text);
            let stored = alias.components_stored();
            let parts: Vec<Vec<u8>> = if stored.is_empty() {
                Vec::new()
            } else {
                stored.split(|b| *b == 0).map(<[u8]>::to_vec).collect()
            };
            assert_eq!(
                LocationPath::from_parts(alias.shape(), parts).unwrap(),
                alias
            );
        }
        assert!(LocationPath::from_parts(Shape::Posix, vec![b"..".to_vec()]).is_err());
        assert!(LocationPath::from_parts(Shape::Unc, vec![b"nas".to_vec()]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_unix_root_path_becomes_a_posix_alias() {
        let alias = LocationPath::from_os_path(Path::new("/Volumes/Music/Library")).unwrap();
        assert_eq!(alias, loc("/Volumes/Music/Library"));
        assert!(LocationPath::from_os_path(Path::new("Music")).is_err());
        assert!(LocationPath::from_os_path(Path::new("/a/../b")).is_err());
    }
}
