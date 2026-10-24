//! Writing to rekordbox's own library, behind a second setting on top of reading it: tracks
//! dropped on a rekordbox playlist go into it, and new tracks go into the rolling "Imported"
//! playlist. Every write needs rekordbox closed and a version the writer is known to work with,
//! is preceded by a backup listed in Settings, and is followed by a fresh read, so openCrate
//! shows what rekordbox now holds.

use std::path::PathBuf;

use oc_core_db::now_ms;
use oc_core_db::offers::RekordboxMember;
use oc_core_db::service::LibraryService;
use oc_rekordbox::db::write::{self, Member, Playlist};
use oc_rekordbox::db::LibraryLocation;
use serde::{Deserialize, Serialize};

use crate::rekordbox_db::{self, Probe};

/// Holds the rekordbox version the owner confirmed the write warning for.
const SETTING: &str = "rekordbox_database_write";
/// The top-level playlist every new track openCrate adds to rekordbox goes into.
pub const IMPORTED: &str = "Imported";

/// rekordbox versions the writer is known to work with: those `rbl-db`'s writes were observed
/// against (7.2.11, 7.2.14 and 7.2.19).
pub fn tested(version: &str) -> bool {
    version.starts_with("7.2.")
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Confirmation {
    confirmed_version: String,
}

/// The write setting as the page shows it.
pub struct WriteState {
    pub enabled: bool,
    pub confirmed_version: Option<String>,
    pub needs_confirmation: bool,
    pub tested: bool,
}

pub fn state(service: &LibraryService, probe: &Probe) -> Result<WriteState, String> {
    let confirmed_version = service
        .setting(SETTING)
        .map_err(|error| error.to_string())?
        .and_then(|text| serde_json::from_str::<Confirmation>(&text).ok())
        .map(|confirmation| confirmation.confirmed_version);
    Ok(WriteState {
        enabled: confirmed_version.is_some(),
        needs_confirmation: confirmed_version
            .as_deref()
            .is_some_and(|version| version != probe.version()),
        confirmed_version,
        tested: tested(probe.version()),
    })
}

/// Clears the setting; reading being turned off clears writing with it.
pub fn clear(service: &LibraryService) -> Result<(), String> {
    service
        .set_setting(SETTING, None)
        .map_err(|error| error.to_string())
}

/// Turning it on needs reading on (and confirmed) and a tested rekordbox version, and records
/// the version the warning was confirmed for.
pub fn set_enabled(
    service: &LibraryService,
    probe: &Probe,
    enabled: bool,
) -> Result<rekordbox_db::RekordboxDbDto, String> {
    if !enabled {
        clear(service)?;
        return rekordbox_db::status(service, probe);
    }
    let reading = rekordbox_db::status(service, probe)?;
    if !reading.enabled || reading.needs_confirmation {
        return Err("Turn on reading rekordbox's library first.".into());
    }
    if !tested(probe.version()) {
        return Err(untested(probe.version()));
    }
    let value = serde_json::to_string(&Confirmation {
        confirmed_version: probe.version().to_owned(),
    })
    .map_err(|error| error.to_string())?;
    service
        .set_setting(SETTING, Some(value))
        .map_err(|error| error.to_string())?;
    rekordbox_db::status(service, probe)
}

fn untested(version: &str) -> String {
    format!(
        "openCrate has not been tested writing to rekordbox {version}, so it will not write to it. \
         Reading still works, and new tracks can reach rekordbox through openCrate's xml file."
    )
}

/// The library to write to, once every gate is passed.
pub(crate) fn ready<'a>(
    service: &LibraryService,
    probe: &'a Probe,
) -> Result<&'a LibraryLocation, String> {
    let state = state(service, probe)?;
    if !state.enabled {
        return Err("Writing to rekordbox is off. Turn it on in Settings.".into());
    }
    if state.needs_confirmation {
        return Err(format!(
            "rekordbox is now version {}. Confirm the warning in Settings again before writing.",
            probe.version()
        ));
    }
    if !state.tested {
        return Err(untested(probe.version()));
    }
    let reading = rekordbox_db::status(service, probe)?;
    if !reading.enabled || reading.needs_confirmation {
        return Err("Reading rekordbox's library is off. Turn it on in Settings.".into());
    }
    rekordbox_db::installed(probe)
}

