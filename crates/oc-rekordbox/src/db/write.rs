//! Writing to rekordbox's own library, behind the write setting. rekordbox must be closed; a
//! backup openCrate lists (and can restore) is taken before anything is written; and what was
//! meant is read back afterwards.

use std::path::{Path, PathBuf};

use rbl_db::write::{ImportOutcome, TrackField, Writer, ATTRIBUTE_FOLDER, ATTRIBUTE_SMART, ROOT};
use rbl_db::{DbError, Library, LibraryLocation, OpenMode};
use rusqlite::{params, OptionalExtension};

use super::{take_snapshot, Snapshot};

/// A track to put in a playlist: one rekordbox has, by its id, or a file it may not have yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    Known(String),
    File(PathBuf),
}

/// Which playlist: one rekordbox has, by its id, or the one with this name at the top of the
/// tree, made if it is missing (the rolling "Imported" playlist).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Playlist {
    Id(String),
    TopLevel(String),
}

/// What a write did.
#[derive(Debug)]
pub struct Added {
    pub playlist: String,
    /// Tracks put in the playlist; ones already in it are left as they are.
    pub added: usize,
    /// Files that were new to rekordbox.
    pub imported: usize,
    /// Files rekordbox could not take, and why. The rest still go in.
    pub refused: Vec<(PathBuf, String)>,
    /// The copy taken before writing.
    pub backup: Snapshot,
}

/// Put `members` at the end of `playlist`, adding files rekordbox does not have yet.
/// `running` is the caller's check for rekordbox and its agent.
pub fn add_to_playlist(
    location: &LibraryLocation,
    backups: &Path,
    running: bool,
    now_ms: i64,
    playlist: &Playlist,
    members: &[Member],
) -> Result<Added, DbError> {
    // Refused before anything is copied; the snapshot refuses a running rekordbox too.
    if running {
        return Err(refused(
            "rekordbox is running. Quit it (and wait for its agent to stop) first.",
        ));
    }
    check_before(location, Some(playlist), members)?;
    let backup = take_snapshot(location, backups, "before-write", false, now_ms)?;
    let mut writer = Writer::open(location.clone(), backups)?;
    // openCrate's own backup, just taken, is the one listed and restored.
    writer.disable_automatic_backups();

    let files: Vec<PathBuf> = members
        .iter()
        .filter_map(|member| match member {
            Member::File(path) => Some(path.clone()),
            Member::Known(_) => None,
        })
        .collect();
    let outcomes = if files.is_empty() {
        Vec::new()
    } else {
        writer.import_files(&files)?
    };
    let mut ids = Vec::with_capacity(members.len());
    let (mut imported, mut refused_files) = (0, Vec::new());
    let mut outcome = outcomes.into_iter();
    for member in members {
        match member {
            Member::Known(id) => ids.push(id.clone()),
            Member::File(path) => match outcome.next() {
                Some(ImportOutcome::Added(id)) => {
                    imported += 1;
                    ids.push(id);
                }
                Some(ImportOutcome::Existing(id)) => ids.push(id),
                Some(ImportOutcome::Refused(reason)) => refused_files.push((path.clone(), reason)),
                None => refused_files.push((path.clone(), "not imported".into())),
            },
        }
    }

    let playlist_id = match playlist {
        Playlist::Id(id) => id.clone(),
        Playlist::TopLevel(name) => match writer.child_named(ROOT, name)? {
            Some(id) => id,
            None => writer.create_playlist(name, ROOT)?,
        },
    };
    let changed = writer.add_tracks(&playlist_id, &ids)?;
    drop(writer);

    verify(location, &playlist_id, &ids)?;
    Ok(Added {
        playlist: playlist_id,
        added: changed.rows,
        imported,
        refused: refused_files,
        backup,
    })
}

/// A track field the merge with openCrate can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackText {
    Title,
    Artist,
    Album,
    Year,
    Label,
}

impl TrackText {
    const fn field(self) -> TrackField {
        match self {
            Self::Title => TrackField::Title,
            Self::Artist => TrackField::Artist,
            Self::Album => TrackField::Album,
            Self::Year => TrackField::Year,
            Self::Label => TrackField::Label,
        }
    }
}

/// One field of one track, set to `value`; empty clears it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldEdit {
    pub content: String,
    pub field: TrackText,
    pub value: String,
}

