//! Playback commands: the page names a track by id; the file is found in the library and only
//! read. One track plays at a time.

use oc_core_db::service::LibraryService;
use oc_player::{Player, Status};
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDto {
    pub track_id: Option<i64>,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    /// The track played to its end; the page moves on to the next one.
    pub ended: bool,
    pub error: Option<String>,
}

pub fn dto(status: Status) -> PlayerDto {
    PlayerDto {
        track_id: status.key.as_deref().and_then(|key| key.parse().ok()),
        playing: status.playing,
        position_ms: status.position_ms,
        duration_ms: status.duration_ms,
        ended: status.ended,
        error: status.error,
    }
}

/// Load a track's file and, when asked, start it from `start_ms`.
pub fn load(
    service: &LibraryService,
    player: &Player,
    track_id: i64,
    start_ms: u64,
    play: bool,
) -> Result<(), String> {
    let path = service
        .track_file(track_id)
        .map_err(|error| error.to_string())?
        .ok_or("This track's file cannot be found. Its music folder may be unplugged, or the file was moved.")?;
    player.load(path, track_id.to_string(), start_ms, play);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_names_the_track_by_id_and_a_missing_file_is_explained() {
        let status = Status {
            key: Some("42".into()),
            playing: true,
            position_ms: 1_500,
            duration_ms: Some(200_000),
            ended: false,
            error: None,
        };
        assert_eq!(dto(status).track_id, Some(42));
        assert_eq!(dto(Status::default()).track_id, None);
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let player = Player::start();
        let error = load(&service, &player, 7, 0, true).unwrap_err();
        assert!(error.contains("cannot be found"), "{error}");
        assert_eq!(player.status(), Status::default(), "nothing was loaded");
    }
}
