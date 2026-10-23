//! Pure naming rules for the library: sanitizing, path templates, Windows path limits and
//! title and version parsing. No file system access and no database; a later slice that
//! moves files will use these to decide where a file belongs, and never the other way round.

pub mod fit;
pub mod sanitize;
pub mod template;
pub mod title;

pub use fit::{fit_destination, utf16_len, FitError, Fitted, PathBudget};
pub use sanitize::{sanitize_component, sanitize_extension, sanitize_file_name};
pub use template::{Field, Fields, RenderedName, Template, TemplateError, DEFAULT_TEMPLATE};
pub use title::{
    folder_artist, full_title, parse_title, same_version, version_key, ParsedTitle, RemixKind,
    ORIGINAL_MIX,
};

use thiserror::Error;

/// Where a track belongs below a library root, ready to show in a plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    /// Folders, then the file name.
    pub components: Vec<String>,
    /// The file stem was shortened to fit Windows' limit; the plan must say so.
    pub stem_shortened: bool,
    /// UTF-16 units of the whole path, root included.
    pub length: usize,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DestinationError {
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    Fit(#[from] FitError),
}

/// Render the template for one track, then fit the result below `root` (the library root as
/// the operating system spells it, without a trailing separator).
pub fn destination(
    template: &Template,
    fields: &Fields,
    extension: &str,
    root: &str,
    budget: &PathBudget,
) -> Result<Destination, DestinationError> {
    let rendered = template.render(fields, extension)?;
    let fitted = fit_destination(root, &rendered.components(), budget)?;
    Ok(Destination {
        components: fitted.components,
        stem_shortened: fitted.stem_shortened,
        length: fitted.length,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(title: &str) -> Fields {
        let parsed = parse_title(title);
        Fields {
            folder_artist: Some(folder_artist(&parsed, None, "Studio 001")),
            release: Some("Between Spaces".into()),
            artist: Some("Studio 001".into()),
            title: Some(full_title(&parsed)),
            track: Some(7),
            ..Fields::default()
        }
    }

    #[test]
    fn a_track_is_rendered_and_fitted_in_one_step() {
        let template = Template::parse(DEFAULT_TEMPLATE).unwrap();
        let made = destination(
            &template,
            &fields("Night Drive"),
            "aif",
            r"G:\Library",
            &PathBudget::classic(),
        )
        .unwrap();
        assert_eq!(
            made.components,
            [
                "Studio 001",
                "Between Spaces",
                "07 - Studio 001 - Night Drive.aif"
            ]
        );
        assert!(!made.stem_shortened);
    }

    #[test]
    fn a_remix_goes_to_the_remixers_folder_and_a_long_title_is_shortened_and_flagged() {
        let template = Template::parse(DEFAULT_TEMPLATE).unwrap();
        let remix = destination(
            &template,
            &fields("Night Drive (X Remix)"),
            "aif",
            r"G:\Library",
            &PathBudget::classic(),
        )
        .unwrap();
        assert_eq!(remix.components[0], "X");
        assert_eq!(
            remix.components[2],
            "07 - Studio 001 - Night Drive (X Remix).aif"
        );

        let long = format!("{} (X Remix)", "Very Long Title ".repeat(12));
        let root = format!(r"G:\{}", "Library".repeat(22));
        let made = destination(
            &template,
            &fields(&long),
            "aif",
            &root,
            &PathBudget::classic(),
        )
        .unwrap();
        assert!(made.stem_shortened);
        assert!(made.components[2].to_lowercase().ends_with(".aif"));
        assert!(made.length <= fit::CLASSIC_LIMIT - fit::TEMP_SUFFIX_RESERVE);
    }

    #[test]
    fn problems_come_back_as_errors_with_their_cause() {
        let template = Template::parse(DEFAULT_TEMPLATE).unwrap();
        let mut missing = fields("Night Drive");
        missing.track = None;
        assert_eq!(
            destination(&template, &missing, "aif", "G:", &PathBudget::classic()),
            Err(DestinationError::Template(TemplateError::Missing("track")))
        );
        let root = format!(r"G:\{}", "r".repeat(250));
        assert!(matches!(
            destination(
                &template,
                &fields("Night Drive"),
                "aif",
                &root,
                &PathBudget::classic()
            ),
            Err(DestinationError::Fit(FitError::FoldersTooLong { .. }))
        ));
    }
}
