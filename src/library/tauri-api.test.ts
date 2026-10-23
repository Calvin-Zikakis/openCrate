import { afterEach, describe, expect, it, vi } from "vitest";
import { readLibraryMode, readLibraryStatus, TauriLibraryApi, tauriInvoke, type Invoke } from "./tauri-api";
import { DEFAULT_QUERY } from "./types";

const row = {
  id: 7, kind: "tracks", title: "After Hours 000007", artist: "Studio 001", artistId: 1,
  release: "Between Spaces 0001", releaseId: 10, bpm: 128, musicalKey: "8A", durationMs: 210_600,
  format: "AIFF", trackCount: 1, missing: false,
};
const page = { items: [row], total: 100_000, start: 5 };
const live = () => new AbortController().signal;

afterEach(() => vi.unstubAllGlobals());

describe("reading through the Rust backend", () => {
  it("sends exactly the wire query and maps rows to what the interface shows", async () => {
    const invoke = vi.fn().mockResolvedValue(page);
    const api = new TauriLibraryApi(invoke);
    const result = await api.read({ ...DEFAULT_QUERY, search: "night", format: "all", artistId: 4 }, 5, 100, live());
    expect(invoke).toHaveBeenCalledWith("library_read", {
      query: { view: "tracks", search: "night", sort: "recent", format: null, artistId: 4, releaseId: null, playlistId: null, crateId: null },
      start: 5,
      count: 100,
    });
    expect(result).toEqual({
      total: 100_000,
      start: 5,
      items: [{
        id: 7, kind: "tracks", title: "After Hours 000007", artist: "Studio 001", artistId: 1,
        release: "Between Spaces 0001", releaseId: 10, bpm: 128, musicalKey: "8A",
        duration: 211, format: "AIFF", trackCount: 1, missing: false, color: 2,
      }],
    });
    await api.read({ ...DEFAULT_QUERY, format: "FLAC" }, 0, 1, live());
    expect(invoke.mock.lastCall?.[1]).toMatchObject({ query: { format: "FLAC" } });
  });

  it("carries the missing flag and treats anything but true as present", async () => {
    const api = new TauriLibraryApi(vi.fn().mockResolvedValue({ items: [{ ...row, missing: true }, { ...row, id: 8, missing: "yes" }], total: 2, start: 0 }));
    const items = (await api.read(DEFAULT_QUERY, 0, 2, live())).items;
    expect(items.map((item) => item.missing)).toEqual([true, false]);
  });

  it("keeps a missing tempo and a non-track row without a format", async () => {
    const release = { ...row, kind: "releases", bpm: null, format: null, durationMs: 0, trackCount: 10 };
    const api = new TauriLibraryApi(vi.fn().mockResolvedValue({ items: [release], total: 1, start: 0 }));
    const [item] = (await api.read(DEFAULT_QUERY, 0, 1, live())).items;
    expect(item).toMatchObject({ kind: "releases", bpm: null, format: null, trackCount: 10, duration: 0 });
  });

  it("refuses unreadable replies instead of rendering them", async () => {
    for (const bad of [
      null, "text", { items: "no", total: 1, start: 0 }, { items: [], total: "1", start: 0 },
      { items: [{ ...row, id: "7" }], total: 1, start: 0 },
      { items: [{ ...row, kind: "playlists" }], total: 1, start: 0 },
      { items: [{ ...row, format: "WAV" }], total: 1, start: 0 },
      { items: [{ ...row, bpm: "fast" }], total: 1, start: 0 },
      { items: [{ ...row, title: undefined }], total: 1, start: 0 },
      { items: [null], total: 1, start: 0 },
    ]) {
      const api = new TauriLibraryApi(vi.fn().mockResolvedValue(bad));
      await expect(api.read(DEFAULT_QUERY, 0, 10, live())).rejects.toThrow("unreadable");
    }
  });

  it("shows the backend's own message when a read fails", async () => {
    const text = new TauriLibraryApi(vi.fn().mockRejectedValue("Request a non-negative start and 1-200 rows."));
    await expect(text.read(DEFAULT_QUERY, 0, 10, live())).rejects.toThrow("1-200 rows");
    const error = new TauriLibraryApi(vi.fn().mockRejectedValue(new Error("boom")));
    await expect(error.read(DEFAULT_QUERY, 0, 10, live())).rejects.toThrow("boom");
    const other = new TauriLibraryApi(vi.fn().mockRejectedValue({ odd: true }));
    await expect(other.read(DEFAULT_QUERY, 0, 10, live())).rejects.toThrow("could not be read");
  });

  it("rejects bad windows before calling the backend", async () => {
    const invoke = vi.fn();
    const api = new TauriLibraryApi(invoke);
    for (const [start, count] of [[-1, 10], [0, 0], [0, 201], [1.5, 10], [0, 2.5]]) {
      await expect(api.read(DEFAULT_QUERY, start!, count!, live())).rejects.toThrow("1-200");
    }
    expect(invoke).not.toHaveBeenCalled();
  });

  it("cancels cleanly, before and during a request, and stops after dispose", async () => {
    let finish: (value: unknown) => void = () => {};
    const invoke = vi.fn().mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const api = new TauriLibraryApi(invoke);
    const before = new AbortController();
    before.abort();
    await expect(api.read(DEFAULT_QUERY, 0, 10, before.signal)).rejects.toMatchObject({ name: "AbortError" });
    expect(invoke).not.toHaveBeenCalled();

    const during = new AbortController();
    const pending = api.read(DEFAULT_QUERY, 0, 10, during.signal);
    const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
    during.abort();
    await rejected;
    finish(page); // a late reply is ignored without error

    api.dispose();
    await expect(api.read(DEFAULT_QUERY, 0, 10, live())).rejects.toThrow("closed");
  });
});

