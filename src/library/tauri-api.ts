import {
  MAX_WINDOW,
  type AliasSuggestion,
  type CrateInfo,
  type FolderInfo,
  type Format,
  type ImportResult,
  type LibraryAdmin,
  type LibraryApi,
  type LibraryItem,
  type LibraryPage,
  type LibraryQuery,
  type EditedTracks,
  type ArtChanged,
  type TrackCovers,
  type MovedFile,
  type MovedFiles,
  type Relinked,
  type LookupKeys,
  type MergeApplied,
  type MergeChange,
  type MergeReview,
  type RekordboxFolder,
  type RekordboxWrite,
  type MirrorInfo,
  type RekordboxBackup,
  type RekordboxCandidate,
  type RekordboxCrates,
  type RekordboxNewTracks,
  type RekordboxSent,
  type RekordboxDatabase,
  type PlaylistInfo,
  type ScanStatus,
  type ScanSummary,
  type DownloadRelease,
  type DownloadsSnapshot,
  type DownloadState,
  type ImportPlan,
  type ReleaseImport,
  type FolderLayout,
  type ReleaseLookup,
  type LibraryHealth,
  type PlayerState,
  type TagView,
  type TrackAnalysis,
  type View,
  type WatchFolderInfo,
  type WaveBands,
  type WaveDetail,
} from "./types";

/** The shape of Tauri's command bridge, injected so tests need no native window. */
export type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

interface ItemWire {
  id: number;
  kind: View;
  title: string;
  artist: string;
  artistId: number;
  release: string;
  releaseId: number;
  bpm: number | null;
  musicalKey: string;
  durationMs: number;
  format: Format | null;
  trackCount: number;
  missing: boolean;
  analysed?: boolean;
  rekordboxKey?: boolean;
}

interface PageWire {
  items: ItemWire[];
  total: number;
  start: number;
}

const VIEWS: readonly string[] = ["tracks", "releases", "artists"];
const FORMATS: readonly string[] = ["AIFF", "MP3", "FLAC"];

/** How the desktop app turns a release id (or "track-7", "picked") into an address the window
 * can load, if it can. */
export function tauriArtwork(): ((key: number | string) => string) | null {
  const internals = (globalThis as { __TAURI_INTERNALS__?: { convertFileSrc?: (path: string, protocol: string) => string } }).__TAURI_INTERNALS__;
  const convert = internals?.convertFileSrc;
  return typeof convert === "function" ? (key) => convert.call(internals, String(key), "artwork") : null;
}

/** Tauri's bridge when running inside the desktop app, otherwise null. */
export function tauriInvoke(): Invoke | null {
  const internals = (globalThis as { __TAURI_INTERNALS__?: { invoke?: Invoke } }).__TAURI_INTERNALS__;
  const invoke = internals?.invoke;
  return typeof invoke === "function" ? (command, args) => invoke.call(internals, command, args) : null;
}

export interface LibraryMode {
  canManageFolders: boolean;
  persistent: boolean;
}

export interface LibraryStatus {
  /** Null unless the desktop app was started with a database-backed library. */
  mode: LibraryMode | null;
  /** Why a requested library could not be opened. */
  error: string | null;
}

export async function readLibraryStatus(invoke: Invoke): Promise<LibraryStatus> {
  try {
    const answer = await invoke("library_mode");
    if (!isRecord(answer)) return { mode: null, error: null };
    const error = typeof answer.error === "string" ? answer.error : null;
    if (answer.mode !== "database") return { mode: null, error };
    return { mode: { canManageFolders: answer.canManageFolders === true, persistent: answer.persistent === true }, error };
  } catch {
    return { mode: null, error: null };
  }
}

/** What the desktop app offers: null unless it was started with a database-backed library. */
export async function readLibraryMode(invoke: Invoke): Promise<LibraryMode | null> {
  return (await readLibraryStatus(invoke)).mode;
}

