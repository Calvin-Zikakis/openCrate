//! The two-way merge with rekordbox, Git style: review what differs since both sides last
//! agreed, then apply the owner's ticked choices. A value kept from rekordbox goes into the
//! file's tags (through a copy, never in place); a value kept from openCrate goes into
//! rekordbox's library, which needs writing switched on. Nothing is applied that the owner did
//! not tick, and a choice made on values that have changed since the review is skipped.

use std::collections::BTreeMap;

use oc_core_db::merge::{Change, Direction, Field};
use oc_core_db::now_ms;
use oc_core_db::service::{LibraryService, ScanOptions};
use oc_rekordbox::db::write::{self, FieldEdit, TrackText};
use oc_tag::TextTag;
use serde::{Deserialize, Serialize};

use crate::rekordbox_db::{self, Probe};
use crate::rekordbox_write;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDto {
    pub rb_key: i64,
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub field: &'static str,
    pub base: Option<String>,
    pub ours: String,
    pub theirs: String,
    /// `toRekordbox`, `toOpenCrate` or `conflict`.
    pub direction: &'static str,
}

impl From<&Change> for ChangeDto {
    fn from(change: &Change) -> Self {
        Self {
            rb_key: change.rb_key,
            track_id: change.track_id,
            title: change.title.clone(),
            artist: change.artist.clone(),
            field: change.field.as_str(),
            base: change.base.clone(),
            ours: change.ours.clone(),
            theirs: change.theirs.clone(),
            direction: match change.direction {
                Direction::ToRekordbox => "toRekordbox",
                Direction::ToOpenCrate => "toOpenCrate",
                Direction::Conflict => "conflict",
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDto {
    pub changes: Vec<ChangeDto>,
    /// Writing to rekordbox is on and ready, so openCrate's values can go to rekordbox.
    pub can_write: bool,
}

/// Which side's value to keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Keep {
    OpenCrate,
    Rekordbox,
}

/// One ticked change, with the values the owner saw.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceDto {
    pub rb_key: i64,
    pub field: String,
    pub keep: Keep,
    pub ours: String,
    pub theirs: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedDto {
    pub to_rekordbox: usize,
    pub to_opencrate: usize,
    /// Choices not applied, and why.
    pub skipped: Vec<String>,
}

fn reading_on(service: &LibraryService, probe: &Probe) -> Result<(), String> {
    let state = rekordbox_db::status(service, probe)?;
    if !state.enabled || state.needs_confirmation {
        return Err(
            "The merge works from a direct read of rekordbox's library. Turn reading on in \
             Settings first."
                .into(),
        );
    }
    Ok(())
}

/// Read rekordbox again and list what differs.
pub fn review(service: &LibraryService, probe: &Probe) -> Result<ReviewDto, String> {
    reading_on(service, probe)?;
    rekordbox_db::read(service, probe)?;
    let changes = service.merge_review().map_err(|error| error.to_string())?;
    let write = rekordbox_write::state(service, probe)?;
    Ok(ReviewDto {
        changes: changes.iter().map(ChangeDto::from).collect(),
        can_write: write.enabled && !write.needs_confirmation && write.tested,
    })
}

/// A tag change for one file, and what it makes agreed once the file is retagged.
type FileChange = (TextTag, String, Agreed);
/// A field both sides hold the same value for: rekordbox's id, the field, the value.
type Agreed = (i64, Field, String);

const fn tag(field: Field) -> TextTag {
    match field {
        Field::Title => TextTag::Title,
        Field::Artist => TextTag::Artist,
        Field::Album => TextTag::Album,
        Field::Year => TextTag::Year,
        Field::Label => TextTag::Label,
    }
}

const fn text(field: Field) -> TrackText {
    match field {
        Field::Title => TrackText::Title,
        Field::Artist => TrackText::Artist,
        Field::Album => TrackText::Album,
        Field::Year => TrackText::Year,
        Field::Label => TrackText::Label,
    }
}

/// Apply the owner's choices. Values for rekordbox are written first, in one write after one
/// backup; then each file to change is retagged and its folder read again.
pub fn apply(
    service: &LibraryService,
    probe: &Probe,
    choices: &[ChoiceDto],
) -> Result<AppliedDto, String> {
    reading_on(service, probe)?;
    // Compared with the state now, without reading rekordbox again: the review just did.
    let current = service.merge_review().map_err(|error| error.to_string())?;
    let mut skipped = Vec::new();
    let mut to_rekordbox = Vec::new();
    let mut to_files: BTreeMap<i64, Vec<FileChange>> = BTreeMap::new();
    let mut agreed = Vec::new();
    for choice in choices {
        let Some(field) = Field::parse(&choice.field) else {
            skipped.push(format!("{} is not a field the merge knows.", choice.field));
            continue;
        };
        let found = current.iter().find(|change| {
            change.rb_key == choice.rb_key
                && change.field == field
                && change.ours == choice.ours
                && change.theirs == choice.theirs
        });
        let Some(change) = found else {
            skipped.push(format!(
                "{} of track {}: changed since the review, so left alone. Review again.",
                choice.field, choice.rb_key
            ));
            continue;
        };
        match choice.keep {
            Keep::OpenCrate => {
                to_rekordbox.push(FieldEdit {
                    content: change.rb_key.to_string(),
                    field: text(field),
                    value: change.ours.clone(),
                    expected: Some(change.theirs.clone()),
                });
                agreed.push((change.rb_key, field, change.ours.clone()));
            }
            Keep::Rekordbox => {
                to_files.entry(change.file_id).or_default().push((
                    tag(field),
                    change.theirs.clone(),
                    (change.rb_key, field, change.theirs.clone()),
                ));
            }
        }
    }

    if !to_rekordbox.is_empty() {
        let location = rekordbox_write::ready(service, probe).map_err(|error| {
            format!("Keeping openCrate's values means writing to rekordbox. {error}")
        })?;
        write::set_fields(
            location,
            &rekordbox_db::backups_dir(service),
            probe.running_now(),
            now_ms(),
            &to_rekordbox,
        )
        .map_err(|error| error.to_string())?;
    }

    let mut retagged = 0;
    let mut roots = Vec::new();
    for (file_id, changes) in &to_files {
        let location = service
            .file_location(*file_id)
            .map_err(|error| error.to_string())?;
        let Some((root_id, path)) = location else {
            skipped.push(format!(
                "File {file_id} cannot be reached, so its tags were not changed."
            ));
            continue;
        };
        let tags: Vec<(TextTag, String)> = changes
            .iter()
            .map(|(tag, value, _)| (*tag, value.clone()))
            .collect();
        match oc_tag::rewrite_tags(&path, &tags) {
            Ok(()) => {
                retagged += changes.len();
                agreed.extend(changes.iter().map(|(_, _, agreed)| agreed.clone()));
                if !roots.contains(&root_id) {
                    roots.push(root_id);
                }
            }
            Err(error) => skipped.push(format!("{}: {error}", path.display())),
        }
    }
    for root_id in roots {
        service
            .scan_root(root_id, &ScanOptions::default())
            .map_err(|error| error.to_string())?;
    }
    service
        .record_merge_base(agreed)
        .map_err(|error| error.to_string())?;
    if !to_rekordbox.is_empty() {
        // So openCrate shows what rekordbox now holds; the write itself was already checked.
        if let Err(error) = rekordbox_db::read(service, probe) {
            skipped.push(format!(
                "rekordbox was written, but reading it again did not work: {error}"
            ));
        }
    }
    Ok(AppliedDto {
        to_rekordbox: to_rekordbox.len(),
        to_opencrate: retagged,
        skipped,
    })
}
