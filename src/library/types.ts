export type View = "tracks" | "releases" | "artists";
export type Sort = "recent" | "title" | "artist" | "bpm" | "playlist" | "crate" | "custom";
export type Format = "AIFF" | "MP3" | "FLAC";

export interface LibraryItem {
  id: number;
  kind: View;
  title: string;
  artist: string;
  artistId: number;
  release: string;
  releaseId: number;
  bpm: number | null;
  musicalKey: string;
  duration: number;
  format: Format | null;
  trackCount: number;
  /** A track none of whose files is on disk right now (not the same as an offline folder). */
  missing: boolean;
  /** rekordbox has analysed it: bpm and key are rekordbox's, not the file's tags'. */
  analysed: boolean;
  /** The key is rekordbox's, not the file's tags'. */
  rekordboxKey: boolean;
  color: number;
}

export interface LibraryQuery {
  view: View;
  search: string;
  sort: Sort;
  format: Format | "all";
  artistId: number | null;
  releaseId: number | null;
  /** Tracks of this rekordbox playlist, in the playlist's own order with sort "playlist". */
  playlistId: number | null;
  /** Tracks in one of the owner's crates (openCrate only, never rekordbox). */
  crateId: number | null;
}

export interface LibraryPage {
  items: LibraryItem[];
  total: number;
  start: number;
}

export interface FolderInfo {
  id: number;
  label: string;
  path: string;
  /** "available", "unavailable" or "unverified". */
  availability: string;
  reason: string;
  lastAvailableMs: number | null;
  files: number;
  missing: number;
}

export interface ScanSummary {
  availability: string;
  reason: string;
  inserted: number;
  updated: number;
  unchanged: number;
  revived: number;
  markedMissing: number;
  heldMissing: number;
  tracksCreated: number;
  tracksUpdated: number;
  tagErrors: number;
  cancelled: boolean;
}

export interface ScanStatus {
  state: "idle" | "running" | "finished" | "failed";
  rootId: number | null;
  phase: string | null;
  filesSeen: number;
  tagsRead: number;
  tagsTotal: number;
  summary: ScanSummary | null;
  error: string | null;
}

export interface PlaylistInfo {
  id: number;
  parentId: number | null;
  isFolder: boolean;
  name: string;
  position: number;
  /** Entries in the playlist, duplicates included. */
  entries: number;
  /** Distinct tracks of it that match a file in the library. */
  inLibrary: number;
}

export interface MirrorCounts {
  matched: number;
  /** Entries pointing outside every folder you added. */
  unmapped: number;
  /** Entries inside a folder, but with no file indexed at that path. */
  notIndexed: number;
  ambiguous: number;
  unrecognized: number;
  playlists: number;
  unresolvedReferences: number;
}

export interface MirrorInfo {
  sourceId: number;
  /** An XML export file, or a read of rekordbox's own library. */
  kind: "xml" | "master_db";
  path: string;
  generation: number;
  entries: number;
  fileMtimeMs: number | null;
  lastReadMs: number | null;
  lastAttemptMs: number | null;
  lastError: string | null;
  counts: MirrorCounts;
  /** The export file, or rekordbox's own library, changed after the last read. */
  changedSinceRead: boolean;
}

export interface ImportResult {
  sourceId: number;
  generation: number;
  unchanged: boolean;
  entries: number;
  matched: number;
  derivedSet: number;
}

/** Reading rekordbox's own library directly: off until the owner confirms the warning. */
export interface RekordboxDatabase {
  installed: boolean;
  appVersion: string | null;
  running: boolean;
  enabled: boolean;
  confirmedVersion: string | null;
  /** On, but rekordbox changed version since the warning was confirmed. */
  needsConfirmation: boolean;
  /** Writing to rekordbox's library is on; it needs reading on too. */
  writeEnabled: boolean;
  writeConfirmedVersion: string | null;
  writeNeedsConfirmation: boolean;
  /** This rekordbox version is one openCrate's writer is known to work with. */
  writeTested: boolean;
}

export type MergeField = "title" | "artist" | "album" | "year" | "label";
export type MergeDirection = "toRekordbox" | "toOpenCrate" | "conflict";