describe("choosing the backend", () => {
  it("finds the bridge only inside the desktop app", () => {
    expect(tauriInvoke()).toBeNull();
    vi.stubGlobal("__TAURI_INTERNALS__", { invoke: vi.fn().mockResolvedValue("ok") });
    expect(tauriInvoke()).toBeTypeOf("function");
    vi.stubGlobal("__TAURI_INTERNALS__", { invoke: "nope" });
    expect(tauriInvoke()).toBeNull();
  });

  it("uses the database only when the backend says it has one", async () => {
    const database = { mode: "database", canManageFolders: true, persistent: false };
    expect(await readLibraryMode(vi.fn().mockResolvedValue(database))).toEqual({ canManageFolders: true, persistent: false });
    expect(await readLibraryMode(vi.fn().mockResolvedValue({ ...database, persistent: true }))).toEqual({ canManageFolders: true, persistent: true });
    expect(await readLibraryMode(vi.fn().mockResolvedValue({ ...database, canManageFolders: "yes" }))).toEqual({ canManageFolders: false, persistent: false });
    expect(await readLibraryMode(vi.fn().mockResolvedValue({ mode: "synthetic", canManageFolders: false, persistent: false }))).toBeNull();
    expect(await readLibraryMode(vi.fn().mockResolvedValue("database"))).toBeNull();
    expect(await readLibraryMode(vi.fn().mockRejectedValue(new Error("no such command")))).toBeNull();
  });
});

describe("album art", () => {
  it("is addressed by release id only, and not offered without the desktop app", () => {
    const withArt = new TauriLibraryApi(vi.fn(), { canManageFolders: false, persistent: false }, (id) => `artwork://localhost/${id}`);
    expect(withArt.artworkUrl(5)).toBe("artwork://localhost/5");
    expect(withArt.artworkUrl(0)).toBeNull();
    expect(withArt.artworkUrl(-1)).toBeNull();
    expect(withArt.artworkUrl(1.5)).toBeNull();
    expect(new TauriLibraryApi(vi.fn()).artworkUrl(5)).toBeNull();
  });

  it("asks once per artist for their covers and keeps only sane ids", async () => {
    const invoke = vi.fn().mockResolvedValue([3, 0, -2, 1.5, "4", 7, 8, 9, 10]);
    const api = new TauriLibraryApi(invoke, { canManageFolders: false, persistent: false }, (id) => `artwork://localhost/${id}`);
    expect(await api.artistCovers(1)).toEqual([3, 7, 8, 9]);
    expect(await api.artistCovers(1)).toEqual([3, 7, 8, 9]);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("library_artist_covers", { artistId: 1 });
    expect(await new TauriLibraryApi(vi.fn()).artistCovers(1)).toEqual([]);
    expect(await new TauriLibraryApi(vi.fn().mockRejectedValue("no"), undefined, () => "x").artistCovers(2)).toEqual([]);
  });
});