/// Set track fields in rekordbox's library, then read every one back. Returns the backup taken
/// just before.
pub fn set_fields(
    location: &LibraryLocation,
    backups: &Path,
    running: bool,
    now_ms: i64,
    edits: &[FieldEdit],
) -> Result<Snapshot, DbError> {
    if running {
        return Err(refused(
            "rekordbox is running. Quit it (and wait for its agent to stop) first.",
        ));
    }
    let known: Vec<Member> = edits
        .iter()
        .map(|edit| Member::Known(edit.content.clone()))
        .collect();
    check_before(location, None, &known)?;
    let backup = take_snapshot(location, backups, "before-write", false, now_ms)?;
    let mut writer = Writer::open(location.clone(), backups)?;
    writer.disable_automatic_backups();
    for edit in edits {
        writer.set_field(&edit.content, edit.field.field(), edit.value.trim())?;
    }
    drop(writer);

    let library = Library::open(location.clone(), OpenMode::ReadOnly)?;
    for edit in edits {
        let stored: Option<String> = library.connection().query_row(
            "SELECT CASE ?2
                WHEN 'title' THEN c.Title
                WHEN 'artist' THEN (SELECT Name FROM djmdArtist WHERE ID = c.ArtistID)
                WHEN 'album' THEN (SELECT Name FROM djmdAlbum WHERE ID = c.AlbumID)
                WHEN 'label' THEN (SELECT Name FROM djmdLabel WHERE ID = c.LabelID)
                ELSE NULLIF(CAST(c.ReleaseYear AS TEXT), '0') END
             FROM djmdContent c WHERE c.ID = ?1",
            params![edit.content, format!("{:?}", edit.field).to_lowercase()],
            |row| row.get(0),
        )?;
        if stored.unwrap_or_default().trim() != edit.value.trim() {
            return Err(DbError::WriteRefused(format!(
                "After writing, track {}'s {:?} is not what was meant. Restore the backup taken \
                 just before from Settings.",
                edit.content, edit.field
            )));
        }
    }
    Ok(backup)
}

/// Tell rekordbox that tracks' files have moved: each `(id, new path)`. Only the location
/// changes; analysis, cues, My Tags and playlist membership stay with the track. Every new path
/// must be a file before anything is written; if anything goes wrong after the backup, rekordbox's
/// library is put back from it. Returns the backup.
pub fn relocate(
    location: &LibraryLocation,
    backups: &Path,
    running: bool,
    now_ms: i64,
    moves: &[(String, PathBuf)],
) -> Result<Snapshot, DbError> {
    if running {
        return Err(refused(
            "rekordbox is running. Quit it (and wait for its agent to stop) first.",
        ));
    }
    if let Some((_, path)) = moves.iter().find(|(_, path)| !path.is_file()) {
        return Err(refused(&format!(
            "{} is not a file; nothing was changed.",
            path.display()
        )));
    }
    let known: Vec<Member> = moves
        .iter()
        .map(|(content, _)| Member::Known(content.clone()))
        .collect();
    check_before(location, None, &known)?;
    let backup = take_snapshot(location, backups, "before-write", false, now_ms)?;
    let written = (|| {
        let mut writer = Writer::open(location.clone(), backups)?;
        writer.disable_automatic_backups();
        for (content, path) in moves {
            writer.relocate(content, &stored_form(path))?;
        }
        drop(writer);
        let library = Library::open(location.clone(), OpenMode::ReadOnly)?;
        for (content, path) in moves {
            let stored: String = library.connection().query_row(
                "SELECT COALESCE(FolderPath, '') FROM djmdContent WHERE ID = ?1",
                params![content],
                |row| row.get(0),
            )?;
            if stored != stored_form(path).to_string_lossy() {
                return Err(DbError::WriteRefused(format!(
                    "After writing, track {content} does not point at {}.",
                    path.display()
                )));
            }
        }
        Ok(())
    })();
    if let Err(error) = written {
        // Put rekordbox back exactly as it was before this write.
        if let Err(undo) = super::restore(&backup.dir, location, backups, false, now_ms + 1) {
            return Err(DbError::WriteRefused(format!(
                "{error} Putting rekordbox back from the backup also failed ({undo}); restore it \
                 from Settings."
            )));
        }
        return Err(error);
    }
    Ok(backup)
}

/// Two entries in rekordbox for the same recording: `dropped` gives way to `kept`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duplicate {
    pub kept: String,
    pub dropped: String,
}

