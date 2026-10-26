//! A dry run of importing one release: where each file would go in the library, what it would
//! become, which tag values would be written, and what would stop it. Nothing is copied, moved or
//! written; tags are only read. The import runs the same plan again and follows it. The page names a
//! watch folder, a release inside it and a library folder by id and name, never by path, and
//! the name is checked to be one plain entry of that folder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use oc_convert::{ConvertOptions, Format};
use oc_core_db::service::LibraryService;
use oc_ingest::{classify, list_folder, EntryKind};
use oc_organize::{
    destination, folder_artist, full_title, parse_title, Fields, PathBudget, Template,
    DEFAULT_TEMPLATE,
};
use oc_paths::{bytes_to_os_string, Availability};
use oc_tag::{guess_from_path, read_file, TagInfo};
use serde::{Deserialize, Serialize};

use crate::tagview::{tag_view, TagViewDto};

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanFileDto {
    /// Where the file is now, relative to the watch folder.
    pub source: String,
    /// Where it would go, relative to the library folder.
    pub destination: Option<String>,
    /// The file name had to be shortened to fit Windows' path limit.
    pub shortened: bool,
    /// Why it cannot go there yet, in plain words.
    pub problem: Option<String>,
    /// What the file's tags say, for checking before import. Audio files only.
    pub tags: Option<TagViewDto>,
    /// The format it will be converted to, such as `AIFF`; `None` when it goes in as it is.
    pub becomes: Option<&'static str>,
    /// What the import will do, when the file can go in. Never sent to the page.
    #[serde(skip)]
    pub job: Option<Job>,
    /// The tags as planned and the length in milliseconds, for a lookup. Never sent.
    #[serde(skip)]
    pub facts: Option<(TagInfo, i64)>,
}

/// One file's import, decided by the plan.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub source: PathBuf,
    pub destination: PathBuf,
    /// Convert to this format; the source's bit depth picks 16 or 24 bits.
    pub convert: Option<(Format, Option<u8>)>,
    /// The tag fields to write: everything for a converted file, otherwise only the values the
    /// file lacks or that were tidied.
    pub write: TagInfo,
}

/// What the owner chose on the Import screen.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    #[serde(default)]
    pub convert: ConvertOptions,
    /// Strip stray separators and padding from names before filing.
    #[serde(default)]
    pub tidy: bool,
    /// Values the owner accepted from a lookup, per file.
    #[serde(default)]
    pub accepted: Vec<Accepted>,
}

/// Tag values accepted for one file, named by its path relative to the watch folder.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Accepted {
    pub source: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
}

impl Accepted {
    fn tags(&self) -> TagInfo {
        TagInfo {
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            album_artist: self.album_artist.clone(),
            label: self.label.clone(),
            genre: self.genre.clone(),
            year: self.year,
            track_number: self.track,
            disc_number: self.disc,
            ..TagInfo::default()
        }
    }
}

/// The setting that holds the folder layout. Absent means [`DEFAULT_TEMPLATE`].
pub const LAYOUT_KEY: &str = "folder_layout";

/// The fields of `merged` that differ from what the file holds.
fn differences(merged: &TagInfo, tagged: &TagInfo) -> TagInfo {
    macro_rules! differ {
        ($($field:ident),+) => {
            TagInfo { $($field: if merged.$field == tagged.$field { None } else { merged.$field.clone() },)+ }
        };
    }
    differ!(
        title,
        artist,
        album,
        album_artist,
        label,
        genre,
        key,
        bpm,
        track_number,
        disc_number,
        year
    )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDto {
    pub release: String,
    pub root_label: String,
    pub files: Vec<PlanFileDto>,
    pub ready: usize,
    pub blocked: usize,
    #[serde(skip)]
    pub root_path: PathBuf,
}

/// The calendar year now, for judging whether a tagged year is plausible.
fn this_year() -> i32 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    i32::try_from(seconds / 31_556_952).unwrap_or(0) + 1970
}

/// Two files that would land in the same place cannot both go there.
fn flag_clashes(files: &mut [PlanFileDto]) {
    let places: Vec<Option<String>> = files
        .iter()
        .map(|file| file.destination.as_ref().map(|place| place.to_lowercase()))
        .collect();
    for (index, file) in files.iter_mut().enumerate() {
        let clash = places[index].is_some()
            && places
                .iter()
                .enumerate()
                .any(|(other, place)| other != index && *place == places[index]);
        if clash && file.problem.is_none() {
            file.problem =
                Some("Another file in this release would go to the same place.".to_owned());
            file.job = None;
        }
    }
}

