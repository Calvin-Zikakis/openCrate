//! The folder layout setting: how imported music is arranged inside a music folder. Stored in
//! the library database so every import follows it. A few presets cover the usual choices; any
//! pattern the template parser accepts can be used instead.

use oc_core_db::service::LibraryService;
use oc_organize::{Fields, Template, DEFAULT_TEMPLATE};
use serde::Serialize;

use crate::plan::LAYOUT_KEY;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresetDto {
    pub name: &'static str,
    pub layout: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutDto {
    pub layout: String,
    /// Where a sample track would go, for showing the choice before it is used.
    pub example: String,
    pub presets: Vec<PresetDto>,
    /// Whether ffmpeg was found, so converting on import can be offered.
    pub can_convert: bool,
}

pub const PRESETS: [PresetDto; 5] = [
    PresetDto {
        name: "Artist / Album / Track",
        layout: DEFAULT_TEMPLATE,
    },
    PresetDto {
        name: "Artist / Year - Album / Track",
        layout: "{folder_artist}/{year} - {release}/{track:02} - {artist} - {title}",
    },
    PresetDto {
        name: "Album / Track",
        layout: "{release}/{track:02} - {artist} - {title}",
    },
    PresetDto {
        name: "Artist / Track",
        layout: "{folder_artist}/{artist} - {title}",
    },
    PresetDto {
        name: "All in one folder",
        layout: "{artist} - {title}",
    },
];

fn sample() -> Fields {
    Fields {
        folder_artist: Some("Studio 001".into()),
        release: Some("Between Spaces".into()),
        artist: Some("Studio 001".into()),
        album_artist: Some("Studio 001".into()),
        remixer: None,
        title: Some("Night Drive".into()),
        year: Some(2019),
        track: Some(1),
        disc: Some(1),
    }
}

/// Check a pattern and show where the sample track would go under it.
fn example(layout: &str) -> Result<String, String> {
    let template =
        Template::parse(layout).map_err(|error| format!("That layout cannot be used: {error}."))?;
    let rendered = template
        .render(&sample(), "aif")
        .map_err(|error| format!("That layout cannot be used: {error}."))?;
    Ok(rendered.components().join("/"))
}

pub fn get(service: &LibraryService) -> Result<LayoutDto, String> {
    let layout = service
        .setting(LAYOUT_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| DEFAULT_TEMPLATE.to_owned());
    Ok(LayoutDto {
        example: example(&layout)?,
        layout,
        presets: PRESETS.to_vec(),
        can_convert: oc_convert::find_ffmpeg().is_some(),
    })
}

/// Save a layout after checking it. The default is stored as no setting at all.
pub fn set(service: &LibraryService, layout: &str) -> Result<LayoutDto, String> {
    let layout = layout.trim();
    example(layout)?;
    let stored = (layout != DEFAULT_TEMPLATE).then(|| layout.to_owned());
    service
        .set_setting(LAYOUT_KEY, stored)
        .map_err(|error| error.to_string())?;
    get(service)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_works_and_a_bad_layout_is_refused_without_changing_the_setting() {
        for preset in &PRESETS {
            assert!(example(preset.layout).is_ok(), "{}", preset.name);
        }
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let start = get(&service).unwrap();
        assert_eq!(start.layout, DEFAULT_TEMPLATE);
        assert_eq!(
            start.example,
            "Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif"
        );
        let chosen = set(&service, PRESETS[2].layout).unwrap();
        assert_eq!(
            chosen.example,
            "Between Spaces/01 - Studio 001 - Night Drive.aif"
        );
        let error = set(&service, "{nonsense}/{title}").unwrap_err();
        assert!(error.starts_with("That layout cannot be used"), "{error}");
        assert_eq!(get(&service).unwrap().layout, PRESETS[2].layout);
        set(&service, DEFAULT_TEMPLATE).unwrap();
        assert_eq!(service.setting(LAYOUT_KEY).unwrap(), None);
    }
}
