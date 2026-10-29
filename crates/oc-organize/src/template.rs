//! Path templates such as `{folder_artist}/{release}/{track:02} - {artist} - {title}`.
//!
//! A template is parsed once and rendered per track. Each `/` in the template starts a new
//! folder level; a `/` inside a field value never does, because values are sanitized
//! first. The last level is the file name.

use thiserror::Error;

use crate::sanitize::{sanitize_component, sanitize_file_name};

/// The default layout: `title` already includes the version.
pub const DEFAULT_TEMPLATE: &str = "{folder_artist}/{release}/{track:02} - {artist} - {title}";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    FolderArtist,
    Release,
    Artist,
    AlbumArtist,
    Remixer,
    Title,
    Year,
    Track,
    Disc,
}

impl Field {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "folder_artist" => Self::FolderArtist,
            "release" => Self::Release,
            "artist" => Self::Artist,
            "album_artist" => Self::AlbumArtist,
            "remixer" => Self::Remixer,
            "title" => Self::Title,
            "year" => Self::Year,
            "track" => Self::Track,
            "disc" => Self::Disc,
            _ => return None,
        })
    }

    const fn name(self) -> &'static str {
        match self {
            Self::FolderArtist => "folder_artist",
            Self::Release => "release",
            Self::Artist => "artist",
            Self::AlbumArtist => "album_artist",
            Self::Remixer => "remixer",
            Self::Title => "title",
            Self::Year => "year",
            Self::Track => "track",
            Self::Disc => "disc",
        }
    }

    const fn is_number(self) -> bool {
        matches!(self, Self::Year | Self::Track | Self::Disc)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fields {
    pub folder_artist: Option<String>,
    pub release: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub remixer: Option<String>,
    pub title: Option<String>,
    pub year: Option<u32>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("unknown template field `{0}`")]
    UnknownField(String),
    #[error("a width such as `:02` only applies to a number field, not `{0}`")]
    WidthOnText(&'static str),
    #[error("invalid width `{0}`")]
    BadWidth(String),
    #[error("unbalanced brace in the template")]
    Unbalanced,
    #[error("the template needs at least one level and every level needs content")]
    EmptyLevel,
    #[error("no value for `{0}`")]
    Missing(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Literal(String),
    Field { field: Field, width: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    levels: Vec<Vec<Part>>,
}

impl Template {
    pub fn parse(text: &str) -> Result<Self, TemplateError> {
        let mut levels = Vec::new();
        for level in text.split('/') {
            let mut parts = Vec::new();
            let mut literal = String::new();
            let mut chars = level.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '{' if chars.peek() == Some(&'{') => {
                        chars.next();
                        literal.push('{');
                    }
                    '}' if chars.peek() == Some(&'}') => {
                        chars.next();
                        literal.push('}');
                    }
                    '}' => return Err(TemplateError::Unbalanced),
                    '{' => {
                        let mut spec = String::new();
                        loop {
                            match chars.next() {
                                Some('}') => break,
                                Some(inner) => spec.push(inner),
                                None => return Err(TemplateError::Unbalanced),
                            }
                        }
                        if !literal.is_empty() {
                            parts.push(Part::Literal(std::mem::take(&mut literal)));
                        }
                        parts.push(parse_field(&spec)?);
                    }
                    other => literal.push(other),
                }
            }
            if !literal.is_empty() {
                parts.push(Part::Literal(literal));
            }
            if parts.is_empty() {
                return Err(TemplateError::EmptyLevel);
            }
            levels.push(parts);
        }
        Ok(Self { levels })
    }

    /// The folder components and the file name for one track.
    pub fn render(&self, fields: &Fields, extension: &str) -> Result<RenderedName, TemplateError> {
        let mut folders = Vec::new();
        let mut file = String::new();
        for (index, level) in self.levels.iter().enumerate() {
            let text = render_level(level, fields)?;
            if index + 1 == self.levels.len() {
                file = sanitize_file_name(&text, extension);
            } else {
                folders.push(sanitize_component(&text));
            }
        }
        Ok(RenderedName { folders, file })
    }
}

/// One folder or file name. A missing track, disc or year is left out with the separator after
/// it, or before it when it ends the name; a name left with nothing in it is still an error.
fn render_level(level: &[Part], fields: &Fields) -> Result<String, TemplateError> {
    let mut pieces = Vec::with_capacity(level.len());
    let mut first_missing = None;
    for part in level {
        pieces.push(match part {
            Part::Literal(literal) => Some(literal.clone()),
            Part::Field { field, width } => match value(fields, *field, *width) {
                Ok(text) => Some(text),
                Err(TemplateError::Missing(name)) if field.is_number() => {
                    first_missing.get_or_insert(name);
                    None
                }
                Err(error) => return Err(error),
            },
        });
    }
    let mut keep = vec![true; level.len()];
    for index in (0..level.len()).filter(|&index| pieces[index].is_none()) {
        keep[index] = false;
        let literal = |at: usize| matches!(level.get(at), Some(Part::Literal(_)));
        if literal(index + 1) {
            keep[index + 1] = false;
        } else if index > 0 && literal(index - 1) {
            keep[index - 1] = false;
        }
    }
    let text: String = pieces
        .into_iter()
        .zip(keep)
        .filter_map(|(piece, kept)| piece.filter(|_| kept))
        .collect();
    match first_missing {
        Some(name) if text.trim().is_empty() => Err(TemplateError::Missing(name)),
        _ => Ok(text),
    }
}

fn parse_field(spec: &str) -> Result<Part, TemplateError> {
    let (name, width) = match spec.split_once(':') {
        Some((name, width)) => (name, Some(width)),
        None => (spec, None),
    };
    let field =
        Field::parse(name.trim()).ok_or_else(|| TemplateError::UnknownField(name.to_owned()))?;
    let width = match width {
        None => 0,
        Some(_) if !field.is_number() => return Err(TemplateError::WidthOnText(field.name())),
        Some(text) => text
            .strip_prefix('0')
            .and_then(|digits| digits.parse::<usize>().ok())
            .filter(|width| (1..=9).contains(width))
            .ok_or_else(|| TemplateError::BadWidth(text.to_owned()))?,
    };
    Ok(Part::Field { field, width })
}

fn value(fields: &Fields, field: Field, width: usize) -> Result<String, TemplateError> {
    let text = |option: &Option<String>| {
        option
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or(TemplateError::Missing(field.name()))
    };
    let number = |option: Option<u32>| {
        option
            .map(|value| format!("{value:0width$}"))
            .ok_or(TemplateError::Missing(field.name()))
    };
    match field {
        Field::FolderArtist => text(&fields.folder_artist),
        Field::Release => text(&fields.release),
        Field::Artist => text(&fields.artist),
        Field::AlbumArtist => text(&fields.album_artist),
        Field::Remixer => text(&fields.remixer),
        Field::Title => text(&fields.title),
        Field::Year => number(fields.year),
        Field::Track => number(fields.track),
        Field::Disc => number(fields.disc),
    }
}

/// A destination below a library root, already sanitized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedName {
    pub folders: Vec<String>,
    pub file: String,
}

