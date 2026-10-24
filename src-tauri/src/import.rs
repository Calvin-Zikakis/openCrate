//! Importing one release: every file the preview says is ready is copied (or converted) into a
//! staging folder beside the music folder, its tags and cover are written into that copy, and it
//! is published into its place with `oc_publish`, which never replaces a file that is already
//! there. The originals in the download folder are only read.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use oc_core_db::service::LibraryService;
use oc_publish::{
    link_no_clobber, stage_copy, sweep_partials, NoFaults, Outcome, Rejection, PART_SUFFIX,
};
use oc_tag::{embedded_cover, folder_cover, write_tags, TagInfo};
use serde::Serialize;

use crate::plan::{plan_release, ImportOptions, Job};

/// Beside the music folder, never inside it, so a sync tool watching the library never sees
/// a file being copied.
pub const STAGING_NAME: &str = ".opencrate-staging";

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedDto {
    pub source: String,
    pub problem: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDto {
    pub root_id: i64,
    pub root_label: String,
    /// Files now in the library.
    pub filed: usize,
    /// Files left where they are, and why.
    pub skipped: Vec<SkippedDto>,
}

/// The staging folder for a music folder: its sibling, which must be on the same volume so the
/// last step can be a hard link.
fn staging_for(root: &Path, label: &str) -> Result<PathBuf, String> {
    let refused = || {
        format!(
            "openCrate prepares files in a folder next to {label} and cannot make one there, \
             so nothing was imported."
        )
    };
    let parent = root.parent().ok_or_else(refused)?;
    let staging = parent.join(STAGING_NAME);
    std::fs::create_dir_all(&staging).map_err(|_| refused())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let device = |path: &Path| std::fs::metadata(path).map(|metadata| metadata.dev());
        if device(&staging).ok() != device(root).ok() {
            return Err(format!(
                "{label} is on its own drive or share, and openCrate needs a folder beside it on \
                 the same drive to prepare files. Nothing was imported."
            ));
        }
    }
    // Copies a crash cut short; nothing else in this folder has that suffix.
    sweep_partials(&staging).map_err(|error| error.to_string())?;
    Ok(staging)
}

fn staged_name(staging: &Path, destination: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis());
    let mut name = format!("{millis}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    if let Some(extension) = destination.extension() {
        name.push('.');
        name.push_str(&extension.to_string_lossy());
    }
    staging.join(name)
}

fn rejection_text(reason: Rejection) -> &'static str {
    match reason {
        Rejection::DestinationOccupied => {
            "A file appeared at its place while importing; it was left alone."
        }
        Rejection::SourceChanged => "The copy changed before it could be filed.",
        Rejection::DestinationChanged => {
            "The filed copy changed straight away; a copy is kept in the staging folder."
        }
    }
}

/// Put a converted copy at `staged`: ffmpeg writes a `.part` file, which takes the staged name
/// only once it is complete.
fn stage_converted(job: &Job, staged: &Path) -> Result<(), String> {
    let Some((format, bit_depth)) = job.convert else {
        return Ok(());
    };
    let ffmpeg = oc_convert::find_ffmpeg().ok_or_else(|| oc_convert::Error::Missing.to_string())?;
    let mut part = staged.as_os_str().to_owned();
    part.push(PART_SUFFIX);
    let part = PathBuf::from(part);
    let _ = std::fs::remove_file(&part);
    let converted = oc_convert::convert(&ffmpeg, &job.source, &part, format, bit_depth)
        .map_err(|error| error.to_string())
        .and_then(|()| link_no_clobber(&part, staged).map_err(|error| error.to_string()));
    let _ = std::fs::remove_file(&part);
    converted
}

/// The cover to embed: a converted file starts with none, so it gets the source's own picture
/// or the release's image; a copied file only gets the release's image if it has no picture.
fn cover_for(job: &Job) -> Option<oc_tag::Cover> {
    let beside = || job.source.parent().and_then(folder_cover);
    if job.convert.is_some() {
        embedded_cover(&job.source).or_else(beside)
    } else if embedded_cover(&job.source).is_none() {
        beside()
    } else {
        None
    }
}