describe("managing folders", () => {
  const folder = {
    id: 3, label: "Music", path: "/Users/dj/Music", availability: "available", reason: "reachable",
    lastAvailableMs: 5, files: 120, missing: 2,
  };
  const idle = { state: "idle", rootId: null, phase: null, filesSeen: 0, tagsRead: 0, tagsTotal: 0, summary: null, error: null };
  const summary = {
    availability: "available", reason: "reachable", inserted: 4, updated: 1, unchanged: 0, revived: 0,
    markedMissing: 2, heldMissing: 0, tracksCreated: 4, tracksUpdated: 1, tagErrors: 1, cancelled: false,
  };
  const manage = { canManageFolders: true, persistent: true };

  it("is offered only when the backend allows it", () => {
    expect(new TauriLibraryApi(vi.fn()).admin).toBeUndefined();
    expect(new TauriLibraryApi(vi.fn(), { canManageFolders: false, persistent: false }).admin).toBeUndefined();
    expect(new TauriLibraryApi(vi.fn(), manage).admin?.persistent).toBe(true);
  });

  it("lists, adds, scans and cancels through the matching commands", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "library_roots" || command === "library_check_roots") return [folder];
      if (command === "library_add_root") return folder;
      if (command === "library_scan_status") return { ...idle, state: "running", rootId: 3, phase: "reading-tags", filesSeen: 9, tagsRead: 4, tagsTotal: 9 };
      if (command === "library_remove_root") return 7;
      return null;
    });
    const admin = new TauriLibraryApi(invoke, manage).admin!;
    expect(await admin.folders()).toEqual([folder]);
    expect(await admin.addFolder()).toEqual(folder);
    await admin.scan(3);
    expect(invoke).toHaveBeenCalledWith("library_scan", { rootId: 3 });
    await admin.scan(3, true);
    expect(invoke).toHaveBeenCalledWith("library_scan", { rootId: 3, confirmMissing: true });
    expect(await admin.scanStatus()).toMatchObject({ state: "running", phase: "reading-tags", tagsRead: 4, tagsTotal: 9 });
    await admin.cancelScan();
    expect(await admin.checkFolders()).toEqual([folder]);
    await admin.confirmFolder(3);
    expect(invoke).toHaveBeenLastCalledWith("library_confirm_root", { rootId: 3 });
    expect(await admin.removeFolder(3)).toBe(7);
    expect(invoke).toHaveBeenLastCalledWith("library_remove_root", { rootId: 3 });
    expect(invoke.mock.calls.map((call) => call[0])).toEqual([
      "library_roots", "library_add_root", "library_scan", "library_scan", "library_scan_status", "library_scan_cancel",
      "library_check_roots", "library_confirm_root", "library_remove_root",
    ]);
  });

  it("lists, adds, switches and removes download folders through their own commands", async () => {
    const download = { id: 9, label: "Downloads", path: "/Users/dj/Downloads", enabled: true, online: true, lastSeenMs: 4 };
    const invoke = vi.fn(async (command: string) => {
      if (command === "library_watch_folders" || command === "library_check_watch_folders") return [download];
      if (command === "library_add_watch_folder") return download;
      return null;
    });
    const admin = new TauriLibraryApi(invoke, manage).admin!;
    expect(await admin.watchFolders()).toEqual([download]);
    expect(await admin.checkWatchFolders()).toEqual([download]);
    expect(await admin.addWatchFolder()).toEqual(download);
    await admin.setWatchFolderEnabled(9, false);
    expect(invoke).toHaveBeenLastCalledWith("library_set_watch_folder_enabled", { id: 9, enabled: false });
    await admin.removeWatchFolder(9);
    expect(invoke).toHaveBeenLastCalledWith("library_remove_watch_folder", { id: 9 });
    const cancelled = new TauriLibraryApi(vi.fn().mockResolvedValue(null), manage).admin!;
    expect(await cancelled.addWatchFolder()).toBeNull();
    const bad = new TauriLibraryApi(vi.fn().mockResolvedValue([{ id: 1, label: "x", path: "y", enabled: "yes", online: true, lastSeenMs: null }]), manage).admin!;
    await expect(bad.watchFolders()).rejects.toThrow("unreadable download folder");
    const refused = new TauriLibraryApi(vi.fn().mockRejectedValue("That folder is, contains or lies inside another download folder."), manage).admin!;
    await expect(refused.addWatchFolder()).rejects.toThrow("another download folder");
  });

  it("reads a track's cues and grid, or nothing, and refuses unreadable data", async () => {
    const analysis = {
      generation: 2, bpm: 126, musicalKey: "6A",
      cues: [{ kind: 0, number: 1, startSeconds: 12.345678, endSeconds: null, name: "Drop" }],
      tempos: [{ startSeconds: 0.0255, bpm: 126, meter: "4/4", beat: 1 }],
    };
    const invoke = vi.fn().mockResolvedValue(analysis);
    const admin = new TauriLibraryApi(invoke, manage).admin!;
    expect(await admin.trackAnalysis(5)).toEqual(analysis);
    expect(invoke).toHaveBeenCalledWith("library_track_analysis", { trackId: 5 });
    expect(await new TauriLibraryApi(vi.fn().mockResolvedValue(null), manage).admin!.trackAnalysis(5)).toBeNull();
    for (const bad of [{ ...analysis, cues: [{ ...analysis.cues[0], startSeconds: "1" }] }, { ...analysis, tempos: "x" }, "nope"]) {
      await expect(new TauriLibraryApi(vi.fn().mockResolvedValue(bad), manage).admin!.trackAnalysis(5)).rejects.toThrow("unreadable");
    }
  });

  it("asks for an import preview by ids and a name, and refuses an unreadable one", async () => {
    const first = {
          source: "Between Spaces/a.aif", destination: "Studio/Between Spaces/01 - Studio - A.aif", shortened: false, problem: null,
          tags: { fields: [{ name: "Artist", value: "Studio", guessed: false, tidiedFrom: "Studio;" }, { name: "Track", value: null, guessed: true, tidiedFrom: null }], audio: "AIFF 16/44.1", length: "3:27", readError: null, warnings: ["The track number is 0."] },
          becomes: "AIFF",
    };
    const plan = {
      release: "Between Spaces", rootLabel: "Music", ready: 1, blocked: 1,
      files: [first, { source: "Between Spaces/b.zip", destination: null, shortened: false, problem: "Archives are not unpacked yet.", tags: null, becomes: null }],
    };
    const invoke = vi.fn().mockResolvedValue(plan);
    const admin = new TauriLibraryApi(invoke, manage).admin!;
    expect(await admin.planRelease(9, "Between Spaces", 3)).toEqual(plan);
    expect(invoke).toHaveBeenCalledWith("library_plan_release", { folderId: 9, name: "Between Spaces", rootId: 3, options: null });
    const options = { convert: { lossless: "aiff" as const, lossy: null }, tidy: true };
    await admin.planRelease(9, "Between Spaces", 3, options);
    expect(invoke).toHaveBeenLastCalledWith("library_plan_release", { folderId: 9, name: "Between Spaces", rootId: 3, options });
    for (const bad of [{ ...plan, files: "x" }, { ...plan, files: [{ ...first, shortened: "no" }] }, { ...plan, ready: -1 }, { ...plan, files: [{ ...first, tags: { ...first.tags, fields: [{ name: "Artist", value: 5, guessed: false }] } }] }, { ...plan, files: [{ ...first, tags: undefined }] }]) {
      await expect(new TauriLibraryApi(vi.fn().mockResolvedValue(bad), manage).admin!.planRelease(9, "x", 3)).rejects.toThrow("unreadable");
    }
    await expect(new TauriLibraryApi(vi.fn().mockRejectedValue("That release is no longer in the download folder."), manage).admin!.planRelease(9, "x", 3)).rejects.toThrow("no longer");
  });

  it("reads the downloads snapshot and refuses an unreadable one", async () => {
    const release = {
      folderId: 9, folderLabel: "Downloads", name: "Between Spaces", loose: false, state: "ready", quietSeconds: null,
      audioFiles: 8, archives: 0, alreadyThere: false, imported: true, newestModifiedMs: 5,
    };
    const folder = { folderId: 9, label: "Downloads", state: "watching", unreadableDirs: 0, truncated: false };
    const good = new TauriLibraryApi(vi.fn().mockResolvedValue({ folders: [folder], releases: [release] }), manage).admin!;
    expect(await good.downloadReleases()).toEqual({ folders: [folder], releases: [release] });
    for (const bad of [
      { folders: [folder], releases: [{ ...release, state: "done" }] },
      { folders: [{ ...folder, state: "lost" }], releases: [] },
      { folders: [], releases: [{ ...release, audioFiles: -1 }] },
      { folders: [], releases: [{ ...release, imported: undefined }] },
      "nope",
    ]) {
      const admin = new TauriLibraryApi(vi.fn().mockResolvedValue(bad), manage).admin!;
      await expect(admin.downloadReleases()).rejects.toThrow("unreadable");
    }
  });

  it("reads what an import did and refuses an unreadable answer", async () => {
    const done = { rootId: 3, rootLabel: "Music", filed: 2, skipped: [{ source: "R/x.zip", problem: "Archives are not unpacked yet." }] };
    const invoke = vi.fn().mockResolvedValue(done);
    expect(await new TauriLibraryApi(invoke, manage).admin!.importRelease(9, "R", 3)).toEqual(done);
    expect(invoke).toHaveBeenLastCalledWith("library_import_release", { folderId: 9, name: "R", rootId: 3, options: null });
    for (const bad of [{ ...done, filed: -1 }, { ...done, skipped: "x" }, { ...done, skipped: [{ source: 1 }] }, null]) {
      await expect(new TauriLibraryApi(vi.fn().mockResolvedValue(bad), manage).admin!.importRelease(9, "R", 3)).rejects.toThrow("unreadable");
    }
  });

  it("asks MusicBrainz only through the lookup command and reads its suggestions", async () => {
    const answer = {
      found: { title: "Between Spaces", artist: "Studio 001", year: 2019, label: "Test Label", tracks: 2, url: "https://musicbrainz.org/release/x" },
      files: [{ source: "R/a.flac", matchedBy: "title", changes: [{ field: "Track", from: null, to: "2" }] }],
      note: null,
    };
    const invoke = vi.fn().mockResolvedValue(answer);
    const admin = new TauriLibraryApi(invoke, manage).admin!;
    expect(await admin.lookupRelease(9, "R", 3)).toEqual(answer);
    expect(invoke).toHaveBeenLastCalledWith("library_lookup_release", { folderId: 9, name: "R", rootId: 3, options: null });
    const none = { found: null, files: [], note: "MusicBrainz has no release that matches." };
    expect(await new TauriLibraryApi(vi.fn().mockResolvedValue(none), manage).admin!.lookupRelease(9, "R", 3)).toEqual(none);
    for (const bad of [{ ...answer, files: "x" }, { ...answer, found: { ...answer.found, tracks: -1 } }, { ...answer, files: [{ source: "a", changes: [{ field: 1, to: "x" }] }] }]) {
      await expect(new TauriLibraryApi(vi.fn().mockResolvedValue(bad), manage).admin!.lookupRelease(9, "R", 3)).rejects.toThrow("unreadable");
    }
  });

  it("reads the library check and refuses an unreadable one", async () => {
    const health = {
      files: 5,
      untidy: [{ rootLabel: "Music", folder: "Studio 001;", suggested: "Studio 001", files: 1 }],
      duplicates: [{ artist: "Studio 001", title: "Soft Focus", files: ["a/x.aif", "b/x.aif"] }],
    };
    const invoke = vi.fn().mockResolvedValue(health);
    expect(await new TauriLibraryApi(invoke, manage).admin!.checkLibrary()).toEqual(health);
    expect(invoke).toHaveBeenLastCalledWith("library_check");
    for (const bad of [{ ...health, files: -1 }, { ...health, untidy: [{ folder: 1 }] }, { ...health, duplicates: [{ artist: "a", title: "t", files: "x" }] }]) {
      await expect(new TauriLibraryApi(vi.fn().mockResolvedValue(bad), manage).admin!.checkLibrary()).rejects.toThrow("unreadable");
    }
  });

  it("reads and saves the folder layout, and refuses an unreadable answer", async () => {
    const layout = { layout: "{release}/{title}", example: "Between Spaces/Night Drive.aif", canConvert: true, presets: [{ name: "Album / Track", layout: "{release}/{title}" }] };
    const invoke = vi.fn().mockResolvedValue(layout);
    const admin = new TauriLibraryApi(invoke, manage).admin!;
    expect(await admin.folderLayout()).toEqual(layout);
    expect(await admin.setFolderLayout("{release}/{title}")).toEqual(layout);
    expect(invoke).toHaveBeenLastCalledWith("library_set_folder_layout", { layout: "{release}/{title}" });
    for (const bad of [{ ...layout, canConvert: "yes" }, { ...layout, presets: [{ name: 1 }] }, { ...layout, example: null }]) {
      await expect(new TauriLibraryApi(vi.fn().mockResolvedValue(bad), manage).admin!.folderLayout()).rejects.toThrow("unreadable");
    }
    await expect(new TauriLibraryApi(vi.fn().mockRejectedValue("That layout cannot be used: unknown template field `x`."), manage).admin!.setFolderLayout("{x}")).rejects.toThrow("cannot be used");
  });

  it("treats a cancelled picker as no folder and reads a finished scan", async () => {
    const none = new TauriLibraryApi(vi.fn().mockResolvedValue(null), manage).admin!;
    expect(await none.addFolder()).toBeNull();
    const done = new TauriLibraryApi(vi.fn().mockResolvedValue({ ...idle, state: "finished", rootId: 3, summary }), manage).admin!;
    expect((await done.scanStatus()).summary).toEqual(summary);
  });

  it("refuses unreadable replies and passes backend messages on", async () => {
    const reply = (value: unknown) => new TauriLibraryApi(vi.fn().mockResolvedValue(value), manage).admin!;
    await expect(reply("no").folders()).rejects.toThrow("unreadable");
    await expect(reply([{ ...folder, files: -1 }]).folders()).rejects.toThrow("unreadable");
    await expect(reply([{ ...folder, label: 5 }]).folders()).rejects.toThrow("unreadable");
    await expect(reply({ ...folder, id: "3" }).addFolder()).rejects.toThrow("unreadable");
    await expect(reply({ ...idle, state: "paused" }).scanStatus()).rejects.toThrow("unreadable");
    await expect(reply({ ...idle, state: "finished", summary: { ...summary, inserted: "4" } }).scanStatus()).rejects.toThrow("unreadable");
    await expect(reply(null).scanStatus()).rejects.toThrow("unreadable");
    const refused = new TauriLibraryApi(vi.fn().mockRejectedValue("A scan is already running."), manage).admin!;
    await expect(refused.scan(3)).rejects.toThrow("already running");
    await expect(refused.folders()).rejects.toThrow("already running");
  });
});