fn single_entry(name: &str) -> Result<(), String> {
    // Exactly one plain name: on Windows "D:foo" is a drive-relative path that a join would
    // follow out of the watch folder.
    let mut parts = Path::new(name).components();
    let plain = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0', ':'])
        && matches!(
            (parts.next(), parts.next()),
            (Some(std::path::Component::Normal(_)), None)
        );
    if plain {
        Ok(())
    } else {
        Err("That is not a release in the watch folder.".to_owned())
    }
}

fn join(components: &[Vec<u8>]) -> String {
    components
        .iter()
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn to_path(base: &Path, components: &[Vec<u8>]) -> Option<PathBuf> {
    let mut path = base.to_path_buf();
    for part in components {
        let name: OsString = bytes_to_os_string(part).ok()?;
        path.push(name);
    }
    Some(path)
}

/// Spectrum edges already measured, by path, size and modified time, so the preview and the
/// import that follows do not decode a file twice.
type EdgeKey = (PathBuf, u64, std::time::SystemTime);
static EDGES: std::sync::Mutex<Option<std::collections::HashMap<EdgeKey, Option<u32>>>> =
    std::sync::Mutex::new(None);

/// The fake-lossless verdict for a lossless file, or `None` when it looks genuine or cannot be
/// checked. Decodes 30 seconds once per file version.
fn lossless_doubt(ffmpeg: &Path, path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let key = (
        path.to_path_buf(),
        metadata.len(),
        metadata.modified().ok()?,
    );
    let known = EDGES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .and_then(|edges| edges.get(&key).copied());
    let edge = known.unwrap_or_else(|| {
        let edge = oc_convert::spectrum::check(ffmpeg, path).ok().flatten();
        let mut edges = EDGES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let edges = edges.get_or_insert_with(Default::default);
        if edges.len() > 20_000 {
            edges.clear();
        }
        edges.insert(key, edge);
        edge
    });
    oc_convert::spectrum::verdict(edge?)
}

/// What does not change from file to file in one plan.
struct Context {
    template: Template,
    root_path: PathBuf,
    root_text: String,
    options: ImportOptions,
    ffmpeg_found: bool,
    /// For the fake-lossless check; `None` skips it.
    ffmpeg: Option<PathBuf>,
}

/// Plan one audio file: read it, decide its format, tags and place.
fn plan_audio(
    context: &Context,
    components: &[Vec<u8>],
    path: &Path,
    source: String,
) -> PlanFileDto {
    let problem = |text: &str| PlanFileDto {
        source: source.clone(),
        destination: None,
        shortened: false,
        problem: Some(text.to_owned()),
        tags: None,
        becomes: None,
        job: None,
        facts: None,
    };
    let parts: Vec<&[u8]> = components.iter().map(Vec::as_slice).collect();
    let read = read_file(path);
    let claims_lossless = read
        .as_ref()
        .is_ok_and(|info| oc_convert::is_lossless(info.container, info.audio.bit_depth));
    let (tagged, convert) = match &read {
        Ok(info) => (
            info.tags.clone(),
            oc_convert::target(
                context.options.convert,
                info.container,
                info.audio.bit_depth,
            )
            .map(|format| (format, info.audio.bit_depth)),
        ),
        Err(_) => (TagInfo::default(), None),
    };
    let duration_ms = read.as_ref().map_or(0, |info| info.audio.duration_ms);
    let accepted = context
        .options
        .accepted
        .iter()
        .find(|item| item.source == source)
        .map(Accepted::tags);
    let (tags, view) = tag_view(
        read,
        &guess_from_path(&parts),
        this_year(),
        context.options.tidy,
        accepted.as_ref(),
    );
    let mut view = view;
    if claims_lossless {
        if let Some(doubt) = context
            .ffmpeg
            .as_deref()
            .and_then(|ffmpeg| lossless_doubt(ffmpeg, path))
        {
            view.warnings.push(doubt);
        }
    }
    let becomes = convert.map(|(format, _)| format.container());
    let facts = Some((tags.clone(), duration_ms));
    let blocked = |text: &str| PlanFileDto {
        tags: Some(view.clone()),
        becomes,
        facts: facts.clone(),
        ..problem(text)
    };
    if convert.is_some() && !context.ffmpeg_found {
        return blocked("Converting needs ffmpeg, which was not found on this computer.");
    }
    let (Some(artist), Some(title)) = (tags.artist.clone(), tags.title.clone()) else {
        return blocked("No artist or title in the tags or file name.");
    };
    let parsed = parse_title(&title);
    let fields = Fields {
        folder_artist: Some(folder_artist(
            &parsed,
            tags.album_artist.as_deref(),
            &artist,
        )),
        release: tags.album.clone(),
        artist: Some(artist),
        album_artist: tags.album_artist.clone(),
        title: Some(full_title(&parsed)),
        track: tags.track_number,
        disc: tags.disc_number,
        year: tags.year.and_then(|year| u32::try_from(year).ok()),
        ..Fields::default()
    };
    let extension = match convert {
        Some((format, _)) => format.extension().to_owned(),
        None => path
            .extension()
            .map(|ext| ext.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    match destination(
        &context.template,
        &fields,
        &extension,
        &context.root_text,
        &PathBudget::classic(),
    ) {
        Ok(made) => {
            let relative: PathBuf = made.components.iter().collect();
            let target = context.root_path.join(&relative);
            let exists = target.exists();
            PlanFileDto {
                source,
                destination: Some(made.components.join("/")),
                shortened: made.stem_shortened,
                problem: exists.then(|| "A file is already there.".to_owned()),
                tags: Some(view.clone()),
                becomes,
                job: (!exists).then(|| Job {
                    source: path.to_path_buf(),
                    destination: target,
                    convert,
                    write: if convert.is_some() {
                        tags.clone()
                    } else {
                        differences(&tags, &tagged)
                    },
                }),
                facts,
            }
        }
        Err(error) => blocked(&match error {
            oc_organize::DestinationError::Template(oc_organize::TemplateError::Missing(field)) => {
                format!("No {} in the tags or file name.", field.replace('_', " "))
            }
            other => other.to_string(),
        }),
    }
}

pub fn plan_release(
    service: &LibraryService,
    folder_id: i64,
    name: &str,
    root_id: i64,
    options: ImportOptions,
) -> Result<PlanDto, String> {
    single_entry(name)?;
    let folder = service
        .watch_folders()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|folder| folder.id == folder_id)
        .ok_or("That watch folder is not in the list.")?;
    let root = service
        .roots()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|info| info.root.id == root_id)
        .ok_or("That music folder is not in the library.")?
        .root;
    if root.availability != Availability::Available {
        return Err(format!("{} cannot be reached right now.", root.label));
    }
    let folder_path = bytes_to_os_string(&folder.path_raw)
        .map(PathBuf::from)
        .map_err(|_| "The watch folder path cannot be shown here.".to_owned())?;
    let root_path = bytes_to_os_string(&root.path_raw)
        .map(PathBuf::from)
        .map_err(|_| "The music folder path cannot be shown here.".to_owned())?;

    let release_path = folder_path.join(name);
    let kind = std::fs::symlink_metadata(&release_path)
        .map_err(|_| "That release is no longer in the watch folder.".to_owned())?;
    if kind.file_type().is_symlink() {
        return Err("Links are not followed, so this cannot be previewed.".to_owned());
    }
    // Components below the watch folder, one list per file.
    let mut entries: Vec<Vec<Vec<u8>>> = Vec::new();
    if kind.is_dir() {
        let listing = list_folder(&release_path).map_err(|error| error.to_string())?;
        for entry in listing.entries {
            let mut components = vec![name.as_bytes().to_vec()];
            components.extend(entry.components);
            entries.push(components);
        }
    } else {
        entries.push(vec![name.as_bytes().to_vec()]);
    }
    entries.sort();

    let layout = service
        .setting(LAYOUT_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| DEFAULT_TEMPLATE.to_owned());
    let context = Context {
        template: Template::parse(&layout).map_err(|error| error.to_string())?,
        root_text: root_path.to_string_lossy().into_owned(),
        root_path: root_path.clone(),
        options,
        ffmpeg_found: oc_convert::find_ffmpeg().is_some(),
        ffmpeg: oc_convert::find_ffmpeg(),
    };
    let mut files = Vec::new();
    for components in &entries {
        let Some(last) = components.last() else {
            continue;
        };
        let source = join(components);
        let problem = |text: &str| PlanFileDto {
            source: source.clone(),
            destination: None,
            shortened: false,
            problem: Some(text.to_owned()),
            tags: None,
            becomes: None,
            job: None,
            facts: None,
        };
        match classify(last) {
            EntryKind::Audio(_) => {}
            EntryKind::Archive => {
                files.push(problem("Archives are not unpacked yet."));
                continue;
            }
            EntryKind::Partial => {
                files.push(problem("Still downloading."));
                continue;
            }
            _ => continue,
        }
        let Some(path) = to_path(&folder_path, components) else {
            files.push(problem("The file name cannot be shown here."));
            continue;
        };
        files.push(plan_audio(&context, components, &path, source));
    }
    flag_clashes(&mut files);
    let blocked = files.iter().filter(|file| file.problem.is_some()).count();
    Ok(PlanDto {
        release: name.to_owned(),
        root_label: root.label,
        ready: files.len() - blocked,
        blocked,
        files,
        root_path,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oc_tag::fixtures::{aiff, wav, write_tags};
    use oc_tag::TagInfo;

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

    fn tagged(path: &Path, bytes: Vec<u8>, tags: &TagInfo) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        write_tags(path, tags);
    }

    fn tags(title: &str, track: Option<u32>) -> TagInfo {
        TagInfo {
            title: Some(title.into()),
            artist: Some("Studio 001".into()),
            album: Some("Between Spaces".into()),
            track_number: track,
            ..TagInfo::default()
        }
    }

    #[test]
    fn a_release_is_laid_out_with_the_default_template_and_nothing_is_written() {
        let s = setup();
        let release = s.area.path().join("Downloads/Between Spaces");
        tagged(
            &release.join("a.aif"),
            aiff(1),
            &tags("Night Drive", Some(1)),
        );
        tagged(
            &release.join("b.wav"),
            wav(1),
            &tags("Soft Focus (Dub Mix)", Some(2)),
        );
        fs::write(release.join("cover.jpg"), b"x").unwrap();
        let before = fs::read_dir(s.area.path().join("Music")).unwrap().count();

        let plan = plan_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            ImportOptions::default(),
        )
        .unwrap();
        assert_eq!((plan.ready, plan.blocked), (2, 0), "{plan:?}");
        let places: Vec<_> = plan
            .files
            .iter()
            .map(|file| file.destination.clone().unwrap())
            .collect();
        assert_eq!(
            places,
            [
                "Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif",
                "Studio 001/Between Spaces/02 - Studio 001 - Soft Focus (Dub Mix).wav"
            ]
        );
        assert_eq!(
            fs::read_dir(s.area.path().join("Music")).unwrap().count(),
            before,
            "the library is untouched"
        );
        assert!(release.join("a.aif").exists());
        let view = plan.files[0].tags.as_ref().unwrap();
        let value = |name: &str| {
            view.fields
                .iter()
                .find(|field| field.name == name)
                .and_then(|field| field.value.clone())
        };
        assert_eq!(value("Artist").as_deref(), Some("Studio 001"));
        assert_eq!(value("Track").as_deref(), Some("1"));
        assert!(view.audio.as_deref().unwrap().starts_with("AIFF"));
        assert!(view.length.is_some());
        assert_eq!(view.warnings.len(), 0, "{:?}", view.warnings);
    }

    #[test]
    fn what_stops_a_file_is_said_in_plain_words() {
        let s = setup();
        let release = s.area.path().join("Downloads/Mixed");
        tagged(&release.join("noart.aif"), aiff(1), &TagInfo::default());
        tagged(
            &release.join("same1.aif"),
            aiff(1),
            &tags("Same Name", Some(4)),
        );
        tagged(
            &release.join("same2.aif"),
            aiff(1),
            &tags("Same Name", Some(4)),
        );
        fs::write(release.join("pending.flac.part"), b"x").unwrap();
        fs::write(release.join("Extras.zip"), b"x").unwrap();
        tagged(
            &s.area
                .path()
                .join("Music/Studio 001/Between Spaces/09 - Studio 001 - Taken.aif"),
            aiff(1),
            &tags("Taken", Some(9)),
        );
        tagged(&release.join("taken.aif"), aiff(1), &tags("Taken", Some(9)));

        let plan = plan_release(
            &s.service,
            s.folder,
            "Mixed",
            s.root,
            ImportOptions::default(),
        )
        .unwrap();
        let problem = |source: &str| {
            plan.files
                .iter()
                .find(|file| file.source.ends_with(source))
                .and_then(|file| file.problem.clone())
        };
        assert_eq!(
            problem("pending.flac.part").as_deref(),
            Some("Still downloading.")
        );
        assert_eq!(
            problem("Extras.zip").as_deref(),
            Some("Archives are not unpacked yet.")
        );
        assert_eq!(
            problem("taken.aif").as_deref(),
            Some("A file is already there.")
        );
        assert!(problem("same1.aif").unwrap().contains("same place"));
        assert!(problem("same2.aif").unwrap().contains("same place"));
        assert!(problem("noart.aif").unwrap().starts_with("No "));
        let tags_of = |source: &str| {
            plan.files
                .iter()
                .find(|file| file.source.ends_with(source))
                .map(|file| file.tags.is_some())
        };
        assert_eq!(
            tags_of("noart.aif"),
            Some(true),
            "a blocked file still shows its tags"
        );
        assert_eq!(tags_of("same1.aif"), Some(true));
        assert_eq!(tags_of("Extras.zip"), Some(false));
        assert_eq!(tags_of("pending.flac.part"), Some(false));
    }

    #[test]
    fn only_a_plain_release_name_inside_a_listed_folder_is_accepted() {
        let s = setup();
        for bad in ["", ".", "..", "../Music", "a/b", "a\\b", "/etc"] {
            assert!(
                plan_release(&s.service, s.folder, bad, s.root, ImportOptions::default()).is_err(),
                "{bad:?}"
            );
        }
        assert!(plan_release(&s.service, 999, "x", s.root, ImportOptions::default()).is_err());
        assert!(plan_release(
            &s.service,
            s.folder,
            "missing",
            s.root,
            ImportOptions::default()
        )
        .is_err());
        assert!(plan_release(&s.service, s.folder, "x", 999, ImportOptions::default()).is_err());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(), s.area.path().join("Downloads/link"))
                .unwrap();
            assert!(plan_release(
                &s.service,
                s.folder,
                "link",
                s.root,
                ImportOptions::default()
            )
            .unwrap_err()
            .contains("Links"));
        }
    }

    #[test]
    fn a_single_loose_file_is_planned_too() {
        let s = setup();
        tagged(
            &s.area.path().join("Downloads/loose.aif"),
            aiff(1),
            &tags("Night Drive", Some(3)),
        );
        let plan = plan_release(
            &s.service,
            s.folder,
            "loose.aif",
            s.root,
            ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(plan.ready, 1);
        assert_eq!(
            plan.files[0].destination.as_deref(),
            Some("Studio 001/Between Spaces/03 - Studio 001 - Night Drive.aif")
        );
    }

    #[test]
    fn a_flac_made_from_an_mp3_is_flagged_and_a_real_one_is_not() {
        let Some(ffmpeg) = oc_convert::find_ffmpeg() else {
            eprintln!("ffmpeg not found; the fake-lossless check is not exercised");
            return;
        };
        let s = setup();
        let release = s.area.path().join("Downloads/Between Spaces");
        fs::create_dir_all(&release).unwrap();
        let run = |args: &[&str]| {
            let ok = std::process::Command::new(&ffmpeg)
                .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
                .args(args)
                .current_dir(&release)
                .status()
                .unwrap()
                .success();
            assert!(ok, "{args:?}");
        };
        let tags = |title: &str, track: &str| {
            [
                "-metadata".to_owned(),
                "artist=Studio 001".to_owned(),
                "-metadata".to_owned(),
                "album=Between Spaces".to_owned(),
                "-metadata".to_owned(),
                format!("title={title}"),
                "-metadata".to_owned(),
                format!("track={track}"),
            ]
        };
        let noise = [
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=d=12:c=pink:r=44100:a=0.3",
            "-ac",
            "2",
        ];
        let mut real: Vec<String> = noise.iter().map(|a| (*a).to_owned()).collect();
        real.extend(tags("Night Drive", "1"));
        real.push("01.flac".into());
        run(&real.iter().map(String::as_str).collect::<Vec<_>>());
        run(&[
            "-i",
            "01.flac",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "128k",
            "lossy.mp3",
        ]);
        let mut fake = vec!["-i".to_owned(), "lossy.mp3".to_owned()];
        fake.extend(tags("Soft Focus", "2"));
        fake.push("02.flac".into());
        run(&fake.iter().map(String::as_str).collect::<Vec<_>>());
        fs::remove_file(release.join("lossy.mp3")).unwrap();

        let plan = plan_release(
            &s.service,
            s.folder,
            "Between Spaces",
            s.root,
            ImportOptions::default(),
        )
        .unwrap();
        let warnings = |name: &str| {
            plan.files
                .iter()
                .find(|file| file.source.ends_with(name))
                .and_then(|file| file.tags.as_ref())
                .map(|view| view.warnings.clone())
                .unwrap()
        };
        assert!(
            warnings("02.flac")
                .iter()
                .any(|w| w.starts_with("Probably made from a lossy file")),
            "{:?}",
            warnings("02.flac")
        );
        assert!(
            !warnings("01.flac").iter().any(|w| w.contains("lossy")),
            "{:?}",
            warnings("01.flac")
        );
        assert_eq!(plan.ready, 2, "a doubt never blocks the import");
    }
}