/// Stage one file, write its tags, and publish it. `Ok(None)` when it is in the library.
fn file_one(service: &LibraryService, staging: &Path, job: &Job) -> Result<Option<String>, String> {
    if let Some(folder) = job.destination.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let staged = staged_name(staging, &job.destination);
    if job.convert.is_some() {
        stage_converted(job, &staged)?;
    } else {
        stage_copy(&job.source, &staged, &mut NoFaults).map_err(|error| error.to_string())?;
    }
    let cover = cover_for(job);
    if job.write != TagInfo::default() || cover.is_some() {
        if let Err(error) = write_tags(&staged, &job.write, cover.as_ref()) {
            let _ = std::fs::remove_file(&staged);
            return Ok(Some(format!("The tags could not be written: {error}")));
        }
    }
    match service.publish_file(staged.clone(), job.destination.clone()) {
        Ok(Outcome::Published) => Ok(None),
        Ok(Outcome::Rejected(reason)) => {
            if reason == Rejection::DestinationOccupied {
                // The original is still in the download folder, so this copy is not needed.
                let _ = std::fs::remove_file(&staged);
            }
            Ok(Some(rejection_text(reason).to_owned()))
        }
        Err(error) => {
            // Never leave a full copy behind for a file that did not go in.
            if !job.destination.exists() {
                let _ = std::fs::remove_file(&staged);
            }
            Err(error.to_string())
        }
    }
}