/** One field of one track that differs between openCrate and rekordbox. */
export interface MergeChange {
  rbKey: number;
  trackId: number;
  title: string;
  artist: string;
  field: MergeField;
  /** What both sides last agreed on; null before the first sync. */
  base: string | null;
  ours: string;
  theirs: string;
  direction: MergeDirection;
}

export interface MergeReview {
  changes: MergeChange[];
  /** Writing to rekordbox is on, so openCrate's values can go to rekordbox. */
  canWrite: boolean;
}

/** A ticked change: which side's value to keep, with the values the owner saw. */
export interface MergeChoice {
  rbKey: number;
  field: MergeField;
  keep: "openCrate" | "rekordbox";
  ours: string;
  theirs: string;
}

export interface MergeApplied {
  toRekordbox: number;
  toOpencrate: number;
  skipped: string[];
}

/** What a write to rekordbox's library did. */
/** A folder made in rekordbox's playlist tree. */
export interface RekordboxFolder {
  /** The name it was given; a taken one gets a number. */
  name: string;
  /** The write worked but reading rekordbox again afterwards did not. */
  readError: string | null;
}

export interface RekordboxWrite {
  playlist: string;
  added: number;
  /** Files that were new to rekordbox. */
  imported: number;
  /** Tracks that could not go in, with why. */
  leftOut: string[];
  /** The write worked but reading rekordbox again afterwards did not. */
  readError: string | null;
}

export interface RekordboxBackup {
  /** The backup's folder name; the only way the page names a backup. */
  name: string;
  takenMs: number;
  reason: string;
  pinned: boolean;
  appVersion: string | null;
  tracks: number;
  playlists: number;
}

/** A track openCrate filed that rekordbox does not have yet. */
export interface RekordboxCandidate {
  fileId: number;
  title: string;
  artist: string;
  album: string;
  durationMs: number;
  container: string;
  filedMs: number;
  /** When it was last written into openCrate's rekordbox XML file. */
  offeredMs: number | null;
  /** Kept out of rekordbox by the owner. */
  hidden: boolean;
}

export interface RekordboxNewTracks {
  tracks: RekordboxCandidate[];
  /** When rekordbox was last read; tracks filed after it may already be in rekordbox. */
  mirrorReadMs: number | null;
  newestFiledMs: number | null;
  xmlPath: string;
  playlist: string;
}

export interface RekordboxSent {
  xmlPath: string;
  tracks: number;
}

/** A crate as it can go into openCrate's rekordbox file. */
export interface RekordboxCrate {
  crateId: number;
  name: string;
  inFile: boolean;
  /** Tracks rekordbox has, written back as its own entries. */
  known: number;
  /** Tracks rekordbox does not have yet. */
  new: number;
  /** Tracks known only from a direct read of rekordbox; left out until an XML export is read. */
  needsExport: number;
  /** Tracks whose files cannot be reached; left out. */
  noFile: number;
}

export interface RekordboxCrates {
  crates: RekordboxCrate[];
  xmlPath: string;
  folder: string;
}

export interface AliasSuggestion {
  rootId: number;
  rootLabel: string;
  /** A rekordbox Location prefix such as /G:/Library. */
  prefix: string;
  entries: number;
}

/** A folder where finished downloads appear. It is only read, and is never part of the library. */
export interface WatchFolderInfo {
  id: number;
  label: string;
  path: string;
  enabled: boolean;
  online: boolean;
  lastSeenMs: number | null;
}

export type DownloadState = "ready" | "downloading" | "settling" | "nothing";

/** One release (or loose file) seen in a download folder. */
export interface DownloadRelease {
  folderId: number;
  folderLabel: string;
  name: string;
  loose: boolean;
  state: DownloadState;
  /** For `settling`: whole seconds the files have been unchanged so far. */
  quietSeconds: number | null;
  audioFiles: number;
  archives: number;
  /** It was already in the folder when the folder was added. */
  alreadyThere: boolean;
  /** Imported with openCrate. The originals are still in the download folder. */
  imported: boolean;
  newestModifiedMs: number;
}