impl RenderedName {
    /// Every component in order, the file name last.
    #[must_use]
    pub fn components(&self) -> Vec<String> {
        self.folders
            .iter()
            .cloned()
            .chain(std::iter::once(self.file.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Fields {
        Fields {
            folder_artist: Some("Studio 001".into()),
            release: Some("Between Spaces".into()),
            artist: Some("Studio 001".into()),
            title: Some("Night Drive (X Remix)".into()),
            track: Some(3),
            year: Some(2019),
            ..Fields::default()
        }
    }

    #[test]
    fn the_default_template_gives_the_owners_layout() {
        let template = Template::parse(DEFAULT_TEMPLATE).unwrap();
        let name = template.render(&fields(), "aif").unwrap();
        assert_eq!(name.folders, ["Studio 001", "Between Spaces"]);
        assert_eq!(name.file, "03 - Studio 001 - Night Drive (X Remix).aif");
        assert_eq!(name.components().len(), 3);
    }

    #[test]
    fn values_are_sanitized_and_a_slash_never_adds_a_level() {
        let mut tricky = fields();
        tricky.folder_artist = Some("AC/DC".into());
        tricky.release = Some("Who? What: Where.".into());
        tricky.title = Some("Con".into());
        let name = Template::parse(DEFAULT_TEMPLATE)
            .unwrap()
            .render(&tricky, "mp3")
            .unwrap();
        assert_eq!(name.folders, ["AC_DC", "Who_ What_ Where_"]);
        assert_eq!(name.file, "03 - Studio 001 - Con.mp3");
        tricky.title = Some(String::new());
        tricky.artist = None;
        assert!(Template::parse("{title}")
            .unwrap()
            .render(&tricky, "mp3")
            .is_err());
    }

    #[test]
    fn widths_literals_braces_and_other_layouts() {
        let template = Template::parse("{year}/{disc:02}-{track:03} {{raw}} {title}").unwrap();
        let mut f = fields();
        f.disc = Some(1);
        let name = template.render(&f, "wav").unwrap();
        assert_eq!(name.folders, ["2019"]);
        assert_eq!(name.file, "01-003 {raw} Night Drive (X Remix).wav");
        assert_eq!(
            Template::parse("{track}")
                .unwrap()
                .render(&f, "aif")
                .unwrap()
                .file,
            "3.aif"
        );
    }

    #[test]
    fn a_missing_number_is_left_out_with_its_separator() {
        let mut f = fields();
        f.track = None;
        f.year = None;
        f.disc = None;
        let render = |text: &str| Template::parse(text).unwrap().render(&f, "aif");
        let plain = render(DEFAULT_TEMPLATE).unwrap();
        assert_eq!(plain.file, "Studio 001 - Night Drive (X Remix).aif");
        assert_eq!(
            render("{folder_artist}/{year} - {release}/{artist} - {title} - {year}")
                .unwrap()
                .components()[1..],
            ["Between Spaces", "Studio 001 - Night Drive (X Remix).aif"]
        );
        assert_eq!(
            render("{disc}-{track:02} {title}").unwrap().file,
            "Night Drive (X Remix).aif"
        );
        assert_eq!(
            render("{folder_artist}/{year}/{title}"),
            Err(TemplateError::Missing("year")),
            "a folder made of nothing but a missing number is still an error"
        );
    }

    #[test]
    fn a_missing_value_is_an_error_not_a_blank() {
        let mut f = fields();
        f.release = Some("   ".into());
        assert_eq!(
            Template::parse(DEFAULT_TEMPLATE).unwrap().render(&f, "aif"),
            Err(TemplateError::Missing("release"))
        );
    }

    #[test]
    fn bad_templates_are_refused() {
        for (text, error) in [
            ("{nope}", TemplateError::UnknownField("nope".into())),
            ("{title:02}", TemplateError::WidthOnText("title")),
            ("{track:2}", TemplateError::BadWidth("2".into())),
            ("{track:00}", TemplateError::BadWidth("00".into())),
            ("{track:0x}", TemplateError::BadWidth("0x".into())),
            ("{title", TemplateError::Unbalanced),
            ("title}", TemplateError::Unbalanced),
            ("", TemplateError::EmptyLevel),
            ("{artist}//{title}", TemplateError::EmptyLevel),
            ("{artist}/", TemplateError::EmptyLevel),
        ] {
            assert_eq!(Template::parse(text), Err(error), "{text:?}");
        }
    }

    #[test]
    fn a_single_level_template_has_only_a_file_name() {
        let name = Template::parse("{artist} - {title}")
            .unwrap()
            .render(&fields(), "flac")
            .unwrap();
        assert_eq!(name.folders.len(), 0);
        assert_eq!(name.file, "Studio 001 - Night Drive (X Remix).flac");
    }
}