/** A query as the Rust commands take it: no format filter is `null`, never "all". */
function toWire(query: LibraryQuery) {
  return {
    view: query.view,
    search: query.search,
    sort: query.sort,
    format: query.format === "all" ? null : query.format,
    artistId: query.artistId,
    releaseId: query.releaseId,
    playlistId: query.playlistId,
    crateId: query.crateId,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function toItem(value: unknown): LibraryItem {
  if (!isRecord(value)) throw new Error("The library returned an unreadable row.");
  const row = value as Partial<ItemWire>;
  const text = (field: unknown) => (typeof field === "string" ? field : null);
  const number = (field: unknown) => (typeof field === "number" && Number.isFinite(field) ? field : null);
  const id = number(row.id);
  const artistId = number(row.artistId);
  const releaseId = number(row.releaseId);
  const durationMs = number(row.durationMs);
  const trackCount = number(row.trackCount);
  const title = text(row.title);
  const artist = text(row.artist);
  const release = text(row.release);
  const musicalKey = text(row.musicalKey);
  if (
    id === null || artistId === null || releaseId === null || durationMs === null || trackCount === null ||
    title === null || artist === null || release === null || musicalKey === null ||
    typeof row.kind !== "string" || !VIEWS.includes(row.kind) ||
    (row.bpm !== null && number(row.bpm) === null) ||
    (row.format !== null && (typeof row.format !== "string" || !FORMATS.includes(row.format)))
  ) {
    throw new Error("The library returned an unreadable row.");
  }
  return {
    id, kind: row.kind as View, title, artist, artistId, release, releaseId, missing: row.missing === true,
    analysed: row.analysed === true,
    rekordboxKey: row.rekordboxKey === true,
    bpm: row.bpm ?? null, musicalKey,
    duration: Math.round(durationMs / 1000),
    format: row.format as Format | null,
    trackCount,
    color: ((releaseId % 8) + 8) % 8,
  };
}

function toPage(value: unknown): LibraryPage {
  if (!isRecord(value)) throw new Error("The library returned an unreadable page.");
  const page = value as Partial<PageWire>;
  if (!Array.isArray(page.items) || typeof page.total !== "number" || typeof page.start !== "number") {
    throw new Error("The library returned an unreadable page.");
  }
  return { items: page.items.map(toItem), total: page.total, start: page.start };
}

const text = (value: unknown, what: string) => {
  if (typeof value !== "string") throw new Error(`The library returned an unreadable ${what}.`);
  return value;
};
const whole = (value: unknown, what: string) => {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) throw new Error(`The library returned an unreadable ${what}.`);
  return value;
};

function toFolder(value: unknown): FolderInfo {
  if (!isRecord(value)) throw new Error("The library returned an unreadable folder.");
  const last = value.lastAvailableMs;
  return {
    id: whole(value.id, "folder"), label: text(value.label, "folder"), path: text(value.path, "folder"),
    availability: text(value.availability, "folder"), reason: text(value.reason, "folder"),
    lastAvailableMs: last === null ? null : whole(last, "folder"),
    files: whole(value.files, "folder"), missing: whole(value.missing, "folder"),
  };
}

function toWatchFolder(value: unknown): WatchFolderInfo {
  if (!isRecord(value) || typeof value.enabled !== "boolean" || typeof value.online !== "boolean") {
    throw new Error("The library returned an unreadable download folder.");
  }
  const seen = value.lastSeenMs;
  return {
    id: whole(value.id, "download folder"), label: text(value.label, "download folder"),
    path: text(value.path, "download folder"), enabled: value.enabled, online: value.online,
    lastSeenMs: seen === null ? null : whole(seen, "download folder"),
  };
}

function toTagView(value: unknown): TagView {
  const bad = () => new Error("The library returned an unreadable import preview.");
  if (!isRecord(value) || !Array.isArray(value.fields) || !Array.isArray(value.warnings)) throw bad();
  return {
    fields: value.fields.map((field: unknown) => {
      if (!isRecord(field) || typeof field.guessed !== "boolean") throw bad();
      return {
        name: text(field.name, "import preview"), value: field.value === null ? null : text(field.value, "import preview"), guessed: field.guessed,
        tidiedFrom: field.tidiedFrom === null || field.tidiedFrom === undefined ? null : text(field.tidiedFrom, "import preview"),
      };
    }),
    audio: value.audio === null ? null : text(value.audio, "import preview"),
    length: value.length === null || value.length === undefined ? null : text(value.length, "import preview"),
    readError: value.readError === null ? null : text(value.readError, "import preview"),
    warnings: value.warnings.map((warning: unknown) => text(warning, "import preview")),
  };
}

function toLookupKeys(value: unknown): LookupKeys {
  const what = "lookup keys";
  const key = (field: unknown) => {
    if (!isRecord(field)) throw new Error(`The library returned unreadable ${what}.`);
    return { saved: flag(field.saved, what), ending: maybeText(field.ending ?? null, what) };
  };
  if (!isRecord(value)) throw new Error(`The library returned unreadable ${what}.`);
  return { discogs: key(value.discogs), acoustid: key(value.acoustid), note: maybeText(value.note ?? null, what) };
}

function toLookup(value: unknown): ReleaseLookup {
  const bad = () => new Error("The library returned an unreadable lookup.");
  if (!isRecord(value) || !Array.isArray(value.files)) throw bad();
  const maybe = (field: unknown) => (field === null || field === undefined ? null : text(field, "lookup"));
  let found: ReleaseLookup["found"] = null;
  if (value.found !== null && value.found !== undefined) {
    const f = value.found;
    if (!isRecord(f)) throw bad();
    found = {
      source: text(f.source, "lookup"), title: text(f.title, "lookup"), artist: text(f.artist, "lookup"),
      year: f.year === null ? null : whole(f.year, "lookup"), label: maybe(f.label), genre: maybe(f.genre),
      tracks: whole(f.tracks, "lookup"), url: text(f.url, "lookup"),
    };
  }
  return {
    found, note: maybe(value.note),
    files: value.files.map((file: unknown) => {
      if (!isRecord(file) || !Array.isArray(file.changes)) throw bad();
      return {
        source: text(file.source, "lookup"), matchedBy: maybe(file.matchedBy),
        changes: file.changes.map((change: unknown) => {
          if (!isRecord(change)) throw bad();
          return { field: text(change.field, "lookup"), from: maybe(change.from), to: text(change.to, "lookup") };
        }),
      };
    }),
  };
}

function toPlayer(value: unknown): PlayerState {
  const bad = () => new Error("The player returned an unreadable state.");
  if (!isRecord(value) || typeof value.playing !== "boolean" || typeof value.ended !== "boolean") throw bad();
  return {
    trackId: value.trackId === null || value.trackId === undefined ? null : whole(value.trackId, "player"),
    playing: value.playing, ended: value.ended,
    positionMs: whole(value.positionMs, "player"),
    durationMs: value.durationMs === null || value.durationMs === undefined ? null : whole(value.durationMs, "player"),
    error: value.error === null || value.error === undefined ? null : text(value.error, "player"),
  };
}

function toHealth(value: unknown): LibraryHealth {
  const bad = () => new Error("The library returned an unreadable check.");
  if (!isRecord(value) || !Array.isArray(value.untidy) || !Array.isArray(value.duplicates) || !Array.isArray(value.broken)) throw bad();
  return {
    files: whole(value.files, "check"),
    broken: value.broken.map((item: unknown) => {
      if (!isRecord(item)) throw bad();
      return { rootLabel: text(item.rootLabel, "check"), path: text(item.path, "check"), error: text(item.error, "check") };
    }),
    untidy: value.untidy.map((item: unknown) => {
      if (!isRecord(item)) throw bad();
      return { rootId: whole(item.rootId, "check"), rootLabel: text(item.rootLabel, "check"), folder: text(item.folder, "check"), suggested: text(item.suggested, "check"), files: whole(item.files, "check") };
    }),
    duplicates: value.duplicates.map((item: unknown) => {
      if (!isRecord(item) || !Array.isArray(item.files)) throw bad();
      if (!Array.isArray(item.fileIds)) throw bad();
      return {
        artist: text(item.artist, "check"), title: text(item.title, "check"), files: item.files.map((file: unknown) => text(file, "check")),
        fileIds: item.fileIds.map((id: unknown) => whole(id, "check")),
        ...(Array.isArray(item.rootLabels) ? { rootLabels: item.rootLabels.map((label: unknown) => text(label, "check")) } : {}),
      };
    }),
  };
}

function toLayout(value: unknown): FolderLayout {
  const bad = () => new Error("The library returned an unreadable folder layout.");
  if (!isRecord(value) || !Array.isArray(value.presets) || typeof value.canConvert !== "boolean") throw bad();
  return {
    layout: text(value.layout, "folder layout"), example: text(value.example, "folder layout"), canConvert: value.canConvert,
    presets: value.presets.map((preset: unknown) => {
      if (!isRecord(preset)) throw bad();
      return { name: text(preset.name, "folder layout"), layout: text(preset.layout, "folder layout") };
    }),
  };
}

function toReleaseImport(value: unknown): ReleaseImport {
  const bad = () => new Error("The library returned an unreadable import result.");
  if (!isRecord(value) || !Array.isArray(value.skipped)) throw bad();
  return {
    rootId: whole(value.rootId, "import"), rootLabel: text(value.rootLabel, "import"), filed: whole(value.filed, "import"),
    skipped: value.skipped.map((item: unknown) => {
      if (!isRecord(item)) throw bad();
      return { source: text(item.source, "import"), problem: text(item.problem, "import") };
    }),
  };
}

function toPlan(value: unknown): ImportPlan {
  const bad = () => new Error("The library returned an unreadable import preview.");
  if (!isRecord(value) || !Array.isArray(value.files)) throw bad();
  return {
    release: text(value.release, "import preview"), rootLabel: text(value.rootLabel, "import preview"),
    ready: whole(value.ready, "import preview"), blocked: whole(value.blocked, "import preview"),
    files: value.files.map((file: unknown) => {
      if (!isRecord(file) || typeof file.shortened !== "boolean") throw bad();
      return {
        source: text(file.source, "import preview"),
        destination: file.destination === null ? null : text(file.destination, "import preview"),
        shortened: file.shortened,
        problem: file.problem === null ? null : text(file.problem, "import preview"),
        tags: file.tags === null ? null : toTagView(file.tags),
        becomes: file.becomes === null || file.becomes === undefined ? null : text(file.becomes, "import preview"),
      };
    }),
  };
}

const DOWNLOAD_STATES: readonly string[] = ["ready", "downloading", "settling", "nothing"];
const WATCH_STATES = ["watching", "off", "offline", "unreadable"] as const;

function toDownloads(value: unknown): DownloadsSnapshot {
  if (!isRecord(value) || !Array.isArray(value.folders) || !Array.isArray(value.releases)) {
    throw new Error("The library returned an unreadable list of downloads.");
  }
  const folders = value.folders.map((folder: unknown) => {
    const state = isRecord(folder) ? WATCH_STATES.find((candidate) => candidate === folder.state) : undefined;
    if (!isRecord(folder) || !state || typeof folder.truncated !== "boolean") {
      throw new Error("The library returned an unreadable list of downloads.");
    }
    return {
      folderId: whole(folder.folderId, "download"), label: text(folder.label, "download"), state,
      unreadableDirs: whole(folder.unreadableDirs, "download"), truncated: folder.truncated,
    };
  });
  const releases = value.releases.map((release: unknown): DownloadRelease => {
    if (
      !isRecord(release) || typeof release.state !== "string" || !DOWNLOAD_STATES.includes(release.state) ||
      typeof release.loose !== "boolean" || typeof release.alreadyThere !== "boolean" ||
      typeof release.imported !== "boolean"
    ) {
      throw new Error("The library returned an unreadable list of downloads.");
    }
    return {
      folderId: whole(release.folderId, "download"), folderLabel: text(release.folderLabel, "download"),
      name: text(release.name, "download"), loose: release.loose, state: release.state as DownloadState,
      quietSeconds: release.quietSeconds === null ? null : whole(release.quietSeconds, "download"),
      audioFiles: whole(release.audioFiles, "download"), archives: whole(release.archives, "download"),
      alreadyThere: release.alreadyThere, imported: release.imported,
      newestModifiedMs: typeof release.newestModifiedMs === "number" ? release.newestModifiedMs : 0,
    };
  });
  return { folders, releases };
}

function toAnalysis(value: unknown): TrackAnalysis {
  const bad = () => new Error("The library returned an unreadable cue list.");
  if (!isRecord(value) || !Array.isArray(value.cues) || !Array.isArray(value.tempos)) throw bad();
  const real = (field: unknown) => {
    if (typeof field !== "number" || !Number.isFinite(field)) throw bad();
    return field;
  };
  const maybe = (field: unknown) => (field === null ? null : real(field));
  return {
    generation: whole(value.generation, "cue list"),
    bpm: maybe(value.bpm),
    musicalKey: text(value.musicalKey, "cue list"),
    cues: value.cues.map((cue: unknown) => {
      if (!isRecord(cue)) throw bad();
      return {
        kind: real(cue.kind), number: maybe(cue.number), startSeconds: real(cue.startSeconds),
        endSeconds: maybe(cue.endSeconds), name: text(cue.name, "cue list"),
      };
    }),
    tempos: value.tempos.map((tempo: unknown) => {
      if (!isRecord(tempo)) throw bad();
      return {
        startSeconds: real(tempo.startSeconds), bpm: real(tempo.bpm),
        meter: tempo.meter === null ? null : text(tempo.meter, "cue list"), beat: maybe(tempo.beat),
      };
    }),
  };
}

function toSummary(value: unknown): ScanSummary {
  if (!isRecord(value)) throw new Error("The library returned an unreadable scan result.");
  const n = (field: string) => whole(value[field], "scan result");
  return {
    availability: text(value.availability, "scan result"), reason: text(value.reason, "scan result"),
    inserted: n("inserted"), updated: n("updated"), unchanged: n("unchanged"), revived: n("revived"),
    markedMissing: n("markedMissing"), heldMissing: n("heldMissing"), tracksCreated: n("tracksCreated"),
    tracksUpdated: n("tracksUpdated"), tagErrors: n("tagErrors"), cancelled: value.cancelled === true,
  };
}

const SCAN_STATES = ["idle", "running", "finished", "failed"] as const;

function toStatus(value: unknown): ScanStatus {
  if (!isRecord(value)) throw new Error("The library returned an unreadable scan status.");
  const state = SCAN_STATES.find((candidate) => candidate === value.state);
  if (!state) throw new Error("The library returned an unreadable scan status.");
  const id = value.rootId;
  return {
    state,
    rootId: id === null ? null : whole(id, "scan status"),
    phase: value.phase === null ? null : text(value.phase, "scan status"),
    filesSeen: whole(value.filesSeen, "scan status"),
    tagsRead: whole(value.tagsRead, "scan status"),
    tagsTotal: whole(value.tagsTotal, "scan status"),
    summary: value.summary === null ? null : toSummary(value.summary),
    error: value.error === null ? null : text(value.error, "scan status"),
  };
}

function toMirror(value: unknown): MirrorInfo {
  if (!isRecord(value) || !isRecord(value.counts)) throw new Error("The library returned an unreadable rekordbox export.");
  const counts = value.counts;
  const optional = (field: unknown) => (field === null ? null : whole(field, "rekordbox export"));
  const n = (field: string) => whole(counts[field], "rekordbox export");
  if (value.kind !== "xml" && value.kind !== "master_db") throw new Error("The library returned an unreadable rekordbox export.");
  return {
    sourceId: whole(value.sourceId, "rekordbox export"), kind: value.kind, path: text(value.path, "rekordbox export"),
    generation: whole(value.generation, "rekordbox export"), entries: whole(value.entries, "rekordbox export"),
    fileMtimeMs: optional(value.fileMtimeMs), lastReadMs: optional(value.lastReadMs),
    lastAttemptMs: optional(value.lastAttemptMs),
    lastError: value.lastError === null ? null : text(value.lastError, "rekordbox export"),
    changedSinceRead: flag(value.changedSinceRead, "rekordbox export"),
    counts: {
      matched: n("matched"), unmapped: n("unmapped"), notIndexed: n("notIndexed"), ambiguous: n("ambiguous"),
      unrecognized: n("unrecognized"), playlists: n("playlists"), unresolvedReferences: n("unresolvedReferences"),
    },
  };
}

function toPlaylist(value: unknown): PlaylistInfo {
  if (!isRecord(value) || typeof value.isFolder !== "boolean") throw new Error("The library returned an unreadable playlist.");
  return {
    id: whole(value.id, "playlist"), parentId: value.parentId === null ? null : whole(value.parentId, "playlist"),
    isFolder: value.isFolder, name: text(value.name, "playlist"), position: whole(value.position, "playlist"),
    entries: whole(value.entries, "playlist"), inLibrary: whole(value.inLibrary, "playlist"),
  };
}

const flag = (value: unknown, what: string) => {
  if (typeof value !== "boolean") throw new Error(`The library returned an unreadable ${what}.`);
  return value;
};
const maybeText = (value: unknown, what: string) => (value === null ? null : text(value, what));

function toRekordboxDatabase(value: unknown): RekordboxDatabase {
  const what = "rekordbox setting";
  if (!isRecord(value)) throw new Error(`The library returned an unreadable ${what}.`);
  return {
    installed: flag(value.installed, what), appVersion: maybeText(value.appVersion, what), running: flag(value.running, what),
    enabled: flag(value.enabled, what), confirmedVersion: maybeText(value.confirmedVersion, what),
    needsConfirmation: flag(value.needsConfirmation, what),
    writeEnabled: flag(value.writeEnabled, what), writeConfirmedVersion: maybeText(value.writeConfirmedVersion, what),
    writeNeedsConfirmation: flag(value.writeNeedsConfirmation, what), writeTested: flag(value.writeTested, what),
  };
}

const FIELDS = ["title", "artist", "album", "year", "label"] as const;
const DIRECTIONS = ["toRekordbox", "toOpenCrate", "conflict"] as const;

function toMergeChange(value: unknown): MergeChange {
  const what = "merge change";
  if (!isRecord(value)) throw new Error(`The library returned an unreadable ${what}.`);
  const field = FIELDS.find((name) => name === value.field);
  const direction = DIRECTIONS.find((name) => name === value.direction);
  if (!field || !direction) throw new Error(`The library returned an unreadable ${what}.`);
  return {
    rbKey: whole(value.rbKey, what), trackId: whole(value.trackId, what), title: text(value.title, what), artist: text(value.artist, what),
    field, base: maybeText(value.base, what), ours: text(value.ours, what), theirs: text(value.theirs, what), direction,
  };
}

function toMergeReview(value: unknown): MergeReview {
  const what = "merge review";
  if (!isRecord(value) || !Array.isArray(value.changes)) throw new Error(`The library returned an unreadable ${what}.`);
  return { changes: value.changes.map(toMergeChange), canWrite: flag(value.canWrite, what) };
}

function toMergeApplied(value: unknown): MergeApplied {
  const what = "merge result";
  if (!isRecord(value) || !Array.isArray(value.skipped)) throw new Error(`The library returned an unreadable ${what}.`);
  return {
    toRekordbox: whole(value.toRekordbox, what), toOpencrate: whole(value.toOpencrate, what),
    skipped: value.skipped.map((item) => text(item, what)),
  };
}

function toMovedFile(value: unknown): MovedFile {
  const what = "moved file";
  if (!isRecord(value)) throw new Error(`The library returned an unreadable ${what}.`);
  return { fileId: whole(value.fileId, what), title: text(value.title, what), artist: text(value.artist, what), path: text(value.path, what) };
}

function toMovedFiles(value: unknown): MovedFiles {
  if (!isRecord(value) || !Array.isArray(value.moved) || !Array.isArray(value.unmatched)) throw new Error("The library returned unreadable moved files.");
  return {
    moved: value.moved.map((item: unknown) => {
      if (!isRecord(item)) throw new Error("The library returned unreadable moved files.");
      return { missing: toMovedFile(item.missing), found: toMovedFile(item.found), sameFile: flag(item.sameFile, "moved file") };
    }),
    unmatched: value.unmatched.map(toMovedFile),
  };
}

function toRelinked(value: unknown): Relinked {
  const what = "relink";
  if (!isRecord(value) || !Array.isArray(value.left)) throw new Error(`The library returned an unreadable ${what}.`);
  return { relinked: whole(value.relinked, what), relocated: whole(value.relocated, what), left: value.left.map((item) => text(item, what)) };
}

function toArtChanged(value: unknown): ArtChanged {
  const what = "art change";
  if (!isRecord(value) || !Array.isArray(value.left)) throw new Error(`The library returned an unreadable ${what}.`);
  return { files: whole(value.files, what), left: value.left.map((item) => text(item, what)) };
}

function toEdited(value: unknown): EditedTracks {
  const what = "edit result";
  if (!isRecord(value) || !Array.isArray(value.left)) throw new Error(`The library returned an unreadable ${what}.`);
  return { files: whole(value.files, what), left: value.left.map((item) => text(item, what)) };
}

function toRekordboxWrite(value: unknown): RekordboxWrite {
  const what = "rekordbox write";
  if (!isRecord(value) || !Array.isArray(value.leftOut)) throw new Error(`The library returned an unreadable ${what}.`);
  return {
    playlist: text(value.playlist, what), added: whole(value.added, what), imported: whole(value.imported, what),
    leftOut: value.leftOut.map((item) => text(item, what)), readError: maybeText(value.readError, what),
  };
}

function toRekordboxFolder(value: unknown): RekordboxFolder {
  const what = "rekordbox folder";
  if (!isRecord(value)) throw new Error(`The library returned an unreadable ${what}.`);
  return { name: text(value.name, what), readError: maybeText(value.readError, what) };
}

function toBackup(value: unknown): RekordboxBackup {
  const what = "rekordbox backup";
  if (!isRecord(value)) throw new Error(`The library returned an unreadable ${what}.`);
  return {
    name: text(value.name, what), takenMs: whole(value.takenMs, what), reason: text(value.reason, what),
    pinned: flag(value.pinned, what), appVersion: maybeText(value.appVersion, what),
    tracks: whole(value.tracks, what), playlists: whole(value.playlists, what),
  };
}

function toBackups(value: unknown): RekordboxBackup[] {
  if (!Array.isArray(value)) throw new Error("The library returned an unreadable list of rekordbox backups.");
  return value.map(toBackup);
}

function toCandidate(value: unknown): RekordboxCandidate {
  const what = "new track";
  if (!isRecord(value)) throw new Error(`The library returned an unreadable ${what}.`);
  return {
    fileId: whole(value.fileId, what), title: text(value.title, what), artist: text(value.artist, what), album: text(value.album, what),
    durationMs: whole(value.durationMs, what), container: text(value.container, what), filedMs: whole(value.filedMs, what),
    offeredMs: value.offeredMs === null ? null : whole(value.offeredMs, what), hidden: flag(value.hidden, what),
  };
}

function toNewTracks(value: unknown): RekordboxNewTracks {
  const what = "list of new tracks";
  if (!isRecord(value) || !Array.isArray(value.tracks)) throw new Error(`The library returned an unreadable ${what}.`);
  const optional = (field: unknown) => (field === null ? null : whole(field, what));
  return {
    tracks: value.tracks.map(toCandidate), mirrorReadMs: optional(value.mirrorReadMs), newestFiledMs: optional(value.newestFiledMs),
    xmlPath: text(value.xmlPath, what), playlist: text(value.playlist, what),
  };
}

function toCrates(value: unknown): RekordboxCrates {
  const what = "list of crates for rekordbox";
  if (!isRecord(value) || !Array.isArray(value.crates)) throw new Error(`The library returned an unreadable ${what}.`);
  return {
    crates: value.crates.map((crate: unknown) => {
      if (!isRecord(crate)) throw new Error(`The library returned an unreadable ${what}.`);
      return {
        crateId: whole(crate.crateId, what), name: text(crate.name, what), inFile: flag(crate.inFile, what),
        known: whole(crate.known, what), new: whole(crate.new, what), needsExport: whole(crate.needsExport, what), noFile: whole(crate.noFile, what),
      };
    }),
    xmlPath: text(value.xmlPath, what), folder: text(value.folder, what),
  };
}

function toSent(value: unknown): RekordboxSent {
  if (!isRecord(value)) throw new Error("The library returned an unreadable send result.");
  return { xmlPath: text(value.xmlPath, "send result"), tracks: whole(value.tracks, "send result") };
}

/** Interleaved low, mid, high bytes, split into the three bands. */
function toBands(value: unknown, most: number): WaveBands {
  if (!Array.isArray(value) || value.length % 3 !== 0 || value.length > most || !value.every((n) => Number.isInteger(n) && n >= 0 && n <= 255)) {
    throw new Error("The library returned an unreadable waveform.");
  }
  const values = value as number[];
  const band = (offset: number) => Array.from({ length: values.length / 3 }, (_, i) => values[i * 3 + offset] as number);
  return { low: band(0), mid: band(1), high: band(2) };
}

/** The close-up arrives as raw bytes: slices a second (two bytes, little-endian), then low, mid
 * and high per slice. No bytes means the track's file cannot be reached. */
function toDetail(value: unknown): WaveDetail | null {
  const unreadable = () => new Error("The library returned an unreadable waveform.");
  let bytes: Uint8Array;
  if (value instanceof ArrayBuffer) bytes = new Uint8Array(value);
  else if (ArrayBuffer.isView(value)) bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  else if (Array.isArray(value) && value.every((n) => Number.isInteger(n) && n >= 0 && n <= 255)) bytes = Uint8Array.from(value as number[]);
  else throw unreadable();
  if (bytes.length === 0) return null;
  const perSecond = (bytes[0] ?? 0) | ((bytes[1] ?? 0) << 8);
  if (bytes.length < 2 || (bytes.length - 2) % 3 !== 0 || perSecond < 1) throw unreadable();
  const slices = (bytes.length - 2) / 3;
  const low = new Uint8Array(slices);
  const mid = new Uint8Array(slices);
  const high = new Uint8Array(slices);
  for (let i = 0; i < slices; i += 1) {
    low[i] = bytes[2 + i * 3] ?? 0;
    mid[i] = bytes[3 + i * 3] ?? 0;
    high[i] = bytes[4 + i * 3] ?? 0;
  }
  return { perSecond, low, mid, high };
}

function toImport(value: unknown): ImportResult {
  if (!isRecord(value) || typeof value.unchanged !== "boolean") throw new Error("The library returned an unreadable import result.");
  return {
    sourceId: whole(value.sourceId, "import result"), generation: whole(value.generation, "import result"),
    unchanged: value.unchanged, entries: whole(value.entries, "import result"),
    matched: whole(value.matched, "import result"), derivedSet: whole(value.derivedSet, "import result"),
  };
}

function toCrate(value: unknown): CrateInfo {
  if (!isRecord(value)) throw new Error("The library returned an unreadable crate.");
  return { id: whole(value.id, "crate"), name: text(value.name, "crate"), tracks: whole(value.tracks, "crate") };
}

const ids = (value: unknown, what: string): number[] => {
  if (!Array.isArray(value)) throw new Error(`The library returned an unreadable ${what}.`);
  return value.map((id) => whole(id, what));
};

function toSuggestion(value: unknown): AliasSuggestion {
  if (!isRecord(value)) throw new Error("The library returned an unreadable suggestion.");
  return {
    rootId: whole(value.rootId, "suggestion"), rootLabel: text(value.rootLabel, "suggestion"),
    prefix: text(value.prefix, "suggestion"), entries: whole(value.entries, "suggestion"),
  };
}

function failure(error: unknown): Error {
  return error instanceof Error ? error : new Error(typeof error === "string" ? error : "The library could not be read.");
}

function adminFor(invoke: Invoke, persistent: boolean): LibraryAdmin {
  return {
    persistent,
    folders: async () => {
      const response = await invoke("library_roots").catch((error: unknown) => { throw failure(error); });
      if (!Array.isArray(response)) throw new Error("The library returned an unreadable folder list.");
      return response.map(toFolder);
    },
    checkFolders: async () => {
      const response = await invoke("library_check_roots").catch((error: unknown) => { throw failure(error); });
      if (!Array.isArray(response)) throw new Error("The library returned an unreadable folder list.");
      return response.map(toFolder);
    },
    confirmFolder: async (folderId) => {
      await invoke("library_confirm_root", { rootId: folderId }).catch((error: unknown) => { throw failure(error); });
    },
    watchFolders: async () => {
      const response = await invoke("library_watch_folders").catch((error: unknown) => { throw failure(error); });
      if (!Array.isArray(response)) throw new Error("The library returned an unreadable list of download folders.");
      return response.map(toWatchFolder);
    },
    checkWatchFolders: async () => {
      const response = await invoke("library_check_watch_folders").catch((error: unknown) => { throw failure(error); });
      if (!Array.isArray(response)) throw new Error("The library returned an unreadable list of download folders.");
      return response.map(toWatchFolder);
    },
    addWatchFolder: async () => {
      const response = await invoke("library_add_watch_folder").catch((error: unknown) => { throw failure(error); });
      return response === null ? null : toWatchFolder(response);
    },
    downloadReleases: async () => toDownloads(await invoke("library_download_releases").catch((error: unknown) => { throw failure(error); })),
    planRelease: async (folderId, name, rootId, options) =>
      toPlan(await invoke("library_plan_release", { folderId, name, rootId, options: options ?? null }).catch((error: unknown) => { throw failure(error); })),
    importRelease: async (folderId, name, rootId, options) =>
      toReleaseImport(await invoke("library_import_release", { folderId, name, rootId, options: options ?? null }).catch((error: unknown) => { throw failure(error); })),
    lookupRelease: async (folderId, name, rootId, options) =>
      toLookup(await invoke("library_lookup_release", { folderId, name, rootId, options: options ?? null }).catch((error: unknown) => { throw failure(error); })),
    waveform: async (trackId) => {
      const value = await invoke("library_waveform", { trackId }).catch((error: unknown) => { throw failure(error); });
      return value === null ? null : toBands(value, 4096 * 3);
    },
    waveformDetail: async (trackId) =>
      toDetail(await invoke("library_waveform_detail", { trackId }).catch((error: unknown) => { throw failure(error); })),
    playTrack: async (trackId, startMs = 0, play = true) => {
      await invoke("player_load", { trackId, startMs, play }).catch((error: unknown) => { throw failure(error); });
    },
    player: async (action, value) => {
      const args = action === "seek" ? { action, positionMs: Math.max(0, Math.round(value ?? 0)) } : action === "volume" ? { action, volume: value ?? 1 } : { action };
      return toPlayer(await invoke("player_control", args).catch((error: unknown) => { throw failure(error); }));
    },
    lookupKeys: async () => toLookupKeys(await invoke("lookup_keys").catch((error: unknown) => { throw failure(error); })),
    setLookupKey: async (service, value) =>
      toLookupKeys(await invoke("lookup_set_key", { serviceName: service, value }).catch((error: unknown) => { throw failure(error); })),
    checkLibrary: async () => toHealth(await invoke("library_check").catch((error: unknown) => { throw failure(error); })),
    trackTags: async (trackIds) => {
      const value: unknown = await invoke("library_track_tags", { trackIds }).catch((error: unknown) => { throw failure(error); });
      const what = "track tags";
      if (!Array.isArray(value)) throw new Error(`The library returned unreadable ${what}.`);
      const maybeNumber = (field: unknown) => (field === null || field === undefined ? null : whole(field, what));
      return value.map((item: unknown) => {
        if (!isRecord(item)) throw new Error(`The library returned unreadable ${what}.`);
        const maybe = (field: unknown) => maybeText(field ?? null, what);
        return {
          trackId: whole(item.trackId, what), title: maybe(item.title), artist: maybe(item.artist), album: maybe(item.album),
          albumArtist: maybe(item.albumArtist), genre: maybe(item.genre), label: maybe(item.label),
          year: maybeNumber(item.year), track: maybeNumber(item.track),
        };
      });
    },
    editTracks: async (trackIds, changes) => toEdited(await invoke("library_edit_tracks", { trackIds, changes }).catch((error: unknown) => { throw failure(error); })),
    exportTracks: async (query, format, name) => {
      const value: unknown = await invoke("library_export", { query: toWire(query), format, name }).catch((error: unknown) => { throw failure(error); });
      if (value === null) return null;
      const what = "export";
      if (!isRecord(value)) throw new Error(`The library returned an unreadable ${what}.`);
      return { path: text(value.path, what), tracks: whole(value.tracks, what), withoutFile: whole(value.withoutFile, what) };
    },
    mergeCrates: async (into, from, remove) => {
      const value: unknown = await invoke("library_merge_crates", { into, from, remove }).catch((error: unknown) => { throw failure(error); });
      return whole(value, "merge");
    },
    sortCrate: async (crateId, sort) => { await invoke("library_sort_crate", { crateId, sort }).catch((error: unknown) => { throw failure(error); }); },
    compareCrates: async (first, second) => {
      const value: unknown = await invoke("library_compare_crates", { first, second }).catch((error: unknown) => { throw failure(error); });
      const what = "crate comparison";
      if (!isRecord(value) || !Array.isArray(value.both) || !Array.isArray(value.onlyFirst) || !Array.isArray(value.onlySecond)) throw new Error(`The library returned an unreadable ${what}.`);
      const named = (items: unknown[]) => items.map((item) => {
        if (!isRecord(item)) throw new Error(`The library returned an unreadable ${what}.`);
        return { trackId: whole(item.trackId, what), title: text(item.title, what), artist: text(item.artist, what) };
      });
      return { both: named(value.both), onlyFirst: named(value.onlyFirst), onlySecond: named(value.onlySecond) };
    },
    releasesWithoutArt: async () => {
      const value: unknown = await invoke("library_releases_without_art").catch((error: unknown) => { throw failure(error); });
      const what = "releases without art";
      if (!Array.isArray(value)) throw new Error(`The library returned unreadable ${what}.`);
      return value.map((item: unknown) => {
        if (!isRecord(item)) throw new Error(`The library returned unreadable ${what}.`);
        return { releaseId: whole(item.releaseId, what), artist: text(item.artist, what), title: text(item.title, what), files: whole(item.files, what), folderImage: flag(item.folderImage, what) };
      });
    },
    setReleaseArt: async (releaseId, source) => {
      const value: unknown = await invoke("library_set_release_art", { releaseId, source }).catch((error: unknown) => { throw failure(error); });
      return value === null ? null : toArtChanged(value);
    },
    removeReleaseArt: async (releaseId) => toArtChanged(await invoke("library_remove_release_art", { releaseId }).catch((error: unknown) => { throw failure(error); })),
    trackCovers: async (trackIds) => {
      const value: unknown = await invoke("library_track_covers", { trackIds }).catch((error: unknown) => { throw failure(error); });
      if (!isRecord(value) || !["none", "same", "mixed"].includes(value.state as string)) throw new Error("The library returned unreadable covers.");
      return { state: value.state as TrackCovers["state"], folderImage: flag(value.folderImage, "covers") };
    },
    movedFiles: async () => toMovedFiles(await invoke("library_moved_files").catch((error: unknown) => { throw failure(error); })),
    relink: async (pairs) => toRelinked(await invoke("library_relink", { pairs }).catch((error: unknown) => { throw failure(error); })),
    relinkChosen: async (missing) => {
      const value: unknown = await invoke("library_relink_chosen", { missing }).catch((error: unknown) => { throw failure(error); });
      return value === null ? null : toRelinked(value);
    },
    pickCover: async () => (await invoke("library_pick_cover").catch((error: unknown) => { throw failure(error); })) === true,
    setTracksArt: async (trackIds, art) => toArtChanged(await invoke("library_set_tracks_art", { trackIds, source: art }).catch((error: unknown) => { throw failure(error); })),
    editEach: async (edits) => toEdited(await invoke("library_edit_each", { edits }).catch((error: unknown) => { throw failure(error); })),
    customFields: async (trackIds) => {
      const value: unknown = await invoke("library_custom_fields", { trackIds }).catch((error: unknown) => { throw failure(error); });
      const what = "custom fields";
      if (!isRecord(value) || !Array.isArray(value.fields) || !Array.isArray(value.names)) throw new Error(`The library returned unreadable ${what}.`);
      return {
        fields: value.fields.map((item: unknown) => {
          if (!isRecord(item)) throw new Error(`The library returned unreadable ${what}.`);
          return { trackId: whole(item.trackId, what), name: text(item.name, what), value: text(item.value, what) };
        }),
        names: value.names.map((name: unknown) => text(name, what)),
      };
    },
    setCustomField: async (trackIds, name, value) => { await invoke("library_set_custom_field", { trackIds, name, value }).catch((error: unknown) => { throw failure(error); }); },
    editHistory: async () => {
      const value: unknown = await invoke("library_edit_history").catch((error: unknown) => { throw failure(error); });
      const what = "edit history";
      if (!Array.isArray(value)) throw new Error(`The library returned unreadable ${what}.`);
      return value.map((item: unknown) => {
        if (!isRecord(item) || !Array.isArray(item.fields)) throw new Error(`The library returned unreadable ${what}.`);
        return { id: whole(item.id, what), savedMs: whole(item.savedMs, what), tracks: whole(item.tracks, what), fields: item.fields.map((field: unknown) => text(field, what)) };
      });
    },
    undoLastEdit: async (id) => toEdited(await invoke("library_undo_last_edit", { id }).catch((error: unknown) => { throw failure(error); })),
    forgetEdit: async (id) => { await invoke("library_forget_edit", { id }).catch((error: unknown) => { throw failure(error); }); },
    trackNames: async () => {
      const value: unknown = await invoke("library_track_names").catch((error: unknown) => { throw failure(error); });
      const what = "track names";
      if (!Array.isArray(value)) throw new Error(`The library returned unreadable ${what}.`);
      return value.map((item: unknown) => {
        if (!isRecord(item)) throw new Error(`The library returned unreadable ${what}.`);
        return { trackId: whole(item.trackId, what), title: text(item.title, what), artist: text(item.artist, what) };
      });
    },
    genres: async () => {
      const value: unknown = await invoke("library_genres").catch((error: unknown) => { throw failure(error); });
      const what = "genres";
      if (!isRecord(value) || !Array.isArray(value.genres) || !Array.isArray(value.groups)) throw new Error(`The library returned unreadable ${what}.`);
      return {
        genres: value.genres.map((item: unknown) => {
          if (!isRecord(item)) throw new Error(`The library returned unreadable ${what}.`);
          return { name: text(item.name, what), tracks: whole(item.tracks, what) };
        }),
        groups: value.groups.map((item: unknown) => {
          if (!isRecord(item) || !Array.isArray(item.members)) throw new Error(`The library returned unreadable ${what}.`);
          return { suggested: text(item.suggested, what), members: item.members.map((member: unknown) => text(member, what)), tracks: whole(item.tracks, what) };
        }),
      };
    },
    renameGenres: async (from, to) => toEdited(await invoke("library_rename_genres", { from, to }).catch((error: unknown) => { throw failure(error); })),
    duplicateCopies: async (fileIds) => {
      const value: unknown = await invoke("library_duplicate_copies", { fileIds }).catch((error: unknown) => { throw failure(error); });
      const what = "duplicate copies";
      if (!isRecord(value) || !Array.isArray(value.copies)) throw new Error(`The library returned unreadable ${what}.`);
      return {
        copies: value.copies.map((copy: unknown) => {
          if (!isRecord(copy)) throw new Error(`The library returned unreadable ${what}.`);
          return {
            fileId: whole(copy.fileId, what), path: text(copy.path, what), rootLabel: text(copy.rootLabel ?? "", what), format: text(copy.format, what),
            inRekordbox: flag(copy.inRekordbox, what), analysed: flag(copy.analysed, what), cues: whole(copy.cues, what), playlists: whole(copy.playlists, what),
          };
        }),
        suggested: value.suggested === null ? null : whole(value.suggested, what), note: maybeText(value.note ?? null, what),
      };
    },
    resolveDuplicate: async (keep, drop) => {
      const value: unknown = await invoke("library_resolve_duplicate", { keep, drop }).catch((error: unknown) => { throw failure(error); });
      const what = "resolved duplicate";
      if (!isRecord(value)) throw new Error(`The library returned an unreadable ${what}.`);
      return { archived: whole(value.archived, what), merged: whole(value.merged, what), archive: text(value.archive, what) };
    },
    tidyFolder: async (rootId, folder) => {
      const value: unknown = await invoke("library_tidy_folder", { rootId, folder }).catch((error: unknown) => { throw failure(error); });
      const what = "tidied folder";
      if (!isRecord(value) || !Array.isArray(value.left)) throw new Error(`The library returned an unreadable ${what}.`);
      return {
        folder: text(value.folder, what), suggested: text(value.suggested, what), moved: whole(value.moved, what),
        relocated: whole(value.relocated, what), left: value.left.map((item) => text(item, what)),
      };
    },
    folderLayout: async () => toLayout(await invoke("library_folder_layout").catch((error: unknown) => { throw failure(error); })),
    setFolderLayout: async (layout) => toLayout(await invoke("library_set_folder_layout", { layout }).catch((error: unknown) => { throw failure(error); })),
    setWatchFolderEnabled: async (id, enabled) => {
      await invoke("library_set_watch_folder_enabled", { id, enabled }).catch((error: unknown) => { throw failure(error); });
    },
    retakeWatchBaseline: async (id) => {
      await invoke("library_retake_watch_baseline", { id }).catch((error: unknown) => { throw failure(error); });
    },
    removeWatchFolder: async (id) => {
      await invoke("library_remove_watch_folder", { id }).catch((error: unknown) => { throw failure(error); });
    },
    addFolder: async () => {
      const response = await invoke("library_add_root").catch((error: unknown) => { throw failure(error); });
      return response === null ? null : toFolder(response);
    },
    scan: async (folderId, confirmMissing = false) => {
      const args = confirmMissing ? { rootId: folderId, confirmMissing: true } : { rootId: folderId };
      await invoke("library_scan", args).catch((error: unknown) => { throw failure(error); });
    },
    removeFolder: async (folderId) => {
      const removed = await invoke("library_remove_root", { rootId: folderId }).catch((error: unknown) => { throw failure(error); });
      return whole(removed, "removal result");
    },
    scanStatus: async () => toStatus(await invoke("library_scan_status").catch((error: unknown) => { throw failure(error); })),
    cancelScan: async () => {
      await invoke("library_scan_cancel").catch((error: unknown) => { throw failure(error); });
    },
    mirrors: async () => {
      const response = await invoke("library_mirrors").catch((error: unknown) => { throw failure(error); });
      if (!Array.isArray(response)) throw new Error("The library returned an unreadable list of exports.");
      return response.map(toMirror);
    },
    playlists: async () => {
      const response = await invoke("library_playlists").catch((error: unknown) => { throw failure(error); });
      if (!Array.isArray(response)) throw new Error("The library returned an unreadable list of playlists.");
      return response.map(toPlaylist);
    },
    trackAnalysis: async (trackId) => {
      const response = await invoke("library_track_analysis", { trackId }).catch((error: unknown) => { throw failure(error); });
      return response === null ? null : toAnalysis(response);
    },
    crates: async () => {
      const response = await invoke("library_crates").catch((error: unknown) => { throw failure(error); });
      if (!Array.isArray(response)) throw new Error("The library returned an unreadable list of crates.");
      return response.map(toCrate);
    },
    createCrate: async (name) => toCrate(await invoke("library_create_crate", { name }).catch((error: unknown) => { throw failure(error); })),
    renameCrate: async (crateId, name) => {
      await invoke("library_rename_crate", { crateId, name }).catch((error: unknown) => { throw failure(error); });
    },
    deleteCrate: async (crateId) => {
      await invoke("library_delete_crate", { crateId }).catch((error: unknown) => { throw failure(error); });
    },
    addToCrate: async (crateId, trackIds) =>
      whole(await invoke("library_add_to_crate", { crateId, trackIds }).catch((error: unknown) => { throw failure(error); }), "crate result"),
    removeFromCrate: async (crateId, trackIds) =>
      whole(await invoke("library_remove_from_crate", { crateId, trackIds }).catch((error: unknown) => { throw failure(error); }), "crate result"),
    moveInCrate: async (crateId, trackIds, beforeId) =>
      ids(await invoke("library_move_in_crate", { crateId, trackIds, beforeId }).catch((error: unknown) => { throw failure(error); }), "crate order"),
    moveTracks: async (trackIds, beforeId, fromSort) => {
      await invoke("library_move_tracks", { trackIds, beforeId, fromSort }).catch((error: unknown) => { throw failure(error); });
    },
    importExport: async () => {
      const response = await invoke("library_import_xml").catch((error: unknown) => { throw failure(error); });
      return response === null ? null : toImport(response);
    },
    rereadMirror: async (sourceId) => toImport(await invoke("library_reread_mirror", { sourceId }).catch((error: unknown) => { throw failure(error); })),
    aliasSuggestions: async (sourceId) => {
      const response = await invoke("library_alias_suggestions", { sourceId }).catch((error: unknown) => { throw failure(error); });
      if (!Array.isArray(response)) throw new Error("The library returned an unreadable list of suggestions.");
      return response.map(toSuggestion);
    },
    addAlias: async (folderId, prefix) => {
      const response = await invoke("library_add_alias", { rootId: folderId, prefix }).catch((error: unknown) => { throw failure(error); });
      return response === true;
    },
    rekordboxDatabase: async () => toRekordboxDatabase(await invoke("rekordbox_db_status").catch((error: unknown) => { throw failure(error); })),
    setRekordboxDatabase: async (enabled) =>
      toRekordboxDatabase(await invoke("rekordbox_db_enable", { enabled }).catch((error: unknown) => { throw failure(error); })),
    readRekordboxDatabase: async () => toImport(await invoke("rekordbox_db_read").catch((error: unknown) => { throw failure(error); })),
    setRekordboxWrite: async (enabled) =>
      toRekordboxDatabase(await invoke("rekordbox_db_enable_write", { enabled }).catch((error: unknown) => { throw failure(error); })),
    addToRekordboxPlaylist: async (playlistId, trackIds) =>
      toRekordboxWrite(await invoke("rekordbox_add_to_playlist", { playlistId, trackIds }).catch((error: unknown) => { throw failure(error); })),
    crateToRekordboxPlaylist: async (crateId, folderId) =>
      toRekordboxWrite(await invoke("rekordbox_crate_to_playlist", { crateId, folderId }).catch((error: unknown) => { throw failure(error); })),
    createRekordboxFolder: async (name, parentId) =>
      toRekordboxFolder(await invoke("rekordbox_create_folder", { name, folderId: parentId }).catch((error: unknown) => { throw failure(error); })),
    createRekordboxPlaylist: async (name, parentId) =>
      toRekordboxFolder(await invoke("rekordbox_create_playlist", { name, folderId: parentId }).catch((error: unknown) => { throw failure(error); })),
    reviewRekordboxMerge: async () => toMergeReview(await invoke("rekordbox_merge_review").catch((error: unknown) => { throw failure(error); })),
    applyRekordboxMerge: async (choices) =>
      toMergeApplied(await invoke("rekordbox_merge_apply", { choices }).catch((error: unknown) => { throw failure(error); })),
    sendNewToRekordbox: async (fileIds) =>
      toRekordboxWrite(await invoke("rekordbox_send_new", { fileIds: fileIds ?? null }).catch((error: unknown) => { throw failure(error); })),
    rekordboxBackups: async () => toBackups(await invoke("rekordbox_backups").catch((error: unknown) => { throw failure(error); })),
    pinRekordboxBackup: async (name, pinned) =>
      toBackups(await invoke("rekordbox_backup_pin", { name, pinned }).catch((error: unknown) => { throw failure(error); })),
    restoreRekordboxBackup: async (name) => toBackup(await invoke("rekordbox_restore", { name }).catch((error: unknown) => { throw failure(error); })),
    rekordboxNewTracks: async () => toNewTracks(await invoke("rekordbox_new_tracks").catch((error: unknown) => { throw failure(error); })),
    sendToRekordbox: async (fileIds) => toSent(await invoke("rekordbox_send", { fileIds }).catch((error: unknown) => { throw failure(error); })),
    hideFromRekordbox: async (fileIds, hidden) =>
      toNewTracks(await invoke("rekordbox_hide", { fileIds, hidden }).catch((error: unknown) => { throw failure(error); })),
    rekordboxCrates: async () => toCrates(await invoke("rekordbox_crates").catch((error: unknown) => { throw failure(error); })),
    setRekordboxCrate: async (crateId, inFile) =>
      toCrates(await invoke("rekordbox_set_crate", { crateId, inFile }).catch((error: unknown) => { throw failure(error); })),
    updateRekordboxFile: async () => toCrates(await invoke("rekordbox_update_file").catch((error: unknown) => { throw failure(error); })),
  };
}

/** Reads the library from the Rust backend. Used only when the backend has a database. */
export class TauriLibraryApi implements LibraryApi {
  private closed = false;
  readonly admin?: LibraryAdmin;

  constructor(
    private readonly invoke: Invoke,
    mode: LibraryMode = { canManageFolders: false, persistent: false },
    private readonly artwork: ((key: number | string) => string) | null = null,
  ) {
    if (mode.canManageFolders) this.admin = adminFor(invoke, mode.persistent);
  }

  private readonly artistCache = new Map<number, Promise<number[]>>();

  artistCovers(artistId: number): Promise<number[]> {
    if (!this.artwork || !Number.isInteger(artistId) || artistId <= 0) return Promise.resolve([]);
    let found = this.artistCache.get(artistId);
    if (!found) {
      found = this.invoke("library_artist_covers", { artistId }).then(
        (ids) => (Array.isArray(ids) ? ids.filter((id): id is number => Number.isInteger(id) && id > 0).slice(0, 4) : []),
        () => [],
      );
      this.artistCache.set(artistId, found);
    }
    return found;
  }

  artworkUrl(releaseId: number): string | null {
    return this.artwork && Number.isInteger(releaseId) && releaseId > 0 ? this.artwork(releaseId) : null;
  }

  coverUrl(of: number | "picked"): string | null {
    if (!this.artwork) return null;
    if (of === "picked") return this.artwork("picked");
    return Number.isInteger(of) && of > 0 ? this.artwork(`track-${of}`) : null;
  }

  read(query: LibraryQuery, start: number, count: number, signal: AbortSignal): Promise<LibraryPage> {
    if (this.closed) return Promise.reject(new Error("The library was closed."));
    if (signal.aborted) return Promise.reject(new DOMException("Request cancelled", "AbortError"));
    if (!Number.isInteger(start) || start < 0 || !Number.isInteger(count) || count < 1 || count > MAX_WINDOW) {
      return Promise.reject(new Error(`Request a non-negative start and 1-${MAX_WINDOW} rows.`));
    }
    const wire = toWire(query);
    return new Promise<LibraryPage>((resolve, reject) => {
      const abort = () => reject(new DOMException("Request cancelled", "AbortError"));
      signal.addEventListener("abort", abort, { once: true });
      this.invoke("library_read", { query: wire, start, count }).then(
        (response) => {
          signal.removeEventListener("abort", abort);
          try {
            resolve(toPage(response));
          } catch (error) {
            reject(error);
          }
        },
        (error: unknown) => {
          signal.removeEventListener("abort", abort);
          reject(error instanceof Error ? error : new Error(typeof error === "string" ? error : "The library could not be read."));
        },
      );
    });
  }

  dispose() {
    this.closed = true;
  }
}