export interface TagField {
  name: string;
  value: string | null;
  /** Not in the file's tags; taken from its folder and file names. */
  guessed: boolean;
  /** The value before stray separators and padding were tidied away. */
  tidiedFrom: string | null;
}

/** What a file's tags say, for checking before import. */
export interface TagView {
  fields: TagField[];
  /** `AIFF 16/44.1` when lossless, `MP3 320 kbps` when not. */
  audio: string | null;
  /** `m:ss`. */
  length: string | null;
  readError: string | null;
  /** Things that look wrong. They do not stop an import. */
  warnings: string[];
}

export interface PlanFile {
  /** Where the file is now, relative to the download folder. */
  source: string;
  /** Where it would go, relative to the music folder. */
  destination: string | null;
  shortened: boolean;
  /** Why it cannot go there yet, in plain words. */
  problem: string | null;
  tags: TagView | null;
  /** The format it will be converted to, such as `AIFF`; null when it goes in as it is. */
  becomes: string | null;
}

export type ConvertFormat = "aiff" | "wav" | "flac" | "mp3";

/** What the owner chose on the Import screen. Null keeps files as they are. */
export interface ImportOptions {
  convert: { lossless: ConvertFormat | null; lossy: ConvertFormat | null };
  /** Strip stray separators (such as a trailing ";") and padding from names. */
  tidy: boolean;
  /** Values accepted from a lookup, per file (named by its path in the download folder). */
  accepted?: Accepted[];
}

export interface Accepted {
  source: string;
  title?: string;
  artist?: string;
  album?: string;
  albumArtist?: string;
  label?: string;
  genre?: string;
  year?: number;
  track?: number;
  disc?: number;
}

export interface LookupKey {
  saved: boolean;
  /** The last four characters, to recognise it by. */
  ending: string | null;
}

export interface LookupKeys {
  discogs: LookupKey;
  acoustid: LookupKey;
  /** What saving found, in plain words. */
  note: string | null;
}

/** What a lookup (MusicBrainz, or Discogs with a token) suggests for a release's files. */
export interface ReleaseLookup {
  found: { source: string; title: string; artist: string; year: number | null; label: string | null; genre: string | null; tracks: number; url: string } | null;
  files: { source: string; matchedBy: string | null; changes: { field: string; from: string | null; to: string }[] }[];
  /** Why nothing was found. */
  note: string | null;
}

/** Where playback is. */
export interface PlayerState {
  trackId: number | null;
  playing: boolean;
  positionMs: number;
  durationMs: number | null;
  /** The track played to its end. */
  ended: boolean;
  error: string | null;
}

export type PlayerAction = "play" | "pause" | "stop" | "status" | "seek" | "volume";

export interface DuplicateCopy {
  fileId: number;
  path: string;
  /** The music folder it is in. */
  rootLabel: string;
  format: string;
  inRekordbox: boolean;
  analysed: boolean;
  cues: number;
  playlists: number;
}

export interface DuplicateCopies {
  copies: DuplicateCopy[];
  /** The copy to keep by the rule, when the rule decides. */
  suggested: number | null;
  note: string | null;
}

export interface ResolvedDuplicate {
  archived: number;
  merged: number;
  /** The archive folder, beside the music folder. */
  archive: string;
}

/** What a track's file holds, for the editor. */
export interface TrackTags {
  trackId: number;
  title: string | null;
  artist: string | null;
  album: string | null;
  albumArtist: string | null;
  genre: string | null;
  label: string | null;
  year: number | null;
  track: number | null;
}

export type EditField = "title" | "artist" | "album" | "albumArtist" | "genre" | "label" | "year" | "track";

/** One save of the track editor. */
export interface EditBatch { id: number; savedMs: number; tracks: number; fields: string[] }

/** The owner's custom fields on the chosen tracks, and every name in use. */
export interface CustomFields { fields: { trackId: number; name: string; value: string }[]; names: string[] }

/** What saving edits did. */
export interface EditedTracks {
  files: number;
  /** Files left as they were, and why. */
  left: string[];
}

export interface GenreCount { name: string; tracks: number }
/** Genres spelled alike, and the spelling suggested for all of them. */
export interface GenreGroup { suggested: string; members: string[]; tracks: number }
export interface Genres { genres: GenreCount[]; groups: GenreGroup[] }
export interface TrackName { trackId: number; title: string; artist: string }