/// What a write did, for the page.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteDto {
    pub playlist: String,
    pub added: usize,
    pub imported: usize,
    /// Tracks that could not go in, with why.
    pub left_out: Vec<String>,
    /// Set when the write worked but reading rekordbox again afterwards did not.
    pub read_error: Option<String>,
}

fn write(
    service: &LibraryService,
    probe: &Probe,
    location: &LibraryLocation,
    playlist: &Playlist,
    name: String,
    members: &[Member],
    mut left_out: Vec<String>,
) -> Result<WriteDto, String> {
    if members.is_empty() {
        return Ok(WriteDto {
            playlist: name,
            added: 0,
            imported: 0,
            left_out,
            read_error: None,
        });
    }
    let done = write::add_to_playlist(
        location,
        &rekordbox_db::backups_dir(service),
        probe.running(),
        now_ms(),
        playlist,
        members,
    )
    .map_err(|error| error.to_string())?;
    left_out.extend(
        done.refused
            .iter()
            .map(|(path, why)| format!("{}: {why}", path.display())),
    );
    // So the sidebar and the new-tracks list show what rekordbox now holds.
    let read_error = rekordbox_db::read(service, probe).err();
    Ok(WriteDto {
        playlist: name,
        added: done.added,
        imported: done.imported,
        left_out,
        read_error,
    })
}

/// Put library tracks at the end of a rekordbox playlist shown in the sidebar.
pub fn add_to_playlist(
    service: &LibraryService,
    probe: &Probe,
    playlist_id: i64,
    track_ids: &[i64],
) -> Result<WriteDto, String> {
    let location = ready(service, probe)?;
    let key = service
        .rekordbox_playlist_key(playlist_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            "This playlist came from an xml export. Read rekordbox's library directly on the \
             rekordbox screen first, so openCrate knows rekordbox's own playlists."
                .to_owned()
        })?;
    let name = service
        .playlists()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|row| row.id == playlist_id)
        .map(|row| row.name)
        .unwrap_or_default();
    let mut members = Vec::with_capacity(track_ids.len());
    let mut left_out = Vec::new();
    let found = service
        .rekordbox_members(track_ids)
        .map_err(|error| error.to_string())?;
    for (track_id, member) in track_ids.iter().zip(found) {
        match member {
            RekordboxMember::Known(key) => members.push(Member::Known(key)),
            RekordboxMember::File(path) => members.push(Member::File(path)),
            RekordboxMember::NoFile => {
                left_out.push(format!("Track {track_id} has no file that can be reached."));
            }
        }
    }
    write(
        service,
        probe,
        location,
        &Playlist::Id(key),
        name,
        &members,
        left_out,
    )
}

/// New tracks rekordbox does not have yet go into rekordbox, in the "Imported" playlist: the
/// ones in `only` when given (the owner's ticks), else every one the owner has not kept out.
pub fn send_new(
    service: &LibraryService,
    probe: &Probe,
    only: Option<&[i64]>,
) -> Result<WriteDto, String> {
    let location = ready(service, probe)?;
    let (found, _) = service
        .rekordbox_candidates()
        .map_err(|error| error.to_string())?;
    let mut members = Vec::new();
    let mut left_out = Vec::new();
    let wanted = |file_id: i64| only.is_none_or(|ids| ids.contains(&file_id));
    for candidate in found
        .into_iter()
        .filter(|candidate| !candidate.hidden && wanted(candidate.file_id))
    {
        match oc_paths::bytes_to_os_string(&candidate.path) {
            Ok(path) => members.push(Member::File(PathBuf::from(path))),
            Err(_) => left_out.push(format!(
                "{}: its path cannot be read here.",
                candidate.title
            )),
        }
    }
    write(
        service,
        probe,
        location,
        &Playlist::TopLevel(IMPORTED.to_owned()),
        IMPORTED.to_owned(),
        &members,
        left_out,
    )
}