describe("rekordbox sync", () => {
  const counts = { matched: 8977, unmapped: 31, notIndexed: 0, ambiguous: 0, unrecognized: 0, playlists: 135, unresolvedReferences: 2 };
  const mirror = {
    sourceId: 3, kind: "xml", path: "/exports/collection.xml", generation: 2, entries: 9008, fileMtimeMs: 10,
    lastReadMs: 20, lastAttemptMs: 30, lastError: null, counts,
  };
  const imported = { sourceId: 3, generation: 2, unchanged: false, entries: 9008, matched: 8977, derivedSet: 8977 };
  const suggestion = { rootId: 7, rootLabel: "Music", prefix: "/G:/Library", entries: 8977 };
  const admin = (invoke: Invoke) => new TauriLibraryApi(invoke, { canManageFolders: true, persistent: true }).admin!;

  it("uses the matching commands and argument names", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "library_mirrors") return [mirror];
      if (command === "library_alias_suggestions") return [suggestion];
      if (command === "library_add_alias") return true;
      if (command === "library_import_xml" || command === "library_reread_mirror") return imported;
      return null;
    });
    const sync = admin(invoke);
    expect(await sync.mirrors()).toEqual([mirror]);
    expect(await sync.importExport()).toEqual(imported);
    expect(await sync.rereadMirror(3)).toEqual(imported);
    expect(await sync.aliasSuggestions(3)).toEqual([suggestion]);
    expect(await sync.addAlias(7, "/G:/Library")).toBe(true);
    expect(invoke.mock.calls).toEqual([
      ["library_mirrors"],
      ["library_import_xml"],
      ["library_reread_mirror", { sourceId: 3 }],
      ["library_alias_suggestions", { sourceId: 3 }],
      ["library_add_alias", { rootId: 7, prefix: "/G:/Library" }],
    ]);
  });

  it("treats a cancelled file picker as no import and a failed read as an error", async () => {
    expect(await admin(vi.fn().mockResolvedValue(null)).importExport()).toBeNull();
    const failing = admin(vi.fn().mockRejectedValue("invalid XML syntax"));
    await expect(failing.importExport()).rejects.toThrow("invalid XML syntax");
    await expect(failing.rereadMirror(3)).rejects.toThrow("invalid XML syntax");
    await expect(failing.addAlias(1, "x")).rejects.toThrow("invalid XML syntax");
  });

  it("refuses unreadable replies", async () => {
    const reply = (value: unknown) => admin(vi.fn().mockResolvedValue(value));
    await expect(reply("no").mirrors()).rejects.toThrow("unreadable");
    await expect(reply([{ ...mirror, counts: { ...counts, matched: "many" } }]).mirrors()).rejects.toThrow("unreadable");
    await expect(reply([{ ...mirror, counts: null }]).mirrors()).rejects.toThrow("unreadable");
    await expect(reply([{ ...mirror, lastError: 5 }]).mirrors()).rejects.toThrow("unreadable");
    await expect(reply([{ ...mirror, kind: "itunes" }]).mirrors()).rejects.toThrow("unreadable");
    await expect(reply({ ...imported, unchanged: "no" }).importExport()).rejects.toThrow("unreadable");
    await expect(reply({ ...imported, entries: -1 }).rereadMirror(3)).rejects.toThrow("unreadable");
    await expect(reply("no").aliasSuggestions(3)).rejects.toThrow("unreadable");
    await expect(reply([{ ...suggestion, prefix: 5 }]).aliasSuggestions(3)).rejects.toThrow("unreadable");
    expect(await reply("yes").addAlias(1, "/x")).toBe(false);
  });
});