/** A release whose files carry no picture. */
export interface MissingArt {
  releaseId: number;
  artist: string;
  title: string;
  files: number;
  /** An image such as cover.jpg sits beside the files. */
  folderImage: boolean;
}
export type ArtSource = "folder" | "online" | "choose";
export interface ArtChanged { files: number; left: string[] }
/** Whether chosen tracks carry a picture ("same": each the same one), and whether any has an image beside its files. */
export interface TrackCovers { state: "none" | "same" | "mixed"; folderImage: boolean }
/** A cover for chosen tracks: the image picked with `pickCover`, each track's own folder image, or none. */
export type TrackArt = "picked" | "folder" | "remove";

export type ExportFormat = "m3u8" | "csv";
export interface Exported { path: string; tracks: number; withoutFile: number }

export interface NamedTrack { trackId: number; title: string; artist: string }
/** Two crates side by side. */
export interface CrateComparison { both: NamedTrack[]; onlyFirst: NamedTrack[]; onlySecond: NamedTrack[] }

/** What tidying a folder did. */
export interface TidiedFolder {
  folder: string;
  suggested: string;
  moved: number;
  /** Moved files rekordbox was told about. */
  relocated: number;
  /** Files left where they are, and why. */
  left: string[];
}

/** What a read-only look over the library found. */
export interface LibraryHealth {
  files: number;
  /** Folders whose names carry stray separators from untidy tags. */
  untidy: { rootId: number; rootLabel: string; folder: string; suggested: string; files: number }[];
  /** The same recording (artist, title, length within 2 s) in more than one file. */
  duplicates: { artist: string; title: string; files: string[]; fileIds: number[]; /** The music folder of each file. */ rootLabels?: string[] }[];
  /** Files the scan could not read tags from, with why. */
  broken: { rootLabel: string; path: string; error: string }[];
}

/** How imported music is arranged inside a music folder. */
export interface FolderLayout {
  layout: string;
  /** Where a sample track would go. */
  example: string;
  presets: { name: string; layout: string }[];
  /** ffmpeg was found, so files can be converted on import. */
  canConvert: boolean;
}

/** A dry run of importing one release. Nothing is copied or written. */
export interface ImportPlan {
  release: string;
  rootLabel: string;
  files: PlanFile[];
  ready: number;
  blocked: number;
}

/** What importing a release did. */
export interface ReleaseImport {
  rootId: number;
  rootLabel: string;
  /** Files now in the music folder. */
  filed: number;
  /** Files left where they are, and why. */
  skipped: { source: string; problem: string }[];
}

export interface DownloadFolderState {
  folderId: number;
  label: string;
  state: "watching" | "off" | "offline" | "unreadable";
  unreadableDirs: number;
  truncated: boolean;
}

export interface DownloadsSnapshot {
  folders: DownloadFolderState[];
  releases: DownloadRelease[];
}

export interface TrackCue {
  /** rekordbox's Type: 0 cue, 1 fade-in, 2 fade-out, 3 load, 4 loop. */
  kind: number;
  /** -1 is a memory cue, 0 and up a hot cue slot. */
  number: number | null;
  startSeconds: number;
  endSeconds: number | null;
  name: string;
}

export interface TrackTempo {
  startSeconds: number;
  bpm: number;
  meter: string | null;
  beat: number | null;
}

/** What a rekordbox export says about one track. Shown as rekordbox wrote it; never edited here. */
export interface TrackAnalysis {
  generation: number;
  bpm: number | null;
  musicalKey: string;
  cues: TrackCue[];
  tempos: TrackTempo[];
}

/** One of the owner's crates: an ordered list of library tracks kept by openCrate only. */
export interface CrateInfo {
  id: number;
  name: string;
  tracks: number;
}