fn ids(connection: &rusqlite::Connection, sql: &str, key: &str) -> Result<Vec<String>, DbError> {
    let mut statement = connection.prepare(sql)?;
    let found = statement
        .query_map(params![key], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(found)
}

const PLAYLISTS_OF: &str =
    "SELECT DISTINCT PlaylistID FROM djmdSongPlaylist WHERE ContentID = ?1 AND rb_local_deleted = 0";
const MEMBERS_OF: &str =
    "SELECT ContentID FROM djmdSongPlaylist WHERE PlaylistID = ?1 AND rb_local_deleted = 0 ORDER BY TrackNo, ID";
const TAGS_OF: &str =
    "SELECT MyTagID FROM djmdSongMyTag WHERE ContentID = ?1 AND rb_local_deleted = 0 ORDER BY TrackNo, MyTagID";

fn count(connection: &rusqlite::Connection, sql: &str, key: &str) -> Result<i64, DbError> {
    Ok(connection.query_row(sql, params![key], |row| row.get(0))?)
}

/// Merge duplicate entries in rekordbox. Every playlist holding the dropped entry gets the kept
/// one in the same place (or keeps the one it already has); the kept entry gains the dropped one's
/// My Tags, and its rating when it has none; then the dropped entry is removed (rekordbox's own
/// soft delete). Refused before anything is written when the dropped entry has cues the kept one
/// lacks, or sits in a playlist that holds a track twice. A backup comes first, and any failure
/// after it puts rekordbox back. Returns the backup.
pub fn merge_duplicates(
    location: &LibraryLocation,
    backups: &Path,
    running: bool,
    now_ms: i64,
    merges: &[Duplicate],
) -> Result<Snapshot, DbError> {
    if running {
        return Err(refused(
            "rekordbox is running. Quit it (and wait for its agent to stop) first.",
        ));
    }
    let known: Vec<Member> = merges
        .iter()
        .flat_map(|merge| {
            [
                Member::Known(merge.kept.clone()),
                Member::Known(merge.dropped.clone()),
            ]
        })
        .collect();
    check_before(location, None, &known)?;
    {
        let library = Library::open(location.clone(), OpenMode::ReadOnly)?;
        let connection = library.connection();
        let cues = "SELECT COUNT(*) FROM djmdCue WHERE ContentID = ?1 AND rb_local_deleted = 0";
        for merge in merges {
            if merge.kept == merge.dropped {
                return Err(refused("A track cannot be merged with itself."));
            }
            if count(connection, cues, &merge.dropped)? > 0
                && count(connection, cues, &merge.kept)? == 0
            {
                return Err(refused(&format!(
                    "Track {} has cues the copy kept lacks. Keep that copy instead, or set the cues \
                     on the other in rekordbox first. Nothing was changed.",
                    merge.dropped
                )));
            }
            for playlist in ids(connection, PLAYLISTS_OF, &merge.dropped)? {
                let members = ids(connection, MEMBERS_OF, &playlist)?;
                let distinct: std::collections::HashSet<&String> = members.iter().collect();
                if distinct.len() != members.len() {
                    return Err(refused(&format!(
                        "Playlist {playlist} holds a track more than once; merging there could lose \
                         a place. Nothing was changed."
                    )));
                }
            }
        }
    }
    let backup = take_snapshot(location, backups, "before-write", false, now_ms)?;
    let written = (|| {
        let mut writer = Writer::open(location.clone(), backups)?;
        writer.disable_automatic_backups();
        let mut places: Vec<(String, String)> = Vec::new();
        for merge in merges {
            let connection = writer.library().connection();
            let playlists = ids(connection, PLAYLISTS_OF, &merge.dropped)?;
            let mut orders = Vec::with_capacity(playlists.len());
            for playlist in &playlists {
                let members = ids(connection, MEMBERS_OF, playlist)?;
                let holds_kept = members.contains(&merge.kept);
                let order: Vec<String> = members
                    .into_iter()
                    .filter_map(|id| {
                        if id != merge.dropped {
                            Some(id)
                        } else if holds_kept {
                            None
                        } else {
                            Some(merge.kept.clone())
                        }
                    })
                    .collect();
                orders.push((playlist.clone(), order));
            }
            let mut tags = ids(connection, TAGS_OF, &merge.kept)?;
            let before = tags.len();
            for tag in ids(connection, TAGS_OF, &merge.dropped)? {
                if !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
            let rating = |content: &str| -> Result<i64, DbError> {
                count(
                    connection,
                    "SELECT COALESCE(Rating, 0) FROM djmdContent WHERE ID = ?1",
                    content,
                )
            };
            let (kept_rating, dropped_rating) = (rating(&merge.kept)?, rating(&merge.dropped)?);
            for (playlist, order) in &orders {
                writer.set_tracks(playlist, order)?;
                places.push((playlist.clone(), merge.kept.clone()));
            }
            if tags.len() != before {
                writer.set_my_tags(&merge.kept, &tags)?;
            }
            if kept_rating == 0 && dropped_rating > 0 {
                writer.set_rating(
                    &merge.kept,
                    u8::try_from(dropped_rating.min(5)).unwrap_or(5),
                )?;
            }
            writer.delete_tracks(std::slice::from_ref(&merge.dropped))?;
        }
        drop(writer);
        let library = Library::open(location.clone(), OpenMode::ReadOnly)?;
        let connection = library.connection();
        for merge in merges {
            let live = "SELECT COUNT(*) FROM djmdContent WHERE ID = ?1 AND rb_local_deleted = 0";
            if count(connection, live, &merge.dropped)? != 0 {
                return Err(DbError::WriteRefused(format!(
                    "After writing, track {} is still in rekordbox.",
                    merge.dropped
                )));
            }
        }
        for (playlist, kept) in &places {
            if !ids(connection, MEMBERS_OF, playlist)?.contains(kept) {
                return Err(DbError::WriteRefused(format!(
                    "After writing, playlist {playlist} does not hold track {kept}."
                )));
            }
        }
        Ok(())
    })();
    if let Err(error) = written {
        if let Err(undo) = super::restore(&backup.dir, location, backups, false, now_ms + 1) {
            return Err(DbError::WriteRefused(format!(
                "{error} Putting rekordbox back from the backup also failed ({undo}); restore it \
                 from Settings."
            )));
        }
        return Err(error);
    }
    Ok(backup)
}

/// A path as rekordbox stores it: with forward slashes on Windows, as rekordbox writes its own.
fn stored_form(path: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(path.to_string_lossy().replace('\\', "/"))
    } else {
        path.to_path_buf()
    }
}

/// Everything that can be checked without writing: the playlist exists and takes tracks, and
/// every known track is live. On Windows, files go in only when the library stores paths in the
/// form openCrate writes.
fn check_before(
    location: &LibraryLocation,
    playlist: Option<&Playlist>,
    members: &[Member],
) -> Result<(), DbError> {
    let library = Library::open(location.clone(), OpenMode::ReadOnly)?;
    let connection = library.connection();
    if let Some(Playlist::Id(id)) = playlist {
        let attribute: Option<i64> = connection
            .query_row(
                "SELECT COALESCE(Attribute, 0) FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        match attribute {
            None => return Err(refused("That playlist is no longer in rekordbox.")),
            Some(ATTRIBUTE_FOLDER) => {
                return Err(refused("That is a folder; drop tracks on a playlist."))
            }
            Some(ATTRIBUTE_SMART) => {
                return Err(refused(
                    "That is an intelligent playlist; rekordbox fills it by its rules.",
                ))
            }
            Some(_) => {}
        }
    }
    for member in members {
        if let Member::Known(id) = member {
            let live: bool = connection.query_row(
                "SELECT EXISTS (SELECT 1 FROM djmdContent WHERE ID = ?1 AND rb_local_deleted = 0)",
                params![id],
                |row| row.get(0),
            )?;
            if !live {
                return Err(refused(&format!(
                    "Track {id} is no longer in rekordbox. Read rekordbox again first."
                )));
            }
        }
    }
    if cfg!(windows)
        && members
            .iter()
            .any(|member| matches!(member, Member::File(_)))
    {
        let mut statement = connection.prepare(
            "SELECT FolderPath FROM djmdContent
             WHERE rb_local_deleted = 0 AND FolderPath IS NOT NULL LIMIT 200",
        )?;
        let stored = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        check_path_form(stored)?;
    }
    Ok(())
}

/// openCrate writes paths with forward slashes, the form rekordbox uses for its own paths on
/// Windows. A library that stores them with backslashes is a form not yet seen, so files are
/// not added to it rather than added in a form it may not find.
pub fn check_path_form<S: AsRef<str>>(stored: impl IntoIterator<Item = S>) -> Result<(), DbError> {
    if stored.into_iter().any(|path| path.as_ref().contains('\\')) {
        return Err(refused(
            "This rekordbox library stores file paths with backslashes, a form openCrate has not \
             been tested with, so it will not add files to it.",
        ));
    }
    Ok(())
}

/// Read the playlist back: every track meant to be in it is.
fn verify(location: &LibraryLocation, playlist: &str, ids: &[String]) -> Result<(), DbError> {
    let library = Library::open(location.clone(), OpenMode::ReadOnly)?;
    let mut statement = library.connection().prepare(
        "SELECT ContentID FROM djmdSongPlaylist WHERE PlaylistID = ?1 AND rb_local_deleted = 0",
    )?;
    let present: std::collections::HashSet<String> = statement
        .query_map(params![playlist], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    match ids.iter().find(|id| !present.contains(*id)) {
        Some(missing) => Err(DbError::WriteRefused(format!(
            "After writing, track {missing} is not in the playlist. Restore the backup taken \
             just before from Settings."
        ))),
        None => Ok(()),
    }
}

fn refused(reason: &str) -> DbError {
    DbError::WriteRefused(reason.to_owned())
}