describe("playlists", () => {
  const folder = { id: 1, parentId: null, isFolder: true, name: "ROOT", position: 0, entries: 0, inLibrary: 0 };
  const list = { id: 3, parentId: 1, isFolder: false, name: "Friday", position: 0, entries: 40, inLibrary: 38 };
  const admin = (invoke: Invoke) => new TauriLibraryApi(invoke, { canManageFolders: true, persistent: true }).admin!;

  it("reads the tree and sends the playlist id in a query", async () => {
    const invoke = vi.fn(async (command: string) => (command === "library_playlists" ? [folder, list] : { items: [], total: 0, start: 0 }));
    expect(await admin(invoke).playlists()).toEqual([folder, list]);
    const api = new TauriLibraryApi(invoke);
    await api.read({ ...DEFAULT_QUERY, sort: "playlist", playlistId: 3 }, 0, 10, live());
    expect(invoke).toHaveBeenLastCalledWith("library_read", {
      query: { view: "tracks", search: "", sort: "playlist", format: null, artistId: null, releaseId: null, playlistId: 3, crateId: null },
      start: 0,
      count: 10,
    });
  });

  it("refuses unreadable playlist data", async () => {
    const reply = (value: unknown) => admin(vi.fn().mockResolvedValue(value));
    await expect(reply("no").playlists()).rejects.toThrow("unreadable");
    await expect(reply([{ ...list, isFolder: "no" }]).playlists()).rejects.toThrow("unreadable");
    await expect(reply([{ ...list, name: 7 }]).playlists()).rejects.toThrow("unreadable");
    await expect(reply([{ ...list, entries: -1 }]).playlists()).rejects.toThrow("unreadable");
    await expect(reply([{ ...list, parentId: "1" }]).playlists()).rejects.toThrow("unreadable");
  });
});