/** Managing the folders a library is built from. Only a backend with a database offers it. */
export interface LibraryAdmin {
  /** The database survives a restart. */
  persistent: boolean;
  folders(): Promise<FolderInfo[]>;
  /** Looks at every folder (read-only) so an unplugged drive is noticed without a scan. */
  checkFolders(): Promise<FolderInfo[]>;
  /** The owner says the folder now at this path is the library. */
  confirmFolder(folderId: number): Promise<void>;
  watchFolders(): Promise<WatchFolderInfo[]>;
  /** Looks at every download folder (read-only) so an unplugged drive is noticed. */
  checkWatchFolders(): Promise<WatchFolderInfo[]>;
  /** Opens the system folder picker for a download folder. Null if it was cancelled. */
  addWatchFolder(): Promise<WatchFolderInfo | null>;
  /** Lists the download folders (read-only) and says which releases look finished. */
  downloadReleases(): Promise<DownloadsSnapshot>;
  /** Where each file of a release would go in a music folder. Reads only. */
  planRelease(folderId: number, name: string, rootId: number, options?: ImportOptions): Promise<ImportPlan>;
  /** Copies the ready files of a release into a music folder. The originals stay where they are. */
  importRelease(folderId: number, name: string, rootId: number, options?: ImportOptions): Promise<ReleaseImport>;
  /** Asks MusicBrainz about a release; only when the owner presses the button. Writes nothing. */
  lookupRelease(folderId: number, name: string, rootId: number, options?: ImportOptions): Promise<ReleaseLookup>;
  /** Which of the owner's lookup keys are saved, each shown by its last four characters only. */
  lookupKeys(): Promise<LookupKeys>;
  /** Save or clear a key; a Discogs token is checked with Discogs first. */
  setLookupKey(service: "discogs" | "acoustid", value: string | null): Promise<LookupKeys>;
  /** A track's whole waveform by sound, 800 slices. Null when its file cannot be reached. */
  waveform(trackId: number): Promise<WaveBands | null>;
  /** The close-up for the scrolling view. Null when its file cannot be reached. */
  waveformDetail(trackId: number): Promise<WaveDetail | null>;
  /** Loads a track's file and plays it (or holds it paused) from `startMs`. */
  playTrack(trackId: number, startMs?: number, play?: boolean): Promise<void>;
  /** Controls the loaded track: `seek` takes a position in ms, `volume` 0 to 1. Resolves to where it is. */
  player(action: PlayerAction, value?: number): Promise<PlayerState>;
  /** Looks for untidy folder names and recordings filed twice. Reads only. */
  checkLibrary(): Promise<LibraryHealth>;
  /** The copies of a recording filed more than once, read against a fresh read of rekordbox. */
  duplicateCopies(fileIds: number[]): Promise<DuplicateCopies>;
  /** Keep one copy; archive the others and merge their rekordbox entries into the kept one's. */
  resolveDuplicate(keep: number, drop: number[]): Promise<ResolvedDuplicate>;
  /** What the chosen tracks' files hold. */
  trackTags(trackIds: number[]): Promise<TrackTags[]>;
  /** Write changes into every file of the chosen tracks; an empty value clears a field. */
  editTracks(trackIds: number[], changes: { field: EditField; value: string }[]): Promise<EditedTracks>;
  /** Each track's own changes, written in one pass. */
  editEach(edits: { trackId: number; changes: { field: EditField; value: string }[] }[]): Promise<EditedTracks>;
  /** The owner's custom fields on tracks, kept in openCrate's database. */
  customFields(trackIds: number[]): Promise<CustomFields>;
  /** Set one custom field on tracks; an empty value removes it. */
  setCustomField(trackIds: number[], name: string, value: string): Promise<void>;
  /** Every save of the track editor, newest first. */
  editHistory(): Promise<EditBatch[]>;
  /** Put back what the files held before the newest save, `id` (refused when a newer one exists). */
  undoLastEdit(id: number): Promise<EditedTracks>;
  /** Drop a save that cannot be undone, so the ones before it can be. */
  forgetEdit(id: number): Promise<void>;
  /** Every track's title and artist, for checking them against the cleanup rules. */
  trackNames(): Promise<TrackName[]>;
  /** Genres with their track counts, and the ones spelled alike. */
  genres(): Promise<Genres>;
  /** Give every track with one of the genres `from` the genre `to`. */
  renameGenres(from: string[], to: string): Promise<EditedTracks>;
  /** Export the tracks a query lists to a file the owner picks (null when cancelled). */
  exportTracks(query: LibraryQuery, format: ExportFormat, name: string): Promise<Exported | null>;
  /** Add every track of the crates `from` to `into`, each once; remove `from` when asked. Returns how many were added. */
  mergeCrates(into: number, from: number[], remove: boolean): Promise<number>;
  /** Put a crate in one of the library's orders, for good. */
  sortCrate(crateId: number, sort: Sort): Promise<void>;
  /** What two crates share and what only one holds. */
  compareCrates(first: number, second: number): Promise<CrateComparison>;
  /** Releases whose files carry no picture. */
  releasesWithoutArt(): Promise<MissingArt[]>;
  /** Give a release a cover from beside its files, the Cover Art Archive, or the system picker (null when cancelled). */
  setReleaseArt(releaseId: number, source: ArtSource): Promise<ArtChanged | null>;
  /** Strip every picture from a release's files. */
  removeReleaseArt(releaseId: number): Promise<ArtChanged>;
  trackCovers(trackIds: number[]): Promise<TrackCovers>;
  /** Pick a cover image with the system picker and hold it until `setTracksArt(…, "picked")`; false when cancelled. */
  pickCover(): Promise<boolean>;
  setTracksArt(trackIds: number[], art: TrackArt): Promise<ArtChanged>;
  /** Move a folder the check names to its tidy name; rekordbox follows the files it uses. */
  tidyFolder(rootId: number, folder: string): Promise<TidiedFolder>;
  folderLayout(): Promise<FolderLayout>;
  /** Checks and saves a layout; rejects with a plain reason if it cannot be used. */
  setFolderLayout(layout: string): Promise<FolderLayout>;
  setWatchFolderEnabled(id: number, enabled: boolean): Promise<void>;
  /** Removes the folder from the list. Nothing on disk is touched. */
  removeWatchFolder(id: number): Promise<void>;
  /** Counts everything in the folder now as already there. */
  retakeWatchBaseline(id: number): Promise<void>;
  /** Opens the system folder picker. Null if it was cancelled. */
  addFolder(): Promise<FolderInfo | null>;
  /** `confirmMissing` accepts a large disappearance that an earlier scan held back. */
  scan(folderId: number, confirmMissing?: boolean): Promise<void>;
  /** Removes the folder from the library. Music files are not touched. Resolves to the entries removed. */
  removeFolder(folderId: number): Promise<number>;
  scanStatus(): Promise<ScanStatus>;
  cancelScan(): Promise<void>;
  mirrors(): Promise<MirrorInfo[]>;
  playlists(): Promise<PlaylistInfo[]>;
  crates(): Promise<CrateInfo[]>;
  createCrate(name: string): Promise<CrateInfo>;
  renameCrate(crateId: number, name: string): Promise<void>;
  /** The tracks stay in the library. */
  deleteCrate(crateId: number): Promise<void>;
  /** Appends, skipping tracks already in the crate. Resolves to how many were added. */
  addToCrate(crateId: number, trackIds: number[]): Promise<number>;
  removeFromCrate(crateId: number, trackIds: number[]): Promise<number>;
  /** Moves tracks to just before `beforeId`, or to the end. Resolves to the new order. */
  moveInCrate(crateId: number, trackIds: number[], beforeId: number | null): Promise<number[]>;
  /** Arranges the whole track list by hand, starting from the order `fromSort` shows. */
  moveTracks(trackIds: number[], beforeId: number | null, fromSort: Sort): Promise<void>;
  /** The cues and beat grid rekordbox has for a track. Null if no export matched it. */
  trackAnalysis(trackId: number): Promise<TrackAnalysis | null>;
  /** Opens the system file picker for a rekordbox XML export. Null if it was cancelled. */
  importExport(): Promise<ImportResult | null>;
  rereadMirror(sourceId: number): Promise<ImportResult>;
  aliasSuggestions(sourceId: number): Promise<AliasSuggestion[]>;
  /** Tell a folder which rekordbox Location prefix belongs to it. */
  addAlias(folderId: number, prefix: string): Promise<boolean>;
  rekordboxDatabase(): Promise<RekordboxDatabase>;
  /** Turning it on records the rekordbox version the warning was confirmed for. */
  setRekordboxDatabase(enabled: boolean): Promise<RekordboxDatabase>;
  /** Backs up rekordbox's library, then reads the copy. rekordbox must be closed. */
  readRekordboxDatabase(): Promise<ImportResult>;
  /** Turn writing to rekordbox's library on or off. */
  setRekordboxWrite(enabled: boolean): Promise<RekordboxDatabase>;
  /** Put library tracks at the end of a rekordbox playlist from the sidebar. */
  addToRekordboxPlaylist(playlistId: number, trackIds: number[]): Promise<RekordboxWrite>;
  /** A new rekordbox playlist in a folder (the top of the tree when null) with a crate's tracks; the crate stays. */
  crateToRekordboxPlaylist(crateId: number, folderId: number | null): Promise<RekordboxWrite>;
  /** A new folder in rekordbox's playlist tree, inside a folder (the top of the tree when null). */
  createRekordboxFolder(name: string, parentId: number | null): Promise<RekordboxFolder>;
  /** Read rekordbox again and list what differs since both sides last agreed. */
  reviewRekordboxMerge(): Promise<MergeReview>;
  /** Apply the ticked changes. */
  applyRekordboxMerge(choices: MergeChoice[]): Promise<MergeApplied>;
  /** New tracks rekordbox does not have go into its "Imported" playlist: these files, or every one not kept out. */
  sendNewToRekordbox(fileIds?: number[]): Promise<RekordboxWrite>;
  rekordboxBackups(): Promise<RekordboxBackup[]>;
  pinRekordboxBackup(name: string, pinned: boolean): Promise<RekordboxBackup[]>;
  /** Puts a backup back; resolves to the backup of the library it replaced. */
  restoreRekordboxBackup(name: string): Promise<RekordboxBackup>;
  rekordboxNewTracks(): Promise<RekordboxNewTracks>;
  /** Writes the chosen tracks into openCrate's rekordbox XML file, in this order. */
  sendToRekordbox(fileIds: number[]): Promise<RekordboxSent>;
  /** Keeps tracks out of rekordbox, or brings them back. */
  hideFromRekordbox(fileIds: number[], hidden: boolean): Promise<RekordboxNewTracks>;
  rekordboxCrates(): Promise<RekordboxCrates>;
  /** Puts a crate in openCrate's rekordbox file or takes it out, and writes the file again. */
  setRekordboxCrate(crateId: number, inFile: boolean): Promise<RekordboxCrates>;
  /** Writes openCrate's rekordbox file again, with the crates as they are now. */
  updateRekordboxFile(): Promise<RekordboxCrates>;
}