pub fn import_release(
    service: &LibraryService,
    folder_id: i64,
    name: &str,
    root_id: i64,
    options: ImportOptions,
) -> Result<ImportDto, String> {
    // Planned again now: the folder may have changed since the preview was shown.
    let plan = plan_release(service, folder_id, name, root_id, options)?;
    if plan.ready == 0 {
        return Err("Nothing in this release is ready to import.".to_owned());
    }
    let staging = staging_for(&plan.root_path, &plan.root_label)?;
    let mut filed = 0;
    let mut skipped = Vec::new();
    for file in plan.files {
        let Some(job) = &file.job else {
            skipped.push(SkippedDto {
                source: file.source,
                problem: file.problem.unwrap_or_default(),
            });
            continue;
        };
        match file_one(service, &staging, job) {
            Ok(None) => filed += 1,
            Ok(Some(problem)) | Err(problem) => skipped.push(SkippedDto {
                source: file.source,
                problem,
            }),
        }
    }
    if filed > 0 {
        service
            .mark_imported(folder_id, name.as_bytes().to_vec(), filed)
            .map_err(|error| error.to_string())?;
    }
    Ok(ImportDto {
        root_id,
        root_label: plan.root_label,
        filed,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oc_tag::fixtures::{aiff, mp3, wav, write_tags as fixture_tags};
    use oc_tag::read_file;

    use super::*;

    struct Setup {
        _data: tempfile::TempDir,
        service: LibraryService,
        area: tempfile::TempDir,
        folder: i64,
        root: i64,
    }

    fn setup() -> Setup {
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let area = tempfile::tempdir().unwrap();
        fs::create_dir_all(area.path().join("Downloads")).unwrap();
        fs::create_dir_all(area.path().join("Music")).unwrap();
        let folder = service
            .add_watch_folder(&area.path().join("Downloads"), "Downloads")
            .unwrap();
        let root = service
            .add_root(&area.path().join("Music"), "Music")
            .unwrap();
        Setup {
            _data: data,
            service,
            area,
            folder,
            root,
        }
    }

    fn tagged(path: &Path, title: &str, track: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, aiff(1)).unwrap();
        fixture_tags(
            path,
            &TagInfo {
                title: Some(title.into()),
                artist: Some("Studio 001".into()),
                album: Some("Between Spaces".into()),
                track_number: Some(track),
                ..TagInfo::default()
            },
        );
    }

    fn files_in(folder: &Path) -> Vec<String> {
        let mut found: Vec<String> = walk(folder)
            .iter()
            .map(|path| {
                // `/`-separated on every system, so one expectation holds everywhere.
                path.strip_prefix(folder)
                    .unwrap()
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect();
        found.sort();
        found
    }

    fn walk(folder: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for entry in fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path);
            }
        }
        found
    }

    #[test]
    fn ready_files_are_filed_byte_for_byte_and_the_originals_stay() {
        let s = setup();
        let release = s.area.path().join("Downloads/Between Spaces");
        tagged(&release.join("a.aif"), "Night Drive", 1);
        tagged(&release.join("b.aif"), "Soft Focus", 2);
        fs::write(release.join("Extras.zip"), b"x").unwrap();
        let before = fs::read(release.join("a.aif")).unwrap();

        let done = import_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(done.filed, 2);
        assert_eq!(
            done.skipped,
            [SkippedDto {
                source: "Between Spaces/Extras.zip".into(),
                problem: "Archives are not unpacked yet.".into()
            }]
        );
        let music = s.area.path().join("Music");
        assert_eq!(
            files_in(&music),
            [
                "Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif",
                "Studio 001/Between Spaces/02 - Studio 001 - Soft Focus.aif"
            ]
        );
        assert_eq!(
            fs::read(music.join("Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif"))
                .unwrap(),
            before
        );
        assert_eq!(
            files_in(&release),
            ["Extras.zip", "a.aif", "b.aif"],
            "originals untouched"
        );
        assert_eq!(fs::read(release.join("a.aif")).unwrap(), before);
        assert!(
            files_in(&s.area.path().join(STAGING_NAME)).is_empty(),
            "staging is left empty"
        );
        assert!(s
            .service
            .imported_releases(s.folder)
            .unwrap()
            .contains(b"Between Spaces".as_slice()));
    }

    #[test]
    fn an_existing_file_is_never_replaced_and_importing_twice_files_nothing_new() {
        let s = setup();
        let release = s.area.path().join("Downloads/Between Spaces");
        tagged(&release.join("a.aif"), "Night Drive", 1);
        let taken = s
            .area
            .path()
            .join("Music/Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif");
        fs::create_dir_all(taken.parent().unwrap()).unwrap();
        fs::write(&taken, b"already here").unwrap();
        tagged(&release.join("b.aif"), "Soft Focus", 2);

        let first = import_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(first.filed, 1);
        assert_eq!(first.skipped[0].problem, "A file is already there.");
        assert_eq!(fs::read(&taken).unwrap(), b"already here");
        let again = import_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            ImportOptions::default(),
        );
        assert_eq!(
            again.unwrap_err(),
            "Nothing in this release is ready to import."
        );
    }

    #[test]
    fn a_file_that_arrives_between_plan_and_publish_is_left_alone() {
        let s = setup();
        let source = s.area.path().join("Downloads/one.aif");
        tagged(&source, "Night Drive", 1);
        let staging = staging_for(&s.area.path().join("Music"), "Music").unwrap();
        let destination = s.area.path().join("Music/late.aif");
        fs::write(&destination, b"arrived first").unwrap();
        let job = Job {
            source,
            destination: destination.clone(),
            convert: None,
            write: TagInfo::default(),
        };
        let problem = file_one(&s.service, &staging, &job).unwrap();
        assert!(problem.unwrap().contains("left alone"));
        assert_eq!(fs::read(&destination).unwrap(), b"arrived first");
        assert!(
            files_in(&staging).is_empty(),
            "the unneeded copy is removed"
        );
    }

    #[test]
    fn a_leftover_partial_copy_is_swept_before_importing() {
        let s = setup();
        let staging = s.area.path().join(STAGING_NAME);
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("1-0.aif.part"), b"half").unwrap();
        staging_for(&s.area.path().join("Music"), "Music").unwrap();
        assert_eq!(files_in(&staging), Vec::<String>::new());
    }

    fn options(tidy: bool, lossless: Option<oc_convert::Format>) -> ImportOptions {
        ImportOptions {
            convert: oc_convert::ConvertOptions {
                lossless,
                lossy: None,
            },
            tidy,
            accepted: Vec::new(),
        }
    }

    #[test]
    fn tidied_names_and_values_from_the_file_name_are_written_into_the_copy() {
        let s = setup();
        let source = s
            .area
            .path()
            .join("Downloads/Between Spaces/03 - Night Drive.mp3");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, mp3(20)).unwrap();
        fixture_tags(
            &source,
            &TagInfo {
                artist: Some("Studio 001;".into()),
                title: Some("Night Drive".into()),
                album: Some("Between Spaces".into()),
                ..TagInfo::default()
            },
        );
        let before = fs::read(&source).unwrap();
        let done = import_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            options(true, None),
        )
        .unwrap();
        assert_eq!(
            (done.filed, done.skipped.len()),
            (1, 0),
            "{:?}",
            done.skipped
        );
        let filed = s
            .area
            .path()
            .join("Music/Studio 001/Between Spaces/03 - Studio 001 - Night Drive.mp3");
        let tags = read_file(&filed).unwrap().tags;
        assert_eq!(tags.artist.as_deref(), Some("Studio 001"), "tidied");
        assert_eq!(tags.track_number, Some(3), "from the file name");
        assert_eq!(&fs::read(&filed).unwrap()[..4], b"ID3\x03");
        assert_eq!(
            fs::read(&source).unwrap(),
            before,
            "the original is untouched"
        );
    }

    #[test]
    fn lossless_files_are_converted_and_tagged_and_others_kept() {
        if oc_convert::find_ffmpeg().is_none() {
            eprintln!("ffmpeg not found; conversion not exercised");
            return;
        }
        let s = setup();
        let release = s.area.path().join("Downloads/Between Spaces");
        let wav_file = release.join("a.wav");
        fs::create_dir_all(&release).unwrap();
        fs::write(&wav_file, wav(1)).unwrap();
        fixture_tags(
            &wav_file,
            &TagInfo {
                artist: Some("Studio 001".into()),
                title: Some("Night Drive".into()),
                album: Some("Between Spaces".into()),
                track_number: Some(1),
                year: Some(2019),
                ..TagInfo::default()
            },
        );
        tagged(&release.join("b.aif"), "Soft Focus", 2);
        let kept = fs::read(release.join("b.aif")).unwrap();
        let plan = plan_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            options(false, Some(oc_convert::Format::Aiff)),
        )
        .unwrap();
        let becomes: Vec<_> = plan.files.iter().map(|file| file.becomes).collect();
        assert_eq!(becomes, [Some("AIFF"), None], "only the WAV is converted");
        let done = import_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            options(false, Some(oc_convert::Format::Aiff)),
        )
        .unwrap();
        assert_eq!(done.filed, 2, "{:?}", done.skipped);
        let music = s.area.path().join("Music/Studio 001/Between Spaces");
        let converted = read_file(&music.join("01 - Studio 001 - Night Drive.aif")).unwrap();
        assert_eq!(converted.container, "AIFF");
        assert_eq!(converted.tags.title.as_deref(), Some("Night Drive"));
        assert_eq!(converted.tags.year, Some(2019));
        assert_eq!(
            fs::read(music.join("02 - Studio 001 - Soft Focus.aif")).unwrap(),
            kept
        );
        assert!(wav_file.exists(), "the original stays");
    }

    #[test]
    fn the_folder_layout_setting_decides_where_files_go() {
        let s = setup();
        s.service
            .set_setting(
                crate::plan::LAYOUT_KEY,
                Some("{release}/{artist} - {title}".into()),
            )
            .unwrap();
        tagged(
            &s.area.path().join("Downloads/Between Spaces/a.aif"),
            "Night Drive",
            1,
        );
        let plan = plan_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(
            plan.files[0].destination.as_deref(),
            Some("Between Spaces/Studio 001 - Night Drive.aif")
        );
    }

    #[test]
    fn accepted_lookup_values_decide_the_place_and_are_written() {
        let s = setup();
        let source = s.area.path().join("Downloads/Between Spaces/x.aif");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, aiff(1)).unwrap();
        fixture_tags(
            &source,
            &TagInfo {
                artist: Some("Studio 001".into()),
                title: Some("night drive".into()),
                album: Some("Between Spaces".into()),
                ..TagInfo::default()
            },
        );
        let mut chosen = options(false, None);
        chosen.accepted.push(crate::plan::Accepted {
            source: "Between Spaces/x.aif".into(),
            title: Some("Night Drive".into()),
            track: Some(4),
            label: Some("Test Label".into()),
            year: Some(2019),
            ..crate::plan::Accepted::default()
        });
        let done = import_release(&s.service, s.folder, "Between Spaces", s.root, chosen).unwrap();
        assert_eq!(done.filed, 1, "{:?}", done.skipped);
        let filed = s
            .area
            .path()
            .join("Music/Studio 001/Between Spaces/04 - Studio 001 - Night Drive.aif");
        let tags = read_file(&filed).unwrap().tags;
        assert_eq!(tags.title.as_deref(), Some("Night Drive"));
        assert_eq!((tags.track_number, tags.year), (Some(4), Some(2019)));
        assert_eq!(tags.label.as_deref(), Some("Test Label"));
    }
}