describe("a library that could not be opened", () => {
  it("passes the reason on so the window can show it", async () => {
    const failed = { mode: "synthetic", canManageFolders: false, persistent: false, error: "The library folder could not be opened: already owned" };
    expect(await readLibraryStatus(vi.fn().mockResolvedValue(failed))).toEqual({ mode: null, error: failed.error });
    expect(await readLibraryMode(vi.fn().mockResolvedValue(failed))).toBeNull();
    const fine = { mode: "database", canManageFolders: true, persistent: true, error: null };
    expect(await readLibraryStatus(vi.fn().mockResolvedValue(fine))).toEqual({ mode: { canManageFolders: true, persistent: true }, error: null });
    expect(await readLibraryStatus(vi.fn().mockResolvedValue({ ...fine, error: 5 }))).toMatchObject({ error: null });
    expect(await readLibraryStatus(vi.fn().mockResolvedValue("junk"))).toEqual({ mode: null, error: null });
    expect(await readLibraryStatus(vi.fn().mockRejectedValue(new Error("x")))).toEqual({ mode: null, error: null });
  });
});


describe("rekordbox database", () => {
  const status = {
    installed: true, appVersion: "7.2.19.0342", running: false, enabled: true,
    confirmedVersion: "7.2.19.0342", needsConfirmation: false,
  };
  const backup = { name: "20261023-120000-read", takenMs: 5, reason: "read", pinned: false, appVersion: "7.2.19.0342", tracks: 40, playlists: 3 };
  const imported = { sourceId: 4, generation: 1, unchanged: false, entries: 40, matched: 0, derivedSet: 0 };
  const admin = (invoke: Invoke) => new TauriLibraryApi(invoke, { canManageFolders: true, persistent: true }).admin!;

  it("uses the matching commands and argument names", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "rekordbox_db_status" || command === "rekordbox_db_enable") return status;
      if (command === "rekordbox_db_read") return imported;
      if (command === "rekordbox_backups" || command === "rekordbox_backup_pin") return [backup];
      if (command === "rekordbox_restore") return { ...backup, reason: "before-restore", pinned: true };
      return null;
    });
    const db = admin(invoke);
    expect(await db.rekordboxDatabase()).toEqual(status);
    expect(await db.setRekordboxDatabase(true)).toEqual(status);
    expect(await db.readRekordboxDatabase()).toEqual(imported);
    expect(await db.rekordboxBackups()).toEqual([backup]);
    expect(await db.pinRekordboxBackup(backup.name, true)).toEqual([backup]);
    expect((await db.restoreRekordboxBackup(backup.name)).reason).toBe("before-restore");
    expect(invoke.mock.calls).toEqual([
      ["rekordbox_db_status"],
      ["rekordbox_db_enable", { enabled: true }],
      ["rekordbox_db_read"],
      ["rekordbox_backups"],
      ["rekordbox_backup_pin", { name: backup.name, pinned: true }],
      ["rekordbox_restore", { name: backup.name }],
    ]);
  });

  it("refuses unreadable replies and passes refusals on", async () => {
    const reply = (value: unknown) => admin(vi.fn().mockResolvedValue(value));
    await expect(reply({ ...status, running: "no" }).rekordboxDatabase()).rejects.toThrow("unreadable");
    await expect(reply({ ...status, appVersion: 7 }).rekordboxDatabase()).rejects.toThrow("unreadable");
    await expect(reply([{ ...backup, tracks: -1 }]).rekordboxBackups()).rejects.toThrow("unreadable");
    await expect(reply("no").rekordboxBackups()).rejects.toThrow("unreadable");
    const refused = admin(vi.fn().mockRejectedValue("rekordbox is running. Quit it first."));
    await expect(refused.readRekordboxDatabase()).rejects.toThrow("rekordbox is running");
  });
});