/** A waveform by sound: for each slice, the level of its lows, mids and highs, 0 to 255. */
export interface WaveBands {
  low: number[];
  mid: number[];
  high: number[];
}

/** The close-up: many slices a second, kept as bytes. */
export interface WaveDetail {
  perSecond: number;
  low: ArrayLike<number>;
  mid: ArrayLike<number>;
  high: ArrayLike<number>;
}

export interface LibraryApi {
  /** Where the window can load a release's cover, or nothing when there is no library to ask. */
  artworkUrl?(releaseId: number): string | null;
  /** Where the editor can load a track's own picture, or the image picked and not yet saved. */
  coverUrl?(of: number | "picked"): string | null;
  /** Up to four of an artist's releases that have a cover, newest first. */
  artistCovers?(artistId: number): Promise<number[]>;
  /** Why the library the owner asked for could not be opened, if it could not. */
  startupError?: string;
  read(query: LibraryQuery, start: number, count: number, signal: AbortSignal): Promise<LibraryPage>;
  dispose(): void;
  admin?: LibraryAdmin;
}

export interface WorkerRequest {
  id: number;
  query: LibraryQuery;
  start: number;
  count: number;
}

export type WorkerReply =
  | { id: number; page: LibraryPage }
  | { id: number; error: string };

export const DEFAULT_QUERY: LibraryQuery = {
  view: "tracks", search: "", sort: "recent", format: "all", artistId: null, releaseId: null, playlistId: null, crateId: null,
};

export const MAX_WINDOW = 200;
