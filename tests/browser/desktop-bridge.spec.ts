import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

// A stand-in for the Tauri bridge, so the real UI runs against the Rust command contract
// without a native window. It records every call.
type Mode = "database" | "managed" | "synthetic" | "broken";

async function stubBridge(page: Page, mode: Mode, failReads = false) {
  await page.addInitScript(
    ({ mode, failReads }) => {
      const calls: { command: string; args?: Record<string, any> }[] = [];
      (window as any).__calls = calls;
      const folders: Record<string, any>[] = [];
      const downloads: Record<string, any>[] = [];
      const crateList: { id: number; name: string; order: number[] }[] = [];
      let libraryOrder: number[] | null = null;
      const imported = new Set<string>();
      // A stand-in player whose position moves in real time while playing.
      const player = { trackId: null as number | null, playing: false, base: 0, since: 0, volume: 1 };
      const duration = () => ((window as any).__shortTracks ? 1500 : 200_000);
      const at = () => Math.min(duration(), player.base + (player.playing ? Date.now() - player.since : 0));
      const playerState = () => {
        const position = at();
        const ended = player.trackId !== null && position >= duration();
        if (ended) { player.base = position; player.playing = false; }
        return { trackId: player.trackId, playing: player.playing, positionMs: Math.round(position), durationMs: player.trackId === null ? null : duration(), ended, error: null };
      };
      let layout = "{folder_artist}/{release}/{track:02} - {artist} - {title}";
      const layoutOut = () => ({
        layout, canConvert: !(window as any).__noFfmpeg,
        example: layout.startsWith("{release}") ? "Between Spaces/01 - Studio 001 - Night Drive.aif" : "Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif",
        presets: [
          { name: "Artist / Album / Track", layout: "{folder_artist}/{release}/{track:02} - {artist} - {title}" },
          { name: "Album / Track", layout: "{release}/{track:02} - {artist} - {title}" },
        ],
      });
      const crateOut = (crate: { id: number; name: string; order: number[] }) => ({ id: crate.id, name: crate.name, tracks: crate.order.length });
      const crateById = (id: unknown) => {
        const found = crateList.find((crate) => crate.id === id);
        if (!found) throw `no crate with id ${String(id)}`;
        return found;
      };
      let statusCalls = 0;
      let scanStarted = false;
      let mirror: Record<string, any> | null = null;
      let aliasAdded = false;
      // rekordbox's own library, read directly once the owner confirms the warning.
      // Tests set __rbEnabled in a later init script, so it is read when first asked for.
      let rekordbox: { enabled: boolean; confirmedVersion: string | null } | null = null;
      const rb = () => (rekordbox ??= (window as any).__rbEnabled
        ? { enabled: true, confirmedVersion: "7.2.19.0342" } : { enabled: false, confirmedVersion: null });
      const rbVersion = () => ((window as any).__rbVersion ?? "7.2.19.0342") as string;
      // Read when first asked, so a test's later init script can set the flag.
      let rbWriteSet: { version: string | null } | null = null;
      const rbWrite = () => (rbWriteSet ??= { version: (window as any).__rbWriting ? "7.2.19.0342" : null }).version;
      const setRbWrite = (version: string | null) => { rbWriteSet = { version }; };
      const rbState = () => ({
        installed: true, appVersion: rbVersion(), running: !!(window as any).__rbRunning, enabled: rb().enabled,
        confirmedVersion: rb().confirmedVersion, needsConfirmation: rb().enabled && rb().confirmedVersion !== rbVersion(),
        writeEnabled: rbWrite() !== null, writeConfirmedVersion: rbWrite(), writeNeedsConfirmation: rbWrite() !== null && rbWrite() !== rbVersion(),
        writeTested: rbVersion().startsWith("7.2."),
      });
      const rbBackups: Record<string, any>[] = [];
      const rbCrates = new Set<number>();
      const rbBackup = (name: string, reason: string, pinned: boolean) =>
        ({ name, takenMs: Date.UTC(2026, 9, 23, 12, rbBackups.length), reason, pinned, appVersion: rbVersion(), tracks: 40, playlists: 3 });
      const counts = (matched: number, unmapped: number) => ({ matched, unmapped, notIndexed: 0, ambiguous: 0, unrecognized: 0, playlists: 135, unresolvedReferences: 2 });
      const makeMirror = (generation: number, matched: number, unmapped: number) => ({
        sourceId: 3, kind: "xml", path: "/Users/dj/exports/collection.xml", generation, entries: 9008, fileMtimeMs: 1,
        lastReadMs: Date.now(), lastAttemptMs: Date.now(), lastError: null, counts: counts(matched, unmapped),
        changedSinceRead: Boolean((window as any).__exportChanged),
      });
      const summary = () => ({
        availability: "available", reason: "reachable", inserted: 20, updated: 0, unchanged: 0, revived: 0,
        markedMissing: 1, heldMissing: (window as any).__heldMissing ? 12 : 0, tracksCreated: 19, tracksUpdated: 0, tagErrors: 1, cancelled: false,
      });
      const status = () => {
        const base = { state: "idle", rootId: null, phase: null, filesSeen: 0, tagsRead: 0, tagsTotal: 0, summary: null, error: null };
        if (!scanStarted) return base;
        statusCalls += 1;
        if (statusCalls < 3) return { ...base, state: "running", rootId: 7, phase: "reading-tags", filesSeen: 20, tagsRead: 5 * statusCalls, tagsTotal: 20 };
        folders[0] = { ...folders[0], files: 20 };
        return { ...base, state: "finished", rootId: 7, tagsRead: 20, summary: summary() };
      };
      (window as any).__TAURI_INTERNALS__ = {
        convertFileSrc: (path: string, protocol: string) => `/__${protocol}/${encodeURIComponent(path)}`,
        invoke: async (command: string, args?: Record<string, any>) => {
          // As serde does in the host: a query's format is one of the formats or null.
          if (args?.query && !(args.query.format === null || ["AIFF", "MP3", "FLAC"].includes(args.query.format))) {
            throw `invalid args \`query\` for command \`${command}\`: unknown variant \`${args.query.format}\``;
          }
          calls.push({ command, args });
          switch (command) {
            case "library_mode":
              if (mode === "broken") throw new Error("no such command");
              if (mode === "synthetic") return { mode: "synthetic", canManageFolders: false, persistent: false, error: (window as any).__startupError ?? null };
              return { mode: "database", canManageFolders: mode === "managed", persistent: Boolean((window as any).__persistent), error: null };
            case "library_roots":
              return folders;
            case "library_check_roots": {
              const state = (window as any).__folderState as string | undefined;
              return folders.map((folder) => (state ? { ...folder, availability: state, reason: state === "unavailable" ? "not found" : "directory identity changed" } : folder));
            }
            case "library_watch_folders":
            case "library_check_watch_folders":
              return downloads;
            case "library_move_tracks": {
              const current = libraryOrder ?? Array.from({ length: 1234 }, (_, i) => i + 1);
              const moving = args?.trackIds as number[];
              const picked = current.filter((id) => moving.includes(id));
              const rest = current.filter((id) => !moving.includes(id));
              const at = args?.beforeId == null || rest.indexOf(args.beforeId) < 0 ? rest.length : rest.indexOf(args.beforeId);
              rest.splice(at, 0, ...picked);
              libraryOrder = rest;
              return null;
            }
            case "library_crates":
              return crateList.map(crateOut);
            case "library_create_crate": {
              const name = String(args?.name ?? "").trim();
              if (!name) throw "A crate needs a name of 1 to 100 characters.";
              const made = { id: crateList.length + 1, name, order: [] as number[] };
              crateList.push(made);
              return crateOut(made);
            }
            case "library_rename_crate":
              crateById(args?.crateId).name = String(args?.name).trim();
              return null;
            case "library_delete_crate":
              crateList.splice(crateList.indexOf(crateById(args?.crateId)), 1);
              return null;
            case "library_add_to_crate": {
              const target = crateById(args?.crateId);
              const fresh = (args?.trackIds as number[]).filter((id) => !target.order.includes(id));
              target.order.push(...fresh);
              return fresh.length;
            }
            case "library_remove_from_crate": {
              const target = crateById(args?.crateId);
              const before = target.order.length;
              target.order = target.order.filter((id) => !(args?.trackIds as number[]).includes(id));
              return before - target.order.length;
            }
            case "library_move_in_crate": {
              const target = crateById(args?.crateId);
              const moving = args?.trackIds as number[];
              const picked = target.order.filter((id) => moving.includes(id));
              const rest = target.order.filter((id) => !moving.includes(id));
              const at = args?.beforeId == null ? rest.length : Math.max(0, rest.indexOf(args.beforeId));
              const position = args?.beforeId != null && rest.indexOf(args.beforeId) < 0 ? rest.length : at;
              rest.splice(position, 0, ...picked);
              target.order = rest;
              return rest;
            }
            case "library_artist_covers": {
              const counts: Record<number, number> = { 1: 1, 2: 2, 3: 3, 4: 4 };
              return [11, 12, 13, 14, 15].slice(0, counts[args?.artistId as number] ?? 0);
            }
            case "library_track_analysis":
              if ((window as any).__noAnalysis) return null;
              return {
                generation: 1, bpm: 128, musicalKey: "8A",
                cues: [
                  { kind: 0, number: 1, startSeconds: 12.345678, endSeconds: null, name: "Drop" },
                  { kind: 4, number: -1, startSeconds: 60.25, endSeconds: 64.5, name: "" },
                ],
                tempos: [{ startSeconds: 0.0255, bpm: 128, meter: "4/4", beat: 1 }],
              };
            case "library_download_releases": {
              const rel = (name: string, state: string, extra: Record<string, any> = {}) => ({
                folderId: 9, folderLabel: "Downloads", name, loose: false, state, quietSeconds: null, audioFiles: 8, archives: 0,
                alreadyThere: false, newestModifiedMs: 5, imported: imported.has(name), ...extra,
              });
              return {
                folders: downloads.map((folder) => ({ folderId: folder.id, label: folder.label, state: (window as any).__watchState ?? "watching", unreadableDirs: 0, truncated: false })),
                releases: downloads.length === 0 ? [] : [
                  rel("Between Spaces", "ready"),
                  rel("Night Drive EP", "downloading", { audioFiles: 3 }),
                  rel("Bought on Bandcamp.zip", "settling", { loose: true, audioFiles: 0, archives: 1, quietSeconds: 4 }),
                  rel("Old Favourites", "ready", { alreadyThere: true, audioFiles: 12 }),
                  ...((window as any).__twoReady ? [rel("Second Light", "ready", { audioFiles: 2 })] : []),
                  ...((window as any).__cleanReady ? [rel("Clean Single", "ready", { audioFiles: 1 })] : []),
                  ...Array.from({ length: (window as any).__manyImported ?? 0 }, (_, n) => rel(`Earlier ${n + 1}`, "ready", { imported: true, audioFiles: 1 })),
                ],
              };
            }
            case "library_lookup_release":
              if (args?.name === "Second Light") throw "MusicBrainz is busy right now. Try again in a minute.";
              return {
                found: { source: "MusicBrainz", title: "Between Spaces", artist: "Studio 001", year: 2019, label: "Test Label", genre: null, tracks: 2, url: "https://musicbrainz.org/release/0b1d" },
                note: null,
                files: [
                  { source: `${args?.name}/01.flac`, matchedBy: "track number and title", changes: [{ field: "Title", from: "Studio 001 - Night Drive.mp3", to: "Night Drive" }, { field: "Year", from: null, to: "2019" }] },
                  { source: `${args?.name}/03.flac`, matchedBy: "title", changes: [{ field: "Label", from: null, to: "Test Label" }] },
                ],
              };
            case "library_waveform":
              return Array.from({ length: 800 * 3 }, (_, i) => Math.round((i % 3 === 0 ? 127 : 60) + (i % 3 === 0 ? 120 : 50) * Math.sin(i / 27)));
            case "library_waveform_detail": {
              // 200 seconds at 300 slices a second, as the bytes the host sends: a kick on every
              // beat, hats between.
              const bytes = new Uint8Array(2 + 60_000 * 3);
              bytes[0] = 300 & 0xff;
              bytes[1] = 300 >> 8;
              for (let slice = 0; slice < 60_000; slice += 1) {
                const beat = slice % 150;
                bytes.set([beat < 24 ? 230 : 40, 90 + (slice % 13) * 5, beat > 60 && beat < 84 ? 160 : 20], 2 + slice * 3);
              }
              return bytes.buffer;
            }
            case "player_load":
              if (args?.trackId === 3) throw "This track's file cannot be found. Its music folder may be unplugged, or the file was moved.";
              player.trackId = args?.trackId; player.base = args?.startMs ?? 0; player.since = Date.now(); player.playing = args?.play !== false;
              return null;
            case "player_control": {
              const action = args?.action;
              if (action === "status" && (window as any).__staleStatus > 0) {
                // An answer from before the last load: the previous track, ended.
                (window as any).__staleStatus -= 1;
                return { trackId: 99, playing: false, positionMs: 1500, durationMs: 1500, ended: true, error: null };
              }
              if (action === "play") { if (at() >= duration()) player.base = 0; player.since = Date.now(); player.playing = true; }
              if (action === "pause") { player.base = at(); player.playing = false; }
              if (action === "seek") {
                // A slow seek: a status asked before it lands still reports the old position.
                if ((window as any).__slowSeek) await new Promise((resolve) => setTimeout(resolve, 600));
                player.base = args?.positionMs; player.since = Date.now();
              }
              if (action === "volume") player.volume = args?.volume;
              if (action === "stop") { player.trackId = null; player.playing = false; player.base = 0; }
              (window as any).__volume = player.volume;
              return playerState();
            }
            case "lookup_keys":
            case "lookup_set_key": {
              const store = ((window as any).__keys ??= { discogs: null, acoustid: null }) as Record<string, string | null>;
              let note: string | null = null;
              if (command === "lookup_set_key") {
                const value = typeof args?.value === "string" && args.value.trim() ? args.value.trim() : null;
                if (args?.serviceName === "discogs" && value === "bad") throw "Discogs did not accept that token. Copy it again from Discogs, Settings, Developers.";
                store[args?.serviceName] = value;
                note = args?.serviceName === "discogs" && value ? "Discogs accepted the token." : null;
              }
              const key = (value: string | null) => ({ saved: value !== null, ending: value === null ? null : value.slice(-4) });
              return { discogs: key(store.discogs ?? null), acoustid: key(store.acoustid ?? null), note };
            }
            case "library_duplicate_copies":
              return {
                suggested: 11, note: null,
                copies: [
                  { fileId: 11, path: "Studio 001/Between Spaces/02 - Soft Focus.aif", format: "AIFF", inRekordbox: true, analysed: true, cues: 4, playlists: 2 },
                  { fileId: 12, path: "Studio 001;Guest/Between Spaces/02 - Soft Focus.aif", format: "MP3, 320 kbps", inRekordbox: true, analysed: true, cues: 0, playlists: 1 },
                ],
              };
            case "library_resolve_duplicate":
              if (!(window as any).__rbWriting) throw "rekordbox has a copy that would go, so it has to be told. Writing to rekordbox is off. Turn it on in Settings.";
              (window as any).__resolved = { keep: args?.keep, drop: args?.drop };
              return { archived: 1, merged: 1, archive: "/Users/dj/Music/openCrate archive/Music" };
            case "library_genres": {
              const renamed = (window as any).__genresMerged;
              return renamed
                ? { genres: [{ name: "Deep House", tracks: 5 }, { name: "Techno", tracks: 2 }], groups: [] }
                : {
                  genres: [{ name: "Deep House", tracks: 3 }, { name: "deep-house", tracks: 2 }, { name: "Techno", tracks: 2 }],
                  groups: [{ suggested: "Deep House", members: ["Deep House", "deep-house"], tracks: 5 }],
                };
            }
            case "library_rename_genres":
              (window as any).__genresMerged = { from: args?.from, to: args?.to };
              (window as any).__renamedGenres = true;
              return { files: 2, left: [] };
            case "library_track_names":
              return [
                { trackId: 1, title: "01 - Night Drive", artist: "Studio 001" },
                { trackId: 2, title: "Soft Focus", artist: "Studio 001" },
                { trackId: 3, title: "Studio 001 - Late Train", artist: "Unknown Artist" },
                ...((window as any).__moreNames ? [
                  { trackId: 4, title: "MAINTAIN", artist: "Studio 001" },
                  { trackId: 5, title: "01 - Night Drive", artist: "Studio 001" },
                ] : []),
              ];
            case "library_edit_each":
              (window as any).__editEach = args?.edits;
              return { files: (args?.edits ?? []).length, left: [] };
            case "library_export":
              (window as any).__exports = [...((window as any).__exports ?? []), { query: args?.query, format: args?.format, name: args?.name }];
              return args?.format === "csv" ? null : { path: "/Users/dj/Desktop/Warm-up.m3u8", tracks: 3, withoutFile: 1 };
            case "library_releases_without_art":
              return ((window as any).__artDone ?? []).includes(2) ? [
                { releaseId: 3, artist: "Studio 001", title: "No Light", files: 2, folderImage: false },
              ] : [
                { releaseId: 2, artist: "Studio 001", title: "Between Spaces", files: 2, folderImage: true },
                { releaseId: 3, artist: "Studio 001", title: "No Light", files: 2, folderImage: false },
              ];
            case "library_set_release_art":
              if (args?.source === "online") throw "The Cover Art Archive has no cover for \"No Light\" by Studio 001.";
              if (args?.source === "choose") return null;
              (window as any).__artDone = [...((window as any).__artDone ?? []), args?.releaseId];
              return { files: 2, left: [] };
            case "library_remove_release_art":
              return { files: 2, left: [] };
            case "library_merge_crates": {
              const into = crateById(args?.into);
              let added = 0;
              for (const id of args?.from ?? []) { for (const track of crateById(id).order) if (!into.order.includes(track)) { into.order.push(track); added += 1; } if (args?.remove) crateList.splice(crateList.indexOf(crateById(id)), 1); }
              return added;
            }
            case "library_sort_crate":
              crateById(args?.crateId).order.sort((a: number, b: number) => a - b);
              (window as any).__sorted = { crateId: args?.crateId, sort: args?.sort };
              return crateById(args?.crateId).order;
            case "library_compare_crates": {
              const name = (id: number) => ({ trackId: id, title: `Rust Track ${String(id).padStart(4, "0")}`, artist: "Backend Artist" });
              const a = crateById(args?.first).order; const b = crateById(args?.second).order;
              return { both: a.filter((id: number) => b.includes(id)).map(name), onlyFirst: a.filter((id: number) => !b.includes(id)).map(name), onlySecond: b.filter((id: number) => !a.includes(id)).map(name) };
            }
            case "library_custom_fields": {
              const store = ((window as any).__custom ??= []) as { trackId: number; name: string; value: string }[];
              const ids = (args?.trackIds ?? []) as number[];
              return { fields: store.filter((field) => ids.includes(field.trackId)), names: [...new Set(store.map((field) => field.name))] };
            }
            case "library_set_custom_field": {
              const store = ((window as any).__custom ??= []) as { trackId: number; name: string; value: string }[];
              const name = String(args?.name).trim(); const value = String(args?.value).trim();
              if (!name) throw "a custom field needs a short, plain name";
              for (const trackId of (args?.trackIds ?? []) as number[]) {
                const at = store.findIndex((field) => field.trackId === trackId && field.name === name);
                if (at >= 0) store.splice(at, 1);
                if (value) store.push({ trackId, name, value });
              }
              return null;
            }
            case "library_edit_history": {
              const undone = (window as any).__undone ?? 0;
              const all = [
                { id: 2, savedMs: Date.UTC(2026, 9, 24, 3, 5), tracks: 1, fields: ["title"] },
                { id: 1, savedMs: Date.UTC(2026, 9, 24, 3, 0), tracks: 2, fields: ["genre", "label"] },
              ];
              const renamed = (window as any).__renamedGenres ? [{ id: 3, savedMs: Date.UTC(2026, 9, 24, 3, 10), tracks: 2, fields: ["genre"] }] : [];
              return [...renamed, ...all].slice(undone);
            }
            case "library_undo_last_edit":
              (window as any).__undone = ((window as any).__undone ?? 0) + 1;
              return { files: 1, left: [] };
            case "library_track_tags":
              return ((args?.trackIds ?? []) as number[]).map((id) => ({
                trackId: id, title: `${String(id).padStart(2, "0")} - RUST TRACK ${String(id).padStart(4, "0")}`, artist: "Backend Artist",
                album: "Backend Release", albumArtist: null, genre: "House", label: null, year: 2019, track: id,
              }));
            case "library_edit_tracks":
              (window as any).__edits = [...((window as any).__edits ?? []), { trackIds: args?.trackIds, changes: args?.changes }];
              return { files: (args?.trackIds ?? []).length, left: [] };
            case "library_tidy_folder":
              if (!(window as any).__rbWriting) throw "rekordbox uses files in this folder, so it has to be told where they go. Writing to rekordbox is off. Turn it on in Settings.";
              (window as any).__tidied = [...((window as any).__tidied ?? []), { rootId: args?.rootId, folder: args?.folder }];
              return { folder: args?.folder, suggested: "Studio 001", moved: 1, relocated: 1, left: [] };
            case "library_check":
              return {
                files: 10,
                untidy: (window as any).__tidied ? [] : [{ rootId: 7, rootLabel: "Music", folder: "Studio 001;", suggested: "Studio 001", files: 1 }],
                duplicates: (window as any).__resolved ? [] : [{ artist: "Studio 001", title: "Soft Focus", files: ["Studio 001/Between Spaces/02 - Soft Focus.aif", "Studio 001;Guest/Between Spaces/02 - Soft Focus.aif"], fileIds: [11, 12], rootLabels: (window as any).__twoFolders ? ["Music", "Music2"] : ["Music", "Music"] }],
                broken: [{ rootLabel: "Music", path: "Studio 001/Between Spaces/03 - Late Train.aif", error: "not an audio file" }],
              };
            case "library_folder_layout":
              return layoutOut();
            case "library_set_folder_layout":
              if (String(args?.layout).includes("{nonsense}")) throw "That layout cannot be used: unknown template field `nonsense`.";
              layout = String(args?.layout);
              return layoutOut();
            case "library_plan_release":
              if (args?.name === "Night Drive EP") throw "That release is no longer in the download folder.";
              if (args?.name === "Clean Single") {
                return {
                  release: args?.name, rootLabel: "Music", ready: 1, blocked: 0,
                  files: [{
                    source: "Clean Single/01.aif", destination: "Studio 001/Clean Single/01 - Studio 001 - Clean Single.aif", shortened: false, problem: null, becomes: null,
                    tags: { fields: [{ name: "Title", value: "Clean Single", guessed: false, tidiedFrom: null }], audio: "AIFF 16/44.1", length: "3:00", readError: null, warnings: [] },
                  }],
                };
              }
              if (args?.name === "Second Light") {
                return {
                  release: args?.name, rootLabel: "Music", ready: 2, blocked: 0,
                  files: [1, 2].map((n) => ({
                    source: `Second Light/0${n}.mp3`, destination: `Studio 001/Second Light/0${n} - Studio 001 - Track ${n}.mp3`, shortened: false, problem: null, becomes: null,
                    tags: { fields: [{ name: "Title", value: `Track ${n}`, guessed: false, tidiedFrom: null }], audio: "MP3 320 kbps", length: "3:00", readError: null, warnings: n === 2 ? ["The year looks wrong."] : [] },
                  })),
                };
              }
              return {
                release: args?.name, rootLabel: "Music", ready: 1, blocked: 1,
                files: [
                  {
                    source: `${args?.name}/01.flac`, destination: `Studio 001/Between Spaces/01 - Studio 001 - Night Drive.${args?.options?.convert?.lossless === "aiff" ? "aif" : "flac"}`, shortened: false, problem: null,
                    becomes: args?.options?.convert?.lossless === "aiff" ? "AIFF" : null,
                    tags: {
                      fields: [
                        { name: "Artist", value: "Studio 001", guessed: false, tidiedFrom: args?.options?.tidy ? "Studio 001;" : null }, { name: "Title", value: "Studio 001 - Night Drive.mp3", guessed: false, tidiedFrom: null },
                        { name: "Track", value: "1", guessed: true, tidiedFrom: null }, { name: "Label", value: null, guessed: false, tidiedFrom: null },
                      ],
                      audio: "FLAC 16/44.1", length: "3:45", readError: null,
                      warnings: ["The title ends with a file extension."],
                    },
                  },
                  { source: `${args?.name}/02.flac`, destination: null, shortened: false, problem: "No track number in the tags or file name.", tags: null, becomes: null },
                ],
              };
            case "library_import_release":
              if ((window as any).__fileNothing) return { rootId: 7, rootLabel: "Music", filed: 0, skipped: [{ source: `${args?.name}/01.aif`, problem: "A file is already there." }] };
              imported.add(String(args?.name));
              if (args?.name === "Second Light") return { rootId: 7, rootLabel: "Music", filed: 2, skipped: [] };
              return { rootId: 7, rootLabel: "Music", filed: 1, skipped: [{ source: `${args?.name}/02.flac`, problem: "No track number in the tags or file name." }] };
            case "library_add_watch_folder":
              if ((window as any).__overlap) throw "That folder is, contains or lies inside your music library. Download folders must be separate from it.";
              downloads.push({ id: 9, label: "Downloads", path: (window as any).__downloadPath ?? "/Users/dj/Downloads", enabled: true, online: true, lastSeenMs: 1 });
              return downloads[0];
            case "library_set_watch_folder_enabled":
              downloads[0] = { ...downloads[0], enabled: args?.enabled };
              return null;
            case "library_retake_watch_baseline":
              return null;
            case "library_remove_watch_folder":
              downloads.splice(0, downloads.length);
              return null;
            case "library_confirm_root":
              (window as any).__folderState = undefined;
              return null;
            case "library_add_root":
              folders.push({ id: 7, label: "Music", path: "/Users/dj/Music", availability: "available", reason: "registered", lastAvailableMs: 1, files: 0, missing: 0 });
              return folders[0];
            case "library_scan":
              if (args?.rootId !== 7) throw "That folder is not in the library.";
              scanStarted = true;
              statusCalls = 0;
              return null;
            case "library_remove_root":
              folders.splice(0, folders.length);
              scanStarted = false;
              return 20;
            case "library_scan_status":
              return status();
            case "library_scan_cancel":
              return null;
            case "library_mirrors":
              return mirror ? [{ ...mirror, changedSinceRead: Boolean((window as any).__exportChanged) }] : [];
            case "library_playlists":
              return mirror ? [
                { id: 1, parentId: null, isFolder: true, name: "ROOT", position: 0, entries: 0, inLibrary: 0 },
                { id: 2, parentId: 1, isFolder: true, name: "Sets", position: 0, entries: 0, inLibrary: 0 },
                { id: 3, parentId: 2, isFolder: false, name: "Friday warm-up", position: 0, entries: 40, inLibrary: 38 },
                { id: 4, parentId: 1, isFolder: false, name: "Closers", position: 1, entries: 12, inLibrary: 12 },
              ] : [];
            case "library_import_xml":
              if (mode === "managed" && (window as any).__failImport) throw "invalid XML syntax";
              mirror = makeMirror(1, 0, 9008);
              return { sourceId: 3, generation: 1, unchanged: false, entries: 9008, matched: 0, derivedSet: 0 };
            case "library_alias_suggestions":
              return aliasAdded ? [] : [{ rootId: 7, rootLabel: "Music", prefix: "/G:/Library", entries: 8977 }];
            case "library_add_alias":
              if (args?.prefix !== "/G:/Library") throw "That is not a valid rekordbox Location prefix.";
              aliasAdded = true;
              return true;
            case "rekordbox_db_status":
              return rbState();
            case "rekordbox_db_enable":
              rb().enabled = args?.enabled === true;
              rb().confirmedVersion = rb().enabled ? rbVersion() : null;
              if (!rb().enabled) setRbWrite(null);
              return rbState();
            case "rekordbox_db_enable_write":
              if (args?.enabled === true && !rb().enabled) throw "Turn on reading rekordbox's library first.";
              setRbWrite(args?.enabled === true ? rbVersion() : null);
              return rbState();
            case "rekordbox_add_to_playlist": {
              if (rbWrite() === null) throw "Writing to rekordbox is off. Turn it on in Settings.";
              if ((window as any).__rbRunning) throw "rekordbox is running. Quit it (and wait for its agent to stop) first.";
              (window as any).__rbAdded = [...((window as any).__rbAdded ?? []), { playlistId: args?.playlistId, trackIds: args?.trackIds }];
              const names: Record<number, string> = { 3: "Friday warm-up", 4: "Closers" };
              return { playlist: names[args?.playlistId] ?? "?", added: (args?.trackIds ?? []).length, imported: 0, leftOut: [], readError: null };
            }
            case "rekordbox_create_folder": {
              if (rbWrite() === null) throw "Writing to rekordbox is off. Turn it on in Settings.";
              const name = String(args?.name ?? "").trim();
              (window as any).__rbFolders = [...((window as any).__rbFolders ?? []), { name, folderId: args?.folderId }];
              return { name: name === "Sets" ? "Sets (2)" : name, readError: null };
            }
            case "rekordbox_crate_to_playlist": {
              if (rbWrite() === null) throw "Writing to rekordbox is off. Turn it on in Settings.";
              const made = crateById(args?.crateId);
              if (made.order.length === 0) throw `${made.name} has no tracks yet, so there is nothing to put in a playlist.`;
              (window as any).__rbCratePlaylists = [...((window as any).__rbCratePlaylists ?? []), { crateId: args?.crateId, folderId: args?.folderId }];
              return { playlist: made.name, added: made.order.length, imported: 0, leftOut: [], readError: null };
            }
            case "rekordbox_merge_review": {
              if (!rb().enabled) throw "The merge works from a direct read of rekordbox's library. Turn reading on in Settings first.";
              const change = (rbKey: number, title: string, field: string, ours: string, theirs: string, direction: string, base: string | null) =>
                ({ rbKey, trackId: rbKey, title, artist: "Studio 001", field, base, ours, theirs, direction });
              return {
                canWrite: rbWrite() !== null,
                changes: (window as any).__merged ? [] : [
                  change(101, "Night Drive", "title", "Night Drive", "Night Drive (Original Mix)", "toOpenCrate", "Night Drive"),
                  change(102, "Soft Focus", "label", "Test Label", "", "toRekordbox", null),
                  change(103, "Late Train", "year", "2019", "2020", "conflict", "2018"),
                ],
              };
            }
            case "rekordbox_merge_apply": {
              const choices = (args?.choices ?? []) as { keep: string }[];
              if (choices.some((choice) => choice.keep === "openCrate") && rbWrite() === null) {
                throw "Keeping openCrate's values means writing to rekordbox. Writing to rekordbox is off. Turn it on in Settings.";
              }
              (window as any).__mergeChoices = choices;
              (window as any).__merged = true;
              return { toRekordbox: choices.filter((choice) => choice.keep === "openCrate").length, toOpencrate: choices.filter((choice) => choice.keep === "rekordbox").length, skipped: [] };
            }
            case "rekordbox_send_new": {
              if (rbWrite() === null) throw "Writing to rekordbox is off. Turn it on in Settings.";
              if ((window as any).__rbRunning) throw "rekordbox is running. Quit it (and wait for its agent to stop) first.";
              const named = args?.fileIds as number[] | null;
              const sent = (track: any) => !track.hidden && (named == null || named.includes(track.fileId));
              const fresh = ((window as any).__newTracks ?? []).filter(sent);
              (window as any).__newTracks = ((window as any).__newTracks ?? []).filter((track: any) => !sent(track));
              (window as any).__rbSentNew = ((window as any).__rbSentNew ?? 0) + 1;
              return { playlist: "Imported", added: fresh.length, imported: fresh.length, leftOut: [], readError: null };
            }
            case "rekordbox_db_read":
              if ((window as any).__rbRunning) throw "rekordbox is running. Quit it (and wait for its agent to stop) first.";
              rbBackups.unshift(rbBackup(`2026102${rbBackups.length}-120000-read`, "read", false));
              mirror = { ...makeMirror(1, 38, 2), kind: "master_db", path: "/Users/dj/Library/Pioneer/rekordbox/master.db", entries: 40 };
              return { sourceId: 3, generation: 1, unchanged: false, entries: 40, matched: 38, derivedSet: 38 };
            case "rekordbox_backups":
              return rbBackups;
            case "rekordbox_backup_pin": {
              const found = rbBackups.find((backup) => backup.name === args?.name);
              if (!found) throw "That backup was not found.";
              found.pinned = args?.pinned === true;
              return rbBackups;
            }
            case "rekordbox_restore": {
              if (!rbBackups.some((backup) => backup.name === args?.name)) throw "That backup was not found.";
              const saved = rbBackup("20261023-130000-before-restore", "before-restore", true);
              rbBackups.unshift(saved);
              return saved;
            }
            case "rekordbox_crates":
            case "rekordbox_set_crate":
            case "rekordbox_update_file": {
              const shown = crateList.map((crate) => ({
                crateId: crate.id, name: crate.name, inFile: rbCrates.has(crate.id),
                known: Math.max(0, crate.order.length - 1), new: crate.order.length > 0 ? 1 : 0, needsExport: crate.id === 2 ? 1 : 0, noFile: 0,
              }));
              if (command === "rekordbox_set_crate") {
                if (!crateList.some((crate) => crate.id === args?.crateId)) throw "no crate with that id";
                if (args?.inFile) rbCrates.add(args.crateId); else rbCrates.delete(args?.crateId);
                return { crates: shown.map((crate) => ({ ...crate, inFile: rbCrates.has(crate.crateId) })), xmlPath: "/Users/dj/Library/Application Support/openCrate/rekordbox/opencrate.xml", folder: "openCrate crates" };
              }
              return { crates: shown, xmlPath: "/Users/dj/Library/Application Support/openCrate/rekordbox/opencrate.xml", folder: "openCrate crates" };
            }
            case "rekordbox_hide":
            case "rekordbox_new_tracks": {
              if (command === "rekordbox_hide") {
                for (const track of (window as any).__newTracks ?? []) if ((args?.fileIds ?? []).includes(track.fileId)) track.hidden = args?.hidden === true;
              }
              const now = Date.UTC(2026, 9, 23, 12);
              const tracks = ((window as any).__newTracks ?? []) as Record<string, any>[];
              const read = (window as any).__mirrorReadMs;
              return {
                tracks, mirrorReadMs: read === undefined ? now : read, newestFiledMs: tracks.length ? now - 1000 : null,
                xmlPath: "/Users/dj/Library/Application Support/openCrate/rekordbox/opencrate.xml", playlist: "openCrate new tracks",
              };
            }
            case "rekordbox_send": {
              const ids = (args?.fileIds ?? []) as number[];
              if (ids.length === 0) throw "Choose at least one track to send.";
              for (const track of (window as any).__newTracks ?? []) if (ids.includes(track.fileId)) track.offeredMs = Date.UTC(2026, 9, 23, 13);
              return { xmlPath: "/Users/dj/Library/Application Support/openCrate/rekordbox/opencrate.xml", tracks: ids.length };
            }
            case "library_reread_mirror":
              mirror = makeMirror(2, 8977, 31);
              return { sourceId: 3, generation: 2, unchanged: false, entries: 9008, matched: 8977, derivedSet: 8977 };
            case "library_read": {
              if (failReads) throw "The backend could not read the library.";
              const { start, count, query } = args as { start: number; count: number; query: { view: string; crateId?: number | null } };
              const row = (i: number) => ({
                id: i + 1, kind: query.view, title: `Rust Track ${String(i + 1).padStart(4, "0")}`,
                artist: "Backend Artist", artistId: 1, release: "Backend Release", releaseId: 2,
                bpm: i % 5 === 0 ? null : 120, musicalKey: "8A", durationMs: 200_000,
                format: query.view === "tracks" ? "AIFF" : null, trackCount: 1, missing: i % 50 === 0, analysed: i % 5 > 1, rekordboxKey: i % 5 !== 1,
              });
              if ((query as { sort?: string }).sort === "custom" && libraryOrder) {
                return { items: libraryOrder.slice(start, start + count).map((id) => row(id - 1)), total: libraryOrder.length, start };
              }
              if (query.crateId != null) {
                const order = crateById(query.crateId).order;
                return { items: order.slice(start, start + count).map((id) => row(id - 1)), total: order.length, start };
              }
              const total = (window as any).__emptyLibrary ? 0 : scanStarted && statusCalls >= 3 ? 1500 : 1234;
              const items = [];
              for (let i = start; i < Math.min(start + count, total); i += 1) {
                items.push({
                  id: i + 1, kind: query.view, title: `Rust Track ${String(i + 1).padStart(4, "0")}`,
                  artist: "Backend Artist", artistId: 1, release: "Backend Release", releaseId: 2,
                  bpm: i % 5 === 0 ? null : 120, musicalKey: "8A", durationMs: 200_000,
                  format: query.view === "tracks" ? "AIFF" : null, trackCount: 1, missing: i % 50 === 0, analysed: i % 5 > 1, rekordboxKey: i % 5 !== 1,
                });
              }
              return { items, total, start };
            }
            default:
              throw new Error(`unexpected command ${command}`);
          }
        },
      };
    },
    { mode, failReads },
  );
}

const calls = (page: Page) => page.evaluate(() => (window as any).__calls as { command: string; args?: any }[]);

test("a desktop app with a database shows rows read through the Rust commands", async ({ page }) => {
  await stubBridge(page, "database");
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("1,234 tracks");
  await expect(page.getByTestId("track-row").first()).toContainText("Rust Track 0001");
  const seen = await calls(page);
  expect(seen[0]?.command).toBe("library_mode");
  const firstRead = seen.find((call) => call.command === "library_read");
  expect(firstRead?.args).toEqual({
    query: { view: "tracks", search: "", sort: "recent", format: null, artistId: null, releaseId: null, playlistId: null, crateId: null },
    start: 0,
    count: expect.any(Number),
  });
  expect(firstRead?.args.count).toBeLessThanOrEqual(200);

  await page.getByRole("textbox", { name: "Search library" }).fill("night");
  await page.getByRole("button", { name: "MP3", exact: true }).click();
  await page.getByLabel("Sort library").selectOption("bpm");
  await expect
    .poll(async () => (await calls(page)).filter((call) => call.command === "library_read").at(-1)?.args.query)
    .toEqual({ view: "tracks", search: "night", sort: "bpm", format: "MP3", artistId: null, releaseId: null, playlistId: null, crateId: null });
});

test("a desktop app without a database keeps the in-memory demo", async ({ page }) => {
  await stubBridge(page, "synthetic");
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  expect((await calls(page)).map((call) => call.command)).toEqual(["library_mode"]);
});

test("a bridge that fails to answer falls back to the in-memory demo", async ({ page }) => {
  await stubBridge(page, "broken");
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
});

test("a backend read error is shown with a way to retry", async ({ page }) => {
  await stubBridge(page, "database", true);
  await page.goto("/");
  await expect(page.getByRole("alert")).toContainText("The backend could not read the library.");
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
});

test("a database without folder management keeps the demo's folder copy", async ({ page }) => {
  await stubBridge(page, "database");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByText("Folder selection is not connected in this demo.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Add folder..." })).toHaveCount(0);
});

test("folders are added, scanned with visible progress, and the library reloads", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("1,234 tracks");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("No folders yet")).toBeVisible();
  await expect(dialog.getByText("temporary and is discarded")).toBeVisible();
  await expect(page.getByText("DEMO DATABASE", { exact: true })).toBeVisible();

  await dialog.getByRole("button", { name: "Add folder..." }).click();
  await expect(dialog.getByText("/Users/dj/Music")).toBeVisible();
  await expect(dialog.getByText("0 files")).toBeVisible();
  await expect(dialog.getByText("Available")).toBeVisible();

  await dialog.getByRole("button", { name: "Scan Music" }).click();
  await expect(dialog.getByLabel("Scan progress")).toBeVisible();
  await page.screenshot({ path: "test-results/folders-scanning.png" });
  await expect(dialog.getByRole("button", { name: "Scan Music" })).toBeDisabled();
  await expect(dialog.getByRole("status").filter({ hasText: "Reading tags" })).toBeVisible();
  await expect(dialog.getByText("Done: 19 new tracks, 0 updated, 1 no longer on disk, 1 without readable tags.")).toBeVisible();
  await expect(dialog.getByText("20 files")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Scan Music" })).toBeEnabled();
  await page.screenshot({ path: "test-results/folders-finished.png" });

  await dialog.getByRole("button", { name: "Close settings" }).click();
  await expect(page.getByRole("status").first()).toContainText("1,500 tracks");
  const commands = (await calls(page)).map((call) => call.command);
  expect(commands).toContain("library_add_root");
  expect(commands.filter((command) => command === "library_scan")).toHaveLength(1);
});

test("a scan error is shown and does not leave the dialog stuck", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => {
    const original = (window as any).__TAURI_INTERNALS__.invoke;
    (window as any).__TAURI_INTERNALS__.invoke = async (command: string, args?: any) => {
      if (command === "library_scan") throw "A scan is already running.";
      return original(command, args);
    };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Add folder..." }).click();
  await dialog.getByRole("button", { name: "Scan Music" }).click();
  await expect(dialog.getByRole("alert")).toContainText("A scan is already running.");
  await expect(dialog.getByRole("button", { name: "Scan Music" })).toBeEnabled();
});

test("rekordbox sync: import an export, accept the suggested prefix, see the match", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await expect(page.getByRole("heading", { name: "No export yet" })).toBeVisible();

  await page.getByRole("button", { name: "Choose export..." }).click();
  await expect(page.getByRole("heading", { name: "collection.xml" })).toBeVisible();
  await expect(page.getByText("/Users/dj/exports/collection.xml")).toBeVisible();
  await expect(page.getByText("Not in your folders", { exact: true }).locator("..")).toContainText("9,008");
  await expect(page.getByText("Read 9,008 entries. 0 match your files.")).toBeVisible();
  await expect(page.getByText("8,977 entries start with /G:/Library")).toBeVisible();
  await page.screenshot({ path: "test-results/sync-suggestion.png" });

  await page.getByRole("button", { name: "Use /G:/Library for Music" }).click();
  await expect(page.getByText("8,977 match your files, so 8,977 tracks now show rekordbox's tempo and key.")).toBeVisible();
  await expect(page.getByText("Match your files", { exact: true }).locator("..")).toContainText("8,977");
  await expect(page.getByText("These entries look like they belong to a folder")).toHaveCount(0);
  await page.screenshot({ path: "test-results/sync-matched.png" });

  const commands = (await calls(page)).map((call) => call.command);
  const tail = commands.slice(commands.indexOf("library_add_alias"));
  expect(tail.slice(0, 2)).toEqual(["library_add_alias", "library_reread_mirror"]);
  await page.getByRole("button", { name: "Read again" }).click();
  await expect(page.getByRole("status").filter({ hasText: "match your files" })).toBeVisible();
});

test("rekordbox sync: a failed read is explained and does not lose the screen", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__failImport = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await page.getByRole("button", { name: "Choose export..." }).click();
  await expect(page.getByRole("alert")).toContainText("invalid XML syntax");
  await expect(page.getByRole("button", { name: "Choose export..." })).toBeEnabled();
});

test("without a database the rekordbox screen stays an honest placeholder", async ({ page }) => {
  await stubBridge(page, "synthetic");
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await expect(page.getByText("Reading a rekordbox export needs a library on disk.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Choose export..." })).toHaveCount(0);
});

test("a folder is removed only after confirmation, and the library reloads", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Add folder..." }).click();
  await expect(dialog.getByText("/Users/dj/Music")).toBeVisible();

  await dialog.getByRole("button", { name: "Remove Music" }).click();
  await expect(dialog.getByRole("alertdialog")).toContainText("Your music files are not touched.");
  await dialog.getByRole("button", { name: "Keep it" }).click();
  await expect(dialog.getByRole("alertdialog")).toHaveCount(0);
  await expect(dialog.getByText("/Users/dj/Music")).toBeVisible();
  expect((await calls(page)).filter((call) => call.command === "library_remove_root")).toHaveLength(0);

  await dialog.getByRole("button", { name: "Remove Music" }).click();
  await dialog.getByRole("button", { name: "Remove from library" }).click();
  await expect(dialog.getByText("No folders yet")).toBeVisible();
  const removal = (await calls(page)).filter((call) => call.command === "library_remove_root");
  expect(removal.map((call) => call.args)).toEqual([{ rootId: 7 }]);
});

test("download folders are added, switched off and removed, and a refused folder is explained", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("No download folders yet.")).toBeVisible();

  await page.evaluate(() => { (window as any).__overlap = true; });
  await dialog.getByRole("button", { name: "Add download folder..." }).click();
  await expect(dialog.getByRole("alert")).toContainText("Download folders must be separate from it.");
  await page.evaluate(() => { (window as any).__overlap = false; });

  await dialog.getByRole("button", { name: "Add download folder..." }).click();
  await expect(dialog.getByText("/Users/dj/Downloads")).toBeVisible();
  await expect(dialog.getByText("Watching", { exact: true })).toBeVisible();
  await expect(dialog.getByRole("alert")).toHaveCount(0);

  await dialog.getByRole("button", { name: "Turn off Downloads" }).click();
  await expect(dialog.getByText("Off", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "Turn on Downloads" }).click();
  await expect(dialog.getByText("Watching", { exact: true })).toBeVisible();

  await dialog.getByRole("button", { name: "Count what is in Downloads now as already there" }).click();
  await expect(dialog.getByText("Everything in Downloads now counts as already there.")).toBeVisible();
  await dialog.getByRole("button", { name: "Remove Downloads from download folders" }).click();
  await expect(dialog.getByText("No download folders yet.")).toBeVisible();
  const sent = (await calls(page)).filter((call) => /watch_folder|watch_baseline/.test(call.command) && call.args);
  expect(sent.map((call) => [call.command, call.args])).toEqual([
    ["library_set_watch_folder_enabled", { id: 9, enabled: false }],
    ["library_set_watch_folder_enabled", { id: 9, enabled: true }],
    ["library_retake_watch_baseline", { id: 9 }],
    ["library_remove_watch_folder", { id: 9 }],
  ]);
});

test("settings open from the keyboard and from the app menu", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await page.keyboard.press("ControlOrMeta+Comma");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Close settings" }).click();
  await expect(dialog).toBeHidden();
  // What the macOS menu item sends.
  await page.evaluate(() => window.dispatchEvent(new CustomEvent("opencrate:open-settings")));
  await expect(dialog).toBeVisible();
});

test("settings close with a click outside them, but not with a drag that ends outside", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await expect(dialog).toBeVisible();
  const box = (await dialog.boundingBox())!;
  // Pressed inside, let go outside: still open.
  await page.mouse.move(box.x + 40, box.y + 60);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width + 30, box.y + 60, { steps: 4 });
  await page.mouse.up();
  await expect(dialog).toBeVisible();
  // A click inside its padding: still open.
  await page.mouse.click(box.x + 6, box.y + 6);
  await expect(dialog).toBeVisible();
  // A click outside: closed.
  await page.mouse.click(box.x + box.width + 30, box.y + 60);
  await expect(dialog).toBeHidden();
});

test("the settings close button stays put while the settings scroll", async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 420 });
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("button", { name: "Add folder..." }).click();
  await dialog.getByRole("button", { name: "Add download folder..." }).click();
  await expect(dialog.getByText("/Users/dj/Downloads")).toBeVisible();

  const body = dialog.locator(".dialog-body");
  expect(await body.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
  await body.evaluate((element) => { element.scrollTop = element.scrollHeight; });
  await expect(dialog.getByText("Nothing is uploaded.", { exact: false })).toBeInViewport();
  const close = dialog.getByRole("button", { name: "Close settings" });
  await expect(close).toBeInViewport();
  await close.click();
  await expect(dialog).toBeHidden();
});

test("a release can be previewed before anything is imported, and the screen scrolls in a small window", async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 560 });
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("button", { name: "Add folder..." }).click();
  await dialog.getByRole("button", { name: "Add download folder..." }).click();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Import" }).first().click();
  await page.getByRole("button", { name: "By release" }).click();

  const screen = page.locator(".sync-screen");
  await expect(page.getByRole("heading", { name: "Already there when you added the folder" })).toBeAttached();
  // Sections keep their own height and never draw over each other, however small the window.
  const boxes = await screen.evaluate((element) => [...element.children].map((child) => {
    const box = child.getBoundingClientRect();
    return { top: box.top, bottom: box.bottom, height: box.height, content: (child as HTMLElement).scrollHeight };
  }));
  boxes.forEach((box, index) => {
    const before = boxes[index - 1];
    if (before) expect(box.top).toBeGreaterThanOrEqual(before.bottom - 1);
  });
  for (const box of boxes) expect(box.height).toBeGreaterThanOrEqual(box.content - 1);
  const scrolls = await screen.evaluate((element) => element.scrollHeight > element.clientHeight);
  expect(scrolls).toBe(true);
  await screen.evaluate((element) => { element.scrollTop = element.scrollHeight; });
  await expect(page.getByRole("listitem").filter({ hasText: "Old Favourites" })).toBeInViewport();

  await screen.evaluate((element) => { element.scrollTop = 0; });
  await expect(page.getByRole("button", { name: "Preview Between Spaces" })).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByText("1 file is ready to go into Music; 1 needs attention and stays in the download folder.")).toBeVisible();
  await expect(page.getByText("Tags are written as shown")).toBeVisible();
  await expect(page.getByText("Studio 001/Between Spaces/01 - Studio 001 - Night Drive.flac")).toBeVisible();
  await expect(page.getByText("No track number in the tags or file name.")).toBeVisible();
  // A wide table scrolls inside its card; the screen itself never scrolls sideways.
  expect(await screen.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
  const button = await page.getByRole("button", { name: "Preview Between Spaces" }).boundingBox();
  const area = await screen.boundingBox();
  expect(button && area && button.x + button.width <= area.x + area.width).toBe(true);
  // Tags are columns: nothing to click to see them. "#" is off until chosen.
  const table = page.locator(".plan-table");
  for (const heading of ["Title", "Album", "Year", "Genre", "Key", "BPM", "Length", "Format", "Goes to"]) {
    await expect(table.getByRole("columnheader", { name: heading, exact: true })).toBeVisible();
  }
  await expect(table.getByRole("columnheader", { name: "#", exact: true })).toHaveCount(0);
  const first = table.getByRole("row").filter({ hasText: "Studio 001 - Night Drive.mp3" });
  await expect(first.getByText("FLAC 16/44.1")).toBeVisible();
  await expect(first.getByText("3:45")).toBeVisible();
  await expect(first.locator(".tag-missing").first()).toBeVisible();
  await expect(page.getByRole("list", { name: "Notes for Between Spaces/01.flac" })).toContainText("The title ends with a file extension.");
  await expect(page.getByRole("list", { name: "Notes for Between Spaces/02.flac" })).toContainText("No track number");

  // Right-click a heading to choose columns; Title cannot be hidden. The heading is brought into
  // view first, as a person would, so the scroll does not land after the menu opens and close it.
  await table.getByRole("columnheader", { name: "Year" }).scrollIntoViewIfNeeded();
  await page.waitForTimeout(100);
  await table.getByRole("columnheader", { name: "Year" }).click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Columns" });
  await expect(menu.getByRole("menuitemcheckbox", { name: "Title" })).toHaveAttribute("aria-disabled", "true");
  await menu.getByRole("menuitemcheckbox", { name: "# (track and disc)" }).click();
  await menu.getByRole("menuitemcheckbox", { name: "Genre" }).click();
  await expect(menu.getByRole("menuitemcheckbox", { name: "Genre" })).toHaveAttribute("aria-checked", "false");
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(table.getByRole("columnheader", { name: "#", exact: true })).toBeVisible();
  await expect(table.getByRole("columnheader", { name: "Genre" })).toHaveCount(0);
  await expect(first.locator(".tag-guessed")).toContainText("1?");

  // Drag a heading's edge to resize it; the arrow keys work too.
  const titleHeading = table.getByRole("columnheader", { name: "Title" });
  const before = (await titleHeading.boundingBox())!.width;
  const handle = (await page.getByRole("separator", { name: "Resize Title" }).boundingBox())!;
  await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
  await page.mouse.down();
  await page.mouse.move(handle.x + handle.width / 2 + 80, handle.y + handle.height / 2, { steps: 5 });
  await page.mouse.up();
  const after = (await titleHeading.boundingBox())!.width;
  expect(Math.abs(after - before - 80)).toBeLessThanOrEqual(2);
  await page.getByRole("separator", { name: "Resize Year" }).focus();
  const yearBefore = (await table.getByRole("columnheader", { name: "Year" }).boundingBox())!.width;
  await page.keyboard.press("ArrowRight");
  expect(Math.round((await table.getByRole("columnheader", { name: "Year" }).boundingBox())!.width - yearBefore)).toBe(10);

  // Both choices survive a reload.
  await page.reload();
  // The stand-in backend forgets its folders on reload; add them again.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("dialog", { name: "Settings" }).getByRole("button", { name: "Add folder..." }).click();
  await page.getByRole("dialog", { name: "Settings" }).getByRole("button", { name: "Add download folder..." }).click();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Import" }).first().click();
  await expect(page.getByRole("button", { name: "Preview Between Spaces" })).toHaveAttribute("aria-expanded", "true");
  await expect(table.getByRole("columnheader", { name: "Genre" })).toHaveCount(0);
  await expect(table.getByRole("columnheader", { name: "#", exact: true })).toBeVisible();
  expect(Math.abs((await titleHeading.boundingBox())!.width - after)).toBeLessThanOrEqual(2);
  await page.getByRole("button", { name: "Preview Night Drive EP" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "no longer in the download folder" })).toBeVisible();
  const sent = (await calls(page)).filter((call) => call.command === "library_plan_release");
  const kept = { convert: { lossless: null, lossy: null }, tidy: true };
  expect(sent.map((call) => call.args)).toEqual([
    { folderId: 9, name: "Between Spaces", rootId: 7, options: kept },
    { folderId: 9, name: "Night Drive EP", rootId: 7, options: kept },
  ]);
});

test("import: a previewed release is copied in, the library rescanned, and it moves to Imported", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("dialog", { name: "Settings" }).getByRole("button", { name: "Add folder..." }).click();
  await page.getByRole("dialog", { name: "Settings" }).getByRole("button", { name: "Add download folder..." }).click();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Import" }).first().click();
  await page.getByRole("button", { name: "By release" }).click();

  await expect(page.getByRole("button", { name: "Preview Between Spaces" })).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByText("the originals stay in Downloads")).toBeVisible();
  await page.getByRole("button", { name: "Import 1 file" }).click();
  await expect(page.getByText("1 file is now in Music. The originals are still in Downloads.")).toBeVisible();
  await expect(page.getByText("No track number in the tags or file name.")).toBeVisible();
  const imported = page.locator(".sync-card").filter({ has: page.getByRole("heading", { name: "Imported" }) });
  await expect(imported.getByText("Between Spaces", { exact: true })).toBeVisible();
  await expect(imported.locator(".folder-badge")).toHaveText("Imported");
  const sent = await calls(page);
  expect(sent.filter((call) => call.command === "library_import_release").map((call) => call.args)).toEqual([{ folderId: 9, name: "Between Spaces", rootId: 7, options: { convert: { lossless: null, lossy: null }, tidy: true } }]);
  const after = sent.slice(sent.findIndex((call) => call.command === "library_import_release"));
  expect(after.some((call) => call.command === "library_scan" && call.args?.rootId === 7)).toBe(true);

  // A release still downloading can be previewed but not imported.
  await page.getByRole("button", { name: "Preview Night Drive EP" }).click();
  await expect(page.getByRole("button", { name: /^Import \d/ })).toHaveCount(0);
});

test("import: with writing to rekordbox on, imported tracks go into its Imported playlist", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => {
    (window as any).__rbEnabled = true;
    (window as any).__rbWriting = true;
    (window as any).__newTracks = [{ fileId: 31, title: "Night Drive", artist: "Studio 001", album: "Between Spaces", durationMs: 225_000, container: "FLAC", filedMs: 1, offeredMs: null, hidden: false }];
  });
  await openImportWithFolders(page);
  await page.getByRole("button", { name: "Import 1 file" }).click();
  await expect(page.getByText("Added 1 track to rekordbox's Imported playlist.")).toBeVisible();
  expect(await page.evaluate(() => (window as any).__rbSentNew)).toBe(1);
});

test("import: with rekordbox open, imported tracks wait for it and the note says so", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => {
    (window as any).__rbEnabled = true;
    (window as any).__rbWriting = true;
    (window as any).__rbRunning = true;
  });
  await openImportWithFolders(page);
  await page.getByRole("button", { name: "Import 1 file" }).click();
  await expect(page.getByText(/Not added to rekordbox yet: rekordbox is running/)).toBeVisible();
  expect(await page.evaluate(() => (window as any).__rbSentNew ?? 0)).toBe(0);
});

/** Opens Import with a music and a download folder, showing releases one by one unless told otherwise. */
async function openImportWithFolders(page: Page, view: "One table" | "By release" = "By release") {
  // Chosen before the screen opens, so no release is read for the other view first.
  await page.addInitScript((value) => { localStorage.setItem("opencrate.importView", value); }, view === "By release" ? "releases" : "table");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("dialog", { name: "Settings" }).getByRole("button", { name: "Add folder..." }).click();
  await page.getByRole("dialog", { name: "Settings" }).getByRole("button", { name: "Add download folder..." }).click();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Import" }).first().click();
  await page.getByRole("group", { name: "Show releases as" }).getByRole("button", { name: view }).click();
}

test("import choices: conversion and tidying are sent with the preview and shown in it", async ({ page }) => {
  await stubBridge(page, "managed");
  await openImportWithFolders(page);
  await page.getByLabel("Convert lossless files to").selectOption("aiff");
  await expect(page.getByRole("button", { name: "Preview Between Spaces" })).toHaveAttribute("aria-expanded", "true");
  const table = page.getByRole("table");
  await expect(table.getByRole("cell", { name: "AIFF", exact: true })).toBeVisible();
  await expect(table.getByRole("columnheader", { name: "Becomes" })).toBeVisible();
  await expect(table.getByText("Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif")).toBeVisible();
  await expect(table.getByLabel("tidied, was Studio 001;")).toBeVisible();
  const sent = (await calls(page)).filter((call) => call.command === "library_plan_release").at(-1)!;
  expect(sent.args.options).toEqual({ convert: { lossless: "aiff", lossy: null }, tidy: true });

  // Turning tidying off re-plans without it, and the choices are remembered.
  await page.getByRole("checkbox", { name: /Tidy names/ }).uncheck();
  await expect(page.getByRole("button", { name: "Preview Between Spaces" })).toHaveAttribute("aria-expanded", "true");
  await expect(table.getByLabel("tidied, was Studio 001;")).toHaveCount(0);
  await page.reload();
  await openImportWithFolders(page);
  await expect(page.getByLabel("Convert lossless files to")).toHaveValue("aiff");
  await expect(page.getByRole("checkbox", { name: /Tidy names/ })).not.toBeChecked();
});

test("import choices: without ffmpeg converting cannot be chosen and is not sent", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => {
    (window as any).__noFfmpeg = true;
    localStorage.setItem("opencrate.importOptions", JSON.stringify({ convert: { lossless: "aiff", lossy: null }, tidy: true }));
  });
  await openImportWithFolders(page);
  await expect(page.getByLabel("Convert lossless files to")).toBeDisabled();
  await expect(page.getByText("Converting needs ffmpeg")).toBeVisible();
  await expect(page.getByRole("button", { name: "Preview Between Spaces" })).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByRole("table")).toBeVisible();
  const sent = (await calls(page)).filter((call) => call.command === "library_plan_release").at(-1)!;
  expect(sent.args.options.convert).toEqual({ lossless: null, lossy: null });
  await expect(page.getByRole("table").getByRole("columnheader", { name: "Becomes" })).toHaveCount(0);
});

test("import: finished releases have their tags read without a click, and those needing a look open", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__cleanReady = true; });
  await openImportWithFolders(page);
  const between = page.getByRole("listitem").filter({ hasText: "Between Spaces" });
  const clean = page.getByRole("listitem").filter({ hasText: "Clean Single" });
  await expect(between.getByRole("button", { name: "Preview Between Spaces" })).toHaveAttribute("aria-expanded", "true");
  await expect(between.locator(".folder-badge")).toHaveText("Needs attention");
  await expect(between.getByText("No track number in the tags or file name.")).toBeVisible();
  await expect(clean.getByRole("button", { name: "Preview Clean Single" })).toHaveAttribute("aria-expanded", "false");
  await expect(clean.locator(".folder-badge")).toHaveText("Ready");
  await expect(clean.getByText("Goes to Studio 001/Clean Single/01 - Studio 001 - Clean Single.aif")).toBeVisible();
  // Still downloading, and already there before the folder was added: read only when opened.
  const planned = async () => (await calls(page)).filter((call) => call.command === "library_plan_release").map((call) => call.args.name);
  expect(await planned()).toEqual(["Between Spaces", "Clean Single"]);

  // Closing one by hand keeps it closed, through the next look at the folder.
  await between.getByRole("button", { name: "Preview Between Spaces" }).click();
  await page.waitForTimeout(3500);
  await expect(between.getByRole("button", { name: "Preview Between Spaces" })).toHaveAttribute("aria-expanded", "false");
  await expect(between.getByText("1 file ready to go into Music")).toBeVisible();
  expect(await planned()).toEqual(["Between Spaces", "Clean Single"]);
});

test("import: one table shows every release's tags at once, with an Import button per release", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__cleanReady = true; });
  await openImportWithFolders(page, "One table");
  // No row has to be opened: every finished release, new or already there, has its tags read.
  const planned = async () => (await calls(page)).filter((call) => call.command === "library_plan_release").map((call) => call.args.name).sort();
  await expect.poll(planned).toEqual(["Between Spaces", "Clean Single", "Old Favourites"]);
  await expect(page.getByRole("button", { name: /^Preview / })).toHaveCount(0);

  const fresh = page.locator(".sync-card").filter({ hasText: "New since you added the folder" });
  const groups = fresh.locator("tbody.plan-group");
  await expect(groups).toHaveCount(4);
  const between = groups.filter({ hasText: "Between Spaces" });
  await expect(between.locator(".folder-badge")).toHaveText("Needs attention");
  await expect(between.getByText("No track number in the tags or file name.")).toBeVisible();
  await expect(between.getByText("FLAC 16/44.1")).toBeVisible();
  const clean = groups.filter({ hasText: "Clean Single" });
  await expect(clean.locator(".folder-badge")).toHaveText("Ready");
  await expect(clean.getByRole("cell", { name: "Studio 001/Clean Single/01 - Studio 001 - Clean Single.aif" })).toBeVisible();
  // Still downloading: listed with its state, nothing to import yet.
  const downloading = groups.filter({ hasText: "Night Drive EP" });
  await expect(downloading.locator(".folder-badge")).toHaveText("Still downloading");
  await expect(downloading.getByRole("button", { name: /^Import / })).toHaveCount(0);
  // One heading row for the whole table.
  await expect(fresh.getByRole("columnheader", { name: "Title", exact: true })).toHaveCount(1);
  const older = page.locator(".sync-card").filter({ hasText: "Already there when you added the folder" });
  await expect(older.locator("tbody.plan-group").filter({ hasText: "Old Favourites" }).getByRole("button", { name: "Import Old Favourites" })).toBeVisible();

  const readsOf = async (name: string) => (await calls(page)).filter((call) => call.command === "library_plan_release" && call.args.name === name).length;
  expect(await readsOf("Between Spaces")).toBe(1);
  await clean.getByRole("button", { name: "Import Clean Single" }).click();
  await expect.poll(async () => (await calls(page)).filter((call) => call.command === "library_import_release").map((call) => call.args.name)).toEqual(["Clean Single"]);
  // An import can take a place another release meant to use, so the rest are read again.
  await expect.poll(() => readsOf("Between Spaces")).toBe(2);
  await expect(page.locator(".sync-card").filter({ hasText: "Already copied into your music folder" })).toContainText("Clean Single");

  // The choice is remembered.
  await page.getByRole("group", { name: "Show releases as" }).getByRole("button", { name: "By release" }).click();
  await expect(page.getByRole("button", { name: "Preview Between Spaces" })).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem("opencrate.importView"))).toBe("releases");
});

test("import: in one table a release's heading stays in view sideways, says when nothing was filed, and choices are read once each", async ({ page }) => {
  await page.setViewportSize({ width: 864, height: 800 });
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__cleanReady = true; (window as any).__fileNothing = true; });
  await openImportWithFolders(page, "One table");
  const fresh = page.locator(".sync-card").filter({ hasText: "New since you added the folder" });
  const clean = fresh.locator("tbody.plan-group").filter({ hasText: "Clean Single" });
  const button = clean.getByRole("button", { name: "Import Clean Single" });
  await expect(button).toBeVisible();
  const wrap = fresh.locator(".plan-table-wrap");
  await button.scrollIntoViewIfNeeded();
  await wrap.evaluate((element) => { element.scrollLeft = element.scrollWidth; });
  await expect(button).toBeInViewport();
  const [buttonBox, wrapBox] = await Promise.all([button.boundingBox(), wrap.boundingBox()]);
  expect(buttonBox && wrapBox && buttonBox.x >= wrapBox.x && buttonBox.x + buttonBox.width <= wrapBox.x + wrapBox.width).toBe(true);

  await button.click();
  await expect(clean.getByRole("status")).toContainText("Nothing was imported. Left where they are: Clean Single/01.aif (A file is already there.).");
  await expect(button).toHaveCount(0);

  // A refused import reads every release again (it may have been refused for a place another took);
  // changing a choice reads each once more, never twice.
  const reads = async () => (await calls(page)).filter((call) => call.command === "library_plan_release").map((call) => call.args.name);
  await expect.poll(async () => (await reads()).length).toBe(6);
  await page.getByRole("checkbox", { name: /Tidy names/ }).click();
  await expect.poll(async () => (await reads()).length).toBe(9);
  await page.waitForTimeout(3500);
  const names = ["Between Spaces", "Clean Single", "Old Favourites"];
  expect((await reads()).sort()).toEqual([...names, ...names, ...names].sort());
});

test("import: a long list of imported releases waits behind Show all", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__manyImported = 8; });
  await openImportWithFolders(page, "One table");
  const done = page.locator(".sync-card").filter({ hasText: "Already copied into your music folder" });
  await expect(done.locator("li.folder-row")).toHaveCount(5);
  await done.getByRole("button", { name: "Show all 8" }).click();
  await expect(done.locator("li.folder-row")).toHaveCount(8);
  await done.getByRole("button", { name: "Show fewer" }).click();
  await expect(done.locator("li.folder-row")).toHaveCount(5);
});

test("import all: a review lists doubtful tags, then every finished release is imported and scanned once", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__twoReady = true; });
  await openImportWithFolders(page);
  await page.getByRole("button", { name: "Import all 2 finished releases..." }).click();
  const review = page.getByRole("region", { name: "Before importing" });
  await expect(review).toContainText("3 files from 2 releases will be imported. 1 file stays in the download folder");
  await expect(review.getByRole("heading", { name: "2 files have tags worth a look" })).toBeVisible();
  await expect(review.getByText("The year looks wrong.")).toBeVisible();
  await expect(review.getByText("No track in the tags; taken from the file name.")).toBeVisible();
  await review.getByRole("button", { name: "Cancel" }).click();
  await expect(review).toHaveCount(0);
  expect((await calls(page)).filter((call) => call.command === "library_import_release")).toHaveLength(0);

  await page.getByRole("button", { name: "Import all 2 finished releases..." }).click();
  await page.getByRole("region", { name: "Before importing" }).getByRole("button", { name: "Import 3 files" }).click();
  await expect(page.getByText("3 files are now in your music folder. 1 file was left in the download folder")).toBeVisible();
  const all = await calls(page);
  expect(all.filter((call) => call.command === "library_import_release").map((call) => call.args.name)).toEqual(["Between Spaces", "Second Light"]);
  expect(all.filter((call) => call.command === "library_scan")).toHaveLength(1);
  const imported = page.locator(".sync-card").filter({ has: page.getByRole("heading", { name: "Imported" }) });
  await expect(imported.getByText("Second Light", { exact: true })).toBeVisible();
});

test("import all: releases already there are brought in together, without a preview each", async ({ page }) => {
  await stubBridge(page, "managed");
  await openImportWithFolders(page);
  const older = page.locator(".sync-card").filter({ has: page.getByRole("heading", { name: "Already there when you added the folder" }) });
  await older.getByRole("button", { name: "Import all 1 release already there..." }).click();
  await older.getByRole("region", { name: "Before importing" }).getByRole("button", { name: /^Import \d+ files?$/ }).click();
  await expect(older.getByText(/now in your music folder/)).toBeVisible();
  const imports = (await calls(page)).filter((call) => call.command === "library_import_release").map((call) => call.args.name);
  expect(imports).toEqual(["Old Favourites"]);
});

test("import all: a lookup on request suggests values, and only ticked files send them", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__twoReady = true; });
  await openImportWithFolders(page);
  await page.getByRole("button", { name: "Import all 2 finished releases..." }).click();
  const review = page.getByRole("region", { name: "Before importing" });
  expect((await calls(page)).filter((call) => call.command === "library_lookup_release")).toHaveLength(0);
  await review.getByRole("button", { name: "Look up 2 releases" }).click();
  const found = review.getByRole("group", { name: "Lookup: Between Spaces" });
  await expect(found).toContainText('Found on MusicBrainz: "Between Spaces" by Studio 001 (2019, Test Label), 2 tracks.');
  await expect(found.getByText("Night Drive", { exact: true })).toBeVisible();
  await expect(review.getByRole("group", { name: "Lookup: Second Light" })).toContainText("MusicBrainz is busy right now");
  await found.getByRole("checkbox", { name: /03\.flac/ }).uncheck();
  await review.getByRole("button", { name: "Import 3 files" }).click();
  await expect(page.getByText("3 files are now in your music folder.")).toBeVisible();
  const sent = (await calls(page)).filter((call) => call.command === "library_import_release");
  expect(sent[0]!.args.options.accepted).toEqual([{ source: "Between Spaces/01.flac", title: "Night Drive", year: 2019 }]);
  expect(sent[1]!.args.options.accepted).toBeUndefined();
  expect((await calls(page)).filter((call) => call.command === "library_lookup_release").map((call) => call.args.name)).toEqual(["Between Spaces", "Second Light"]);
});

test("settings: lookup keys are saved, shown only by their ending, refused when bad, and removed", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  const discogs = dialog.locator("#key-discogs");
  await expect(discogs).toHaveAttribute("type", "password");
  await expect(discogs).toHaveAttribute("placeholder", "Paste it here");
  const state = discogs.locator("xpath=../following-sibling::small[1]");
  await expect(state).toHaveText("Not set");
  await discogs.fill("bad");
  await discogs.locator("xpath=..").getByRole("button", { name: "Save" }).click();
  await expect(dialog.getByRole("alert")).toContainText("Discogs did not accept that token");
  await discogs.fill("my-token-wxyz");
  await discogs.locator("xpath=..").getByRole("button", { name: "Save" }).click();
  await expect(dialog.getByRole("status").filter({ hasText: "Discogs accepted the token." })).toBeVisible();
  await expect(state).toHaveText("Saved, ending in wxyz");
  await expect(discogs).toHaveValue("");
  await dialog.getByRole("button", { name: "Remove the Discogs personal token" }).click();
  await expect(state).toHaveText("Not set");
});

test("library check: untidy folder names and recordings filed twice are listed, read only", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("button", { name: "Check library" }).click();
  const results = dialog.getByRole("region", { name: "Library check results" });
  await expect(results).toContainText("Looked at 10 files.");
  await expect(results.getByRole("heading", { name: "1 folder name looks untidy" })).toBeVisible();
  await expect(results).toContainText("Studio 001; would be Studio 001");
  await expect(results.getByRole("heading", { name: "1 recording is filed more than once" })).toBeVisible();
  await expect(results.getByText("Studio 001;Guest/Between Spaces/02 - Soft Focus.aif")).toBeVisible();
  await expect(results.getByRole("heading", { name: "1 file could not be read" })).toBeVisible();
  await expect(results.getByRole("list", { name: "Files that could not be read" })).toContainText("Music: not an audio file");
  const sent = (await calls(page)).map((call) => call.command);
  expect(sent.filter((command) => /move|rename|delete|remove/.test(command))).toEqual([]);
});

test("library check: copies in different music folders say which folder each is in", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__twoFolders = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("button", { name: "Check library" }).click();
  const results = dialog.getByRole("region", { name: "Library check results" });
  const copies = results.locator("li li");
  await expect(copies).toHaveText(["Music: Studio 001/Between Spaces/02 - Soft Focus.aif", "Music2: Studio 001;Guest/Between Spaces/02 - Soft Focus.aif"]);
});

test("library check: a duplicate is resolved by keeping the analysed copy, after a look at each", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__rbWriting = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("button", { name: "Check library" }).click();
  const results = dialog.getByRole("region", { name: "Library check results" });
  await results.getByRole("button", { name: "Resolve Soft Focus" }).click();
  const panel = results.getByRole("region", { name: "Which Soft Focus to keep?" });
  const first = panel.getByRole("radio", { name: /Studio 001\/Between Spaces\/02 - Soft Focus.aif/ });
  await expect(first).toBeChecked();
  await expect(panel).toContainText("AIFF, in rekordbox, analysed, 4 cues, in 2 playlists");
  await expect(panel).toContainText("MP3, 320 kbps, in rekordbox, analysed, in 1 playlist");
  await panel.getByRole("button", { name: "Keep this copy" }).click();
  await expect(results.getByRole("status")).toContainText("1 copy moved to /Users/dj/Music/openCrate archive/Music; 1 merged into the kept copy in rekordbox.");
  expect(await page.evaluate(() => (window as any).__resolved)).toEqual({ keep: 11, drop: [12] });
  await expect(results.getByRole("heading", { name: /filed more than once/ })).toHaveCount(0);
});

test("library check: an untidy folder is tidied after a confirmation, and rekordbox follows", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("button", { name: "Check library" }).click();
  const results = dialog.getByRole("region", { name: "Library check results" });
  await results.getByRole("button", { name: "Tidy Studio 001;" }).click();
  const confirm = results.getByRole("region", { name: "Tidy Studio 001;?" });
  await expect(confirm).toContainText("1 file moves to Studio 001");
  // Writing off: refused, nothing moved.
  await confirm.getByRole("button", { name: "Tidy" }).click();
  await expect(results.getByRole("alert")).toContainText("Turn it on in Settings");
  expect(await page.evaluate(() => (window as any).__tidied)).toBeUndefined();

  await page.evaluate(() => { (window as any).__rbWriting = true; });
  await confirm.getByRole("button", { name: "Tidy" }).click();
  await expect(results.getByRole("status")).toContainText("Moved 1 file to Studio 001; rekordbox was told where 1 went.");
  await expect(results.getByRole("heading", { name: /folder name/ })).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__tidied)).toEqual([{ rootId: 7, folder: "Studio 001;" }]);
});

test("player: double-click plays, Space pauses, seek and volume work, and the next track follows", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const bar = page.getByRole("contentinfo", { name: "Player" });
  await expect(bar.getByRole("button", { name: "Play" })).toBeDisabled();
  await page.getByText("Rust Track 0002").dblclick();
  await expect(bar.getByRole("button", { name: "Pause" })).toBeVisible();
  await expect(bar).toContainText("Rust Track 0002");
  await expect(bar.getByText("3:20")).toBeVisible();
  let loads = (await calls(page)).filter((call) => call.command === "player_load");
  expect(loads.map((call) => call.args.trackId)).toEqual([2]);

  const wave = bar.getByTestId("waveform");
  await expect(wave.locator("rect.band-low")).toHaveCount(320);
  await expect(wave.locator("rect.band-high")).toHaveCount(320);
  // The close-up scrolls while the track plays.
  const scope = page.getByTestId("scope");
  await expect(scope).toBeVisible();
  const snapshot = () => scope.evaluate((canvas: HTMLCanvasElement) => {
    const data = canvas.getContext("2d")!.getImageData(0, 0, canvas.width, canvas.height).data;
    let painted = 0;
    let sum = 0;
    for (let i = 3; i < data.length; i += 4) if (data[i]! > 0) { painted += 1; sum += i % 9973; }
    return { painted, sum };
  });
  await expect.poll(async () => (await snapshot()).painted).toBeGreaterThan(0);
  const first = await snapshot();
  await expect.poll(async () => (await snapshot()).sum).not.toBe(first.sum);
  await page.screenshot({ path: "test-results/player-waveform.png" });
  await expect(wave.locator(".cue-mark")).toHaveCount(2);
  await expect(wave.locator(".cue-mark.hot")).toHaveAttribute("title", "Hot cue B: Drop");
  await expect(wave.locator(".cue-mark").last()).toHaveAttribute("style", /left: 30\.125%/);

  await page.keyboard.press("Space");
  await expect(bar.getByRole("button", { name: "Play" })).toBeVisible();
  await page.keyboard.press("Space");
  await expect(bar.getByRole("button", { name: "Pause" })).toBeVisible();

  const position = bar.getByRole("slider", { name: "Position" });
  await position.focus();
  for (let step = 0; step < 8; step += 1) await page.keyboard.press("PageUp");
  await expect.poll(async () => (await calls(page)).filter((call) => call.command === "player_control" && call.args.action === "seek").length).toBeGreaterThan(0);

  await bar.getByRole("slider", { name: "Volume" }).fill("0.3");
  await expect.poll(() => page.evaluate(() => (window as any).__volume)).toBe(0.3);

  await bar.getByRole("button", { name: "Next track" }).click();
  await expect(bar).toContainText("Rust Track 0003");
  await expect(bar.getByRole("alert")).toContainText("cannot be found");
  // The track before must not carry on under the new title.
  await expect(bar.getByRole("button", { name: "Play" })).toBeVisible();
  expect((await calls(page)).filter((call) => call.command === "player_control" && call.args.action === "stop")).toHaveLength(1);
  await bar.getByRole("button", { name: "Next track" }).click();
  await expect(bar).toContainText("Rust Track 0004");
  await page.evaluate(() => { (window as any).__shortTracks = true; });
  await expect(bar).toContainText("Rust Track 0005", { timeout: 6000 });
  loads = (await calls(page)).filter((call) => call.command === "player_load");
  expect(loads.map((call) => call.args.trackId)).toEqual([2, 3, 4, 5]);

  await page.reload();
  await expect(page.getByRole("contentinfo", { name: "Player" }).getByRole("slider", { name: "Volume" })).toHaveValue("0.3");
});

test("player: a click anywhere on the overview seeks there, and the bar never shows the old spot again", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByText("Rust Track 0002").dblclick();
  const bar = page.getByRole("contentinfo", { name: "Player" });
  await expect(bar.getByRole("button", { name: "Pause" })).toBeVisible();
  const wave = bar.getByTestId("waveform");
  const box = (await wave.boundingBox())!;
  const seeks = async () => (await calls(page)).filter((call) => call.command === "player_control" && call.args.action === "seek").map((call) => call.args.positionMs);

  // Some browsers click on whole pixels: within a pixel and a half of the aim is exact.
  const pixel = (200_000 / box.width) * 1.5;
  const near = (ms: number | undefined, aim: number) => ms !== undefined && Math.abs(ms - aim) <= pixel;

  // Near the start, where the playhead is: 5% of 200 seconds.
  await page.mouse.click(box.x + box.width * 0.05, box.y + box.height / 2);
  await expect.poll(async () => near((await seeks())[0], 10_000)).toBe(true);
  expect(await seeks()).toHaveLength(1);

  // Halfway, with a seek that takes a while to land.
  await page.evaluate(() => { (window as any).__slowSeek = true; });
  await page.mouse.click(box.x + box.width * 0.5, box.y + box.height / 2);
  const clockText = bar.locator(".seek span").first();
  const seen = new Set<string>();
  for (let i = 0; i < 12; i += 1) {
    seen.add((await clockText.textContent()) ?? "");
    await page.waitForTimeout(100);
  }
  expect([...seen].every((text) => text >= "1:39"), [...seen].join(" ")).toBe(true);
  expect(near((await seeks()).at(-1), 100_000)).toBe(true);
});

test("player: a row's play button, a queue, and dropping a track on the player", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const bar = page.getByRole("contentinfo", { name: "Player" });
  const now = bar.locator(".now-playing");
  const loads = async () => (await calls(page)).filter((call) => call.command === "player_load").map((call) => call.args.trackId);
  const row = (n: string) => page.getByTestId("track-row").filter({ hasText: `Rust Track ${n}` });

  // The playing row shows it, in place of its number.
  await row("0005").dblclick();
  await expect(row("0005").getByRole("img", { name: "Now playing" })).toBeVisible();
  await bar.getByRole("button", { name: "Pause" }).click();
  await expect(row("0005").getByRole("img", { name: "Paused" })).toBeVisible();
  await bar.getByRole("button", { name: "Play" }).click();

  // Hovering a row brings up a play button at its left.
  await row("0004").hover();
  const play = page.getByRole("button", { name: "Play Rust Track 0004" });
  await expect(play).toBeVisible();
  await play.click();
  await expect(now).toContainText("Rust Track 0004");

  // Right-click to queue: "Play next" goes in front, "Add to queue" at the end.
  for (const [n, choice] of [["0010", "Add to queue"], ["0012", "Add to queue"], ["0008", "Play next"]] as const) {
    await row(n).click({ button: "right" });
    await page.getByRole("menu", { name: `Rust Track ${n}` }).getByRole("menuitem", { name: choice }).click();
  }
  await bar.getByRole("button", { name: "Queue, 3 tracks" }).click();
  const queue = page.getByRole("region", { name: "Queue" });
  await expect(queue.getByRole("listitem")).toHaveText([/Rust Track 0008/, /Rust Track 0010/, /Rust Track 0012/]);
  await queue.getByRole("button", { name: "Remove Rust Track 0010 from the queue" }).click();
  await expect(queue.getByRole("listitem")).toHaveCount(2);

  // Queued tracks play first; then the list carries on from where it was left.
  await bar.getByRole("button", { name: "Next track" }).click();
  await expect(now).toContainText("Rust Track 0008");
  await bar.getByRole("button", { name: "Next track" }).click();
  await expect(now).toContainText("Rust Track 0012");
  await expect(queue).toContainText("Nothing queued");
  await bar.getByRole("button", { name: "Next track" }).click();
  await expect(now).toContainText("Rust Track 0005");
  expect(await loads()).toEqual([5, 4, 8, 12, 5]);

  // A track dropped on the player plays.
  await row("0007").dragTo(bar);
  await expect(now).toContainText("Rust Track 0007");
});

test("player: a release, artist, crate or playlist plays from its first playable track", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Releases", exact: true }).click();
  await page.getByTestId("library-card").first().click();
  await page.getByRole("button", { name: "Play all" }).click();
  const bar = page.getByRole("contentinfo", { name: "Player" });
  // Track 1 is missing from disk, so the list starts at track 2.
  await expect(bar.locator(".now-playing")).toContainText("Rust Track 0002");
  await bar.getByRole("button", { name: "Next track" }).click();
  await expect(bar.locator(".now-playing")).toContainText("Rust Track 0003");
});

test("editing: tracks chosen together are edited at once, with cleanup shown before saving", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const row = (n: string) => page.getByTestId("track-row").filter({ hasText: `Rust Track ${n}` });
  await row("0002").click();
  await row("0004").click({ modifiers: ["ControlOrMeta"] });
  await expect(row("0002")).toHaveAttribute("aria-pressed", "true");
  await expect(row("0004")).toHaveAttribute("aria-pressed", "true");
  await expect(row("0003")).toHaveAttribute("aria-pressed", "false");

  await row("0004").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Edit details (2 tracks)..." }).click();
  const editor = page.getByRole("dialog", { name: "Edit 2 tracks" });
  await expect(editor.getByLabel("Genre")).toHaveValue("House");
  await expect(editor.getByLabel("Title")).toHaveAttribute("placeholder", "Several values");
  await expect(editor.getByLabel("Track number")).toHaveCount(0);

  await editor.getByLabel("Genre").fill("Deep House");
  await editor.getByRole("button", { name: "Remove number prefixes" }).click();
  await editor.getByRole("button", { name: "Fix casing" }).click();
  const preview = editor.getByRole("list", { name: "Cleanup changes" });
  await expect(preview).toContainText("Title: 02 - RUST TRACK 0002 -> Rust Track 0002");
  await expect(preview).toContainText("Title: 04 - RUST TRACK 0004 -> Rust Track 0004");
  await editor.getByRole("button", { name: "Save 3 changes" }).click();
  await expect(editor).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__edits)).toEqual([
    { trackIds: [2, 4], changes: [{ field: "genre", value: "Deep House" }] },
  ]);
  expect(await page.evaluate(() => (window as any).__editEach)).toEqual([
    { trackId: 2, changes: [{ field: "title", value: "Rust Track 0002" }] },
    { trackId: 4, changes: [{ field: "title", value: "Rust Track 0004" }] },
  ]);

  // Shift-click takes a range; Cmd+I opens the editor for it.
  await row("0002").click();
  await row("0005").click({ modifiers: ["Shift"] });
  for (const n of ["0002", "0003", "0004", "0005"]) await expect(row(n)).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("ControlOrMeta+KeyI");
  await expect(page.getByRole("dialog", { name: "Edit 4 tracks" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Edit 4 tracks" })).toHaveCount(0);
});

test("editing: custom fields are kept per track and shared across a selection", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const row = (n: string) => page.getByTestId("track-row").filter({ hasText: `Rust Track ${n}` });
  await row("0002").click();
  await row("0004").click({ modifiers: ["ControlOrMeta"] });
  await page.keyboard.press("ControlOrMeta+KeyI");
  const editor = page.getByRole("dialog", { name: "Edit 2 tracks" });
  const fields = editor.getByRole("group", { name: "Your fields" });
  await fields.getByLabel("New field name").fill("Energy");
  await fields.getByLabel("New field value").fill("7");
  await fields.getByRole("button", { name: "Add" }).click();
  await expect(fields.getByLabel("Energy value")).toHaveValue("7");
  await page.keyboard.press("Escape");

  await row("0004").click();
  await page.keyboard.press("ControlOrMeta+KeyI");
  const one = page.getByRole("dialog", { name: "Edit details" });
  await one.getByRole("group", { name: "Your fields" }).getByLabel("Energy value").fill("9");
  await one.getByRole("group", { name: "Your fields" }).getByLabel("Energy value").blur();
  await page.keyboard.press("Escape");
  await row("0002").click();
  await row("0004").click({ modifiers: ["ControlOrMeta"] });
  await page.keyboard.press("ControlOrMeta+KeyI");
  await expect(page.getByRole("dialog", { name: "Edit 2 tracks" }).getByLabel("Energy value")).toHaveAttribute("placeholder", "Several values");
  expect(await page.evaluate(() => (window as any).__custom)).toEqual([{ trackId: 2, name: "Energy", value: "7" }, { trackId: 4, name: "Energy", value: "9" }]);
});

test("editing: a recipe saves typed values and cleanup rules and applies them again", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const row = (n: string) => page.getByTestId("track-row").filter({ hasText: `Rust Track ${n}` });
  await row("0002").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Edit details..." }).click();
  const editor = page.getByRole("dialog", { name: "Edit details" });
  await editor.getByLabel("Genre").fill("Techno");
  await editor.getByRole("button", { name: "Remove number prefixes" }).click();
  await editor.getByRole("button", { name: "Save as recipe..." }).click();
  await editor.getByLabel("Recipe name").fill("Techno import");
  await editor.getByRole("button", { name: "Save", exact: true }).click();
  await page.keyboard.press("Escape");

  await row("0004").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Edit details..." }).click();
  const again = page.getByRole("dialog", { name: "Edit details" });
  await again.getByLabel("Apply a recipe").selectOption("Techno import");
  await expect(again.getByLabel("Genre")).toHaveValue("Techno");
  await expect(again.getByRole("list", { name: "Cleanup changes" })).toContainText("Title: 04 - RUST TRACK 0004 -> RUST TRACK 0004");
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("opencrate.editRecipes") ?? "[]"))).toEqual([{ name: "Techno import", values: { genre: "Techno" }, rules: ["numbers"] }]);
});

test("clean up: genres spelled alike are merged, and titles and artists are fixed after a look", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Clean up" }).click();
  const genres = page.getByRole("region", { name: "Genres" });
  await expect(genres.getByRole("list", { name: "Genres spelled alike" })).toContainText("Deep House, deep-house");
  await expect(genres.getByLabel("New name for Deep House, deep-house")).toHaveValue("Deep House");
  await genres.getByRole("button", { name: "Merge" }).click();
  await expect(genres.getByRole("status")).toContainText("2 files now have the genre Deep House.");
  expect(await page.evaluate(() => (window as any).__genresMerged)).toEqual({ from: ["deep-house"], to: "Deep House" });
  await expect(genres).toContainText("No genres are spelled two ways.");
  // The merge is the newest save in Recent changes at once, ready to undo.
  await expect(page.getByRole("region", { name: "Recent changes" }).getByRole("button", { name: /^Undo: genre of 2 tracks/ })).toBeVisible();

  const titles = page.getByRole("region", { name: "Titles and artists" });
  await titles.getByRole("button", { name: "Check titles and artists" }).click();
  const found = titles.getByRole("list", { name: "Proposed fixes" });
  await expect(found).toContainText("Title: 01 - Night Drive -> Night Drive");
  await expect(found).toContainText("Artist: Unknown Artist -> Studio 001");
  await expect(found.getByRole("listitem")).toHaveCount(2);
  await titles.getByRole("button", { name: "Clean up 2 tracks" }).click();
  await expect(titles.getByRole("status")).toContainText("2 tracks were cleaned up.");
  expect(await page.evaluate(() => (window as any).__editEach)).toEqual([
    { trackId: 1, changes: [{ field: "title", value: "Night Drive" }] },
    { trackId: 3, changes: [{ field: "title", value: "Late Train" }, { field: "artist", value: "Studio 001" }] },
  ]);
});

test("clean up: a change of capitals alone starts unticked, and the same fix on two tracks is one row", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__moreNames = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "Clean up" }).click();
  const titles = page.getByRole("region", { name: "Titles and artists" });
  await titles.getByRole("button", { name: "Check titles and artists" }).click();
  const found = titles.getByRole("list", { name: "Proposed fixes" });
  await expect(found.getByRole("listitem")).toHaveCount(3);
  const night = found.getByRole("listitem").filter({ hasText: "Title: 01 - Night Drive -> Night Drive" });
  await expect(night).toContainText("2 tracks");
  await expect(night.getByRole("checkbox")).toBeChecked();
  const caps = found.getByRole("listitem").filter({ hasText: "Title: MAINTAIN -> Maintain" });
  await expect(caps.getByRole("checkbox")).not.toBeChecked();
  await expect(titles).toContainText("1 suggestion only changes capitals and starts unticked");
  await titles.getByRole("button", { name: "Clean up 3 tracks" }).click();
  const sent = await page.evaluate(() => (window as any).__editEach as { trackId: number }[]);
  expect(sent.map((edit) => edit.trackId).sort()).toEqual([1, 3, 5]);
});

test("a crate is exported as an M3U8 or CSV to a file the owner picks", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const crates = page.getByRole("navigation", { name: "Crates" });
  await crates.getByRole("button", { name: "New crate" }).click();
  await crates.getByRole("textbox", { name: "New crate name" }).fill("Warm-up");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Warm-up" })).toBeVisible();
  // An empty crate has nothing to export.
  await expect(page.getByText("This crate is empty")).toBeVisible();
  await expect(page.getByRole("group", { name: "Export" })).toHaveCount(0);
  await page.getByRole("button", { name: "Back to tracks" }).click();
  await page.getByRole("button", { name: /Rust Track 0001,/ }).dragTo(crates.getByRole("button", { name: /Warm-up/ }));
  await crates.getByRole("button", { name: /Warm-up/ }).click();
  const exportGroup = page.getByRole("group", { name: "Export" });
  await exportGroup.getByRole("button", { name: "Export M3U8 playlist..." }).click();
  await expect(exportGroup.getByRole("status")).toContainText("3 tracks exported to /Users/dj/Desktop/Warm-up.m3u8. 1 has no file that can be reached.");
  await exportGroup.getByRole("button", { name: "Export CSV..." }).click();
  const sent = await page.evaluate(() => (window as any).__exports);
  expect(sent).toHaveLength(2);
  expect(sent[0].format).toBe("m3u8");
  expect(sent[0].name).toBe("Warm-up");
  expect(sent[0].query.crateId).toBeTruthy();
  expect(sent[1].format).toBe("csv");
});

test("an empty library says there is no music yet, points to Settings and Import, and has nothing to export", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__emptyLibrary = true; });
  await page.goto("/");
  const empty = page.locator(".empty-state");
  await expect(empty.getByRole("heading", { name: "No music in your library yet" })).toBeVisible();
  await expect(page.getByRole("group", { name: "Export" })).toHaveCount(0);
  await empty.getByRole("button", { name: "Open Settings" }).click();
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeHidden();
  await empty.getByRole("button", { name: "Go to Import" }).click();
  await expect(page.getByRole("heading", { name: "Import", level: 1 })).toBeVisible();
  // Once something is typed, an empty result is about the search, not about the library.
  await page.getByRole("button", { name: "Tracks" }).first().click();
  await page.getByRole("textbox", { name: "Search library" }).fill("night");
  await expect(page.locator(".empty-state").getByRole("heading", { name: "No matches this time" })).toBeVisible();
});

test("settings: a long download folder path is shown whole, on a line of its own, above its buttons", async ({ page }) => {
  await page.setViewportSize({ width: 864, height: 700 });
  await stubBridge(page, "managed");
  const path = "/Users/dj/Library/Mobile Documents/com~apple~CloudDocs/Music Downloads/New Releases 2026";
  await page.addInitScript((value) => { (window as any).__downloadPath = value; }, path);
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("button", { name: "Add download folder..." }).click();
  const row = dialog.locator(".folder-row").filter({ hasText: "Downloads" });
  const shown = row.locator("small[title]");
  await expect(shown).toHaveText(path);
  expect(await shown.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
  // The buttons sit below the path, inside the dialog, and Remove is still reachable.
  const remove = row.getByRole("button", { name: "Remove Downloads from download folders" });
  await expect(remove).toBeInViewport();
  const [pathBox, removeBox, dialogBox] = await Promise.all([shown.boundingBox(), remove.boundingBox(), dialog.boundingBox()]);
  expect(pathBox && removeBox && removeBox.y >= pathBox.y + pathBox.height - 1).toBe(true);
  expect(removeBox && dialogBox && removeBox.x + removeBox.width <= dialogBox.x + dialogBox.width).toBe(true);
});

test("import: columns past the edge of a narrow preview are pointed out, and the hint goes when they fit", async ({ page }) => {
  await page.setViewportSize({ width: 864, height: 700 });
  await stubBridge(page, "managed");
  await openImportWithFolders(page);
  const table = page.locator(".plan-table-wrap");
  await expect(table).toBeVisible();
  expect(await table.evaluate((element) => element.scrollWidth > element.clientWidth)).toBe(true);
  const hint = page.getByRole("note").filter({ hasText: "More columns are off to the right" });
  await expect(hint).toBeVisible();
  await expect(page.locator(".plan-table-frame")).toHaveAttribute("data-more-right", "true");
  // Scrolled to the far end, nothing is left to find.
  await table.evaluate((element) => { element.scrollLeft = element.scrollWidth; });
  await expect(page.locator(".plan-table-frame")).toHaveAttribute("data-more-right", "false");
  // A wide window shows every column.
  await page.setViewportSize({ width: 2000, height: 700 });
  await expect(hint).toHaveCount(0);
});

test("keys are shown in the chosen notation", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const row = page.getByTestId("track-row").filter({ hasText: "Rust Track 0002" });
  await expect(row.locator(".track-key")).toHaveText("8A");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("radio", { name: "Open Key (1m)" }).check();
  await page.keyboard.press("Escape");
  await expect(row.locator(".track-key")).toHaveText("1m");
  await page.reload();
  await expect(page.getByTestId("track-row").filter({ hasText: "Rust Track 0002" }).locator(".track-key")).toHaveText("1m");
});

test("a release page can choose, find or remove its cover", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Releases", exact: true }).click();
  await page.getByTestId("library-card").first().click();
  const cover = page.getByRole("group", { name: "Cover" });
  await cover.getByRole("button", { name: "Remove cover" }).click();
  await expect(cover.getByRole("status")).toContainText("The cover was removed from 2 files.");
  await cover.getByRole("button", { name: "Choose cover..." }).click();
  const sent = (await calls(page)).filter((call) => /release_art/.test(call.command)).map((call) => [call.command, call.args.releaseId]);
  expect(sent).toEqual([["library_remove_release_art", 1], ["library_set_release_art", 1]]);
});

test("clean up: crates are compared, sorted and merged", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const crates = page.getByRole("navigation", { name: "Crates" });
  for (const [name, tracks] of [["Warm-up", ["0003", "0001", "0002"]], ["Peak", ["0002", "0005"]]] as const) {
    await crates.getByRole("button", { name: "New crate" }).click();
    await crates.getByRole("textbox", { name: "New crate name" }).fill(name);
    await page.keyboard.press("Enter");
    await page.getByRole("button", { name: "Tracks", exact: true }).click();
    for (const n of tracks) await page.getByRole("button", { name: new RegExp(`Rust Track ${n},`) }).dragTo(crates.getByRole("button", { name: new RegExp(name) }));
  }
  await page.getByRole("button", { name: "Clean up" }).click();
  const card = page.getByRole("region", { name: "Crates" });
  await card.getByLabel("First").selectOption({ label: "Warm-up" });
  await card.getByLabel("Second").selectOption({ label: "Peak" });
  await card.getByRole("button", { name: "Compare" }).click();
  const comparison = card.getByRole("region", { name: "Comparison" });
  await expect(comparison.getByRole("heading", { name: "In both (1)" })).toBeVisible();
  await expect(comparison.getByRole("heading", { name: "Only in Warm-up (2)" })).toBeVisible();
  await expect(comparison.getByRole("heading", { name: "Only in Peak (1)" })).toBeVisible();
  // Comparing the other way round leaves Merge and Sort as they were.
  await card.getByLabel("First").selectOption({ label: "Peak" });
  await card.getByLabel("Second").selectOption({ label: "Warm-up" });
  await expect(card.getByLabel("Into")).toHaveValue("1");
  await expect(card.getByLabel("From")).toHaveValue("2");
  await expect(card.getByLabel(/^Crate/)).toHaveValue("1");
  await card.getByLabel("Order").selectOption("bpm");
  await card.getByRole("button", { name: "Sort crate" }).click();
  await expect(card.getByRole("status")).toContainText("Warm-up is now in BPM order.");
  // A comparison of crates as they were is gone once one changes.
  await expect(card.getByRole("region", { name: "Comparison" })).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__sorted)).toEqual({ crateId: 1, sort: "bpm" });
  await card.getByLabel("From").selectOption({ label: "Peak" });
  await card.getByRole("button", { name: "Merge" }).click();
  await expect(card.getByRole("status")).toContainText("1 track added to Warm-up; Peak removed.");
  await expect(crates.getByRole("button", { name: /Peak/ })).toHaveCount(0);
});

test("clean up: the newest save is undone and the list moves on", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Clean up" }).click();
  const recent = page.getByRole("region", { name: "Recent changes" });
  await expect(recent.getByRole("list", { name: "Saves" }).getByRole("listitem")).toHaveCount(2);
  await recent.getByRole("button", { name: /^Undo: title of 1 track/ }).click();
  await expect(recent.getByRole("status")).toContainText("1 file is back as before.");
  await expect(recent.getByRole("button", { name: /^Undo: genre, label of 2 tracks/ })).toBeVisible();
  await recent.getByRole("button", { name: /^Undo: genre, label of 2 tracks/ }).click();
  await expect(recent).toContainText("No changes yet.");
});

test("clean up: releases without a cover are listed and filled from beside their files", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Clean up" }).click();
  const art = page.getByRole("region", { name: "Album art" });
  await art.getByRole("button", { name: "Check album art" }).click();
  await expect(art).toContainText("2 releases have no cover; 1 has an image beside the files.");
  const list = art.getByRole("list", { name: "Releases without art" });
  await expect(list.getByRole("listitem")).toHaveCount(2);
  await art.getByRole("button", { name: "Use the folder image for Between Spaces" }).click();
  await expect(list.getByRole("listitem")).toHaveCount(1);
  await art.getByRole("button", { name: "Find a cover online for No Light" }).click();
  await expect(list).toContainText("The Cover Art Archive has no cover");
  await art.getByRole("button", { name: "Choose an image for No Light" }).click();
  await expect(list.getByRole("listitem")).toHaveCount(1);
  const sent = (await calls(page)).filter((call) => call.command === "library_set_release_art").map((call) => call.args);
  expect(sent).toEqual([{ releaseId: 2, source: "folder" }, { releaseId: 3, source: "online" }, { releaseId: 3, source: "choose" }]);
});

test("player: at the end of a track an unplayable next one is skipped", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__shortTracks = true; });
  await page.goto("/");
  await page.getByText("Rust Track 0002").dblclick();
  const bar = page.getByRole("contentinfo", { name: "Player" });
  await expect(bar).toContainText("Rust Track 0004", { timeout: 6000 });
  await expect(bar.getByRole("button", { name: "Pause" })).toBeVisible();
  const loads = (await calls(page)).filter((call) => call.command === "player_load").map((call) => call.args.trackId);
  expect(loads.slice(0, 3)).toEqual([2, 3, 4]);
});

test("player: a status left over from the previous track does not skip the new one", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.evaluate(() => { (window as any).__staleStatus = 2; });
  await page.getByText("Rust Track 0002").dblclick();
  const bar = page.getByRole("contentinfo", { name: "Player" });
  await expect(bar.getByRole("button", { name: "Pause" })).toBeVisible();
  await expect.poll(() => page.evaluate(() => (window as any).__staleStatus)).toBe(0);
  await page.waitForTimeout(600);
  await expect(bar).toContainText("Rust Track 0002");
  expect((await calls(page)).filter((call) => call.command === "player_load").map((call) => call.args.trackId)).toEqual([2]);
});

test("player: Enter plays the focused track, previous restarts or steps back", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByText("Rust Track 0005").click();
  await page.keyboard.press("Enter");
  const bar = page.getByRole("contentinfo", { name: "Player" });
  await expect(bar).toContainText("Rust Track 0005");
  await bar.getByRole("button", { name: "Previous track" }).click();
  await expect(bar).toContainText("Rust Track 0004");
  await page.waitForTimeout(3300);
  await bar.getByRole("button", { name: "Previous track" }).click();
  await expect.poll(async () => (await calls(page)).filter((call) => call.command === "player_control" && call.args.action === "seek").map((call) => call.args.positionMs)).toEqual([0]);
  await expect(bar).toContainText("Rust Track 0004");
});

test("folder layout: presets and a custom pattern are saved, and a bad one is explained", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await expect(dialog.getByRole("radio", { name: "Artist / Album / Track", exact: true })).toBeChecked();
  await expect(dialog.getByText("Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif")).toBeVisible();
  await dialog.getByRole("radio", { name: "Album / Track", exact: true }).check();
  await expect(dialog.getByText("Between Spaces/01 - Studio 001 - Night Drive.aif", { exact: true })).toBeVisible();
  await dialog.getByLabel("Custom folder layout").fill("{nonsense}/{title}");
  await dialog.getByRole("button", { name: "Use this" }).click();
  await expect(dialog.getByRole("alert")).toContainText("That layout cannot be used");
  await expect(dialog.getByRole("radio", { name: "Album / Track", exact: true })).toBeChecked();
  await dialog.getByLabel("Custom folder layout").fill("{artist}/{title}");
  await dialog.getByRole("button", { name: "Use this" }).click();
  await expect(dialog.getByRole("radio", { name: "Custom" })).toBeChecked();
  const saved = (await calls(page)).filter((call) => call.command === "library_set_folder_layout").map((call) => call.args.layout);
  expect(saved).toEqual(["{release}/{track:02} - {artist} - {title}", "{nonsense}/{title}", "{artist}/{title}"]);
});

test("a track's cues and beat grid are shown as rekordbox wrote them, or explained as missing", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Cues and grid" })).toHaveCount(0);
  await page.getByText("Rust Track 0002").click();
  await page.getByRole("button", { name: "Cues and grid" }).click();
  const dialog = page.getByRole("dialog", { name: "Cues and beat grid" });
  await expect(dialog.getByText("Hot cue B")).toBeVisible();
  await expect(dialog.getByText("0:12.345")).toBeVisible();
  await expect(dialog.getByRole("cell", { name: "Loop", exact: true })).toBeVisible();
  await expect(dialog.getByText("1:04.500")).toBeVisible();
  await expect(dialog.getByText("0:00.025")).toBeVisible();
  await expect(dialog.getByText("128 BPM").first()).toBeVisible();
  await dialog.getByRole("button", { name: "Close cues and grid" }).click();

  await page.evaluate(() => { (window as any).__noAnalysis = true; });
  await page.getByRole("button", { name: "Cues and grid" }).click();
  await expect(page.getByRole("dialog", { name: "Cues and beat grid" })).toContainText("rekordbox has nothing for Rust Track 0002 yet");
  const sent = (await calls(page)).filter((call) => call.command === "library_track_analysis");
  expect(sent.map((call) => call.args)).toEqual([{ trackId: 2 }, { trackId: 2 }]);
});

test("covers are loaded from the library, and a release without one keeps its drawn cover", async ({ page }) => {
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==", "base64");
  let hasCover = true;
  await page.route("**/__artwork/**", (route) => (hasCover
    ? route.fulfill({ status: 200, contentType: "image/png", body: png })
    : route.fulfill({ status: 404, body: "" })));
  await stubBridge(page, "database");
  await page.goto("/");
  const row = page.getByRole("button", { name: /Rust Track 0001/ });
  const image = row.locator("img.art-image");
  await expect(image).toHaveAttribute("src", "/__artwork/2");
  await expect.poll(() => image.evaluate((element) => (element as HTMLImageElement).naturalWidth)).toBe(1);

  hasCover = false;
  await page.reload();
  await expect(page.getByText("Rust Track 0001")).toBeVisible();
  await expect(page.locator("img.art-image")).toHaveCount(0);
  await expect(row.locator(".artwork")).toBeVisible();
});

test("an artist shows a collage of up to four release covers, laid out by how many there are", async ({ page }) => {
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==", "base64");
  await page.route("**/__artwork/**", (route) => route.fulfill({ status: 200, contentType: "image/png", body: png }));
  await stubBridge(page, "database");
  await page.goto("/");
  await page.getByRole("button", { name: "Artists" }).first().click();
  const card = (n: number) => page.getByRole("button", { name: new RegExp(`Rust Track ${String(n).padStart(4, "0")}`) });
  const boxes = (n: number) => card(n).locator("img.art-image").evaluateAll((images) => images.map((image) => {
    const box = image.getBoundingClientRect();
    return { x: Math.round(box.x), y: Math.round(box.y), w: Math.round(box.width), h: Math.round(box.height) };
  }));
  await expect(card(4).locator("img.art-image")).toHaveCount(4);
  await expect(card(1).locator("img.art-image")).toHaveCount(1);
  await expect(card(2).locator("img.art-image")).toHaveCount(2);
  await expect(card(3).locator("img.art-image")).toHaveCount(3);
  await expect(card(5).locator("img.art-image")).toHaveCount(0);

  const [one] = await boxes(1);
  const two = await boxes(2);
  const three = await boxes(3);
  const four = await boxes(4);
  // Two: side by side, each the full height and about half the width.
  expect(two[0]!.y).toBe(two[1]!.y);
  expect(two[0]!.h).toBe(one!.h);
  expect(Math.abs(two[0]!.w * 2 - one!.w)).toBeLessThanOrEqual(2);
  // Three: one across the top, two below.
  expect(Math.abs(three[0]!.w - one!.w)).toBeLessThanOrEqual(1);
  expect(three[1]!.y).toBe(three[2]!.y);
  expect(three[1]!.y).toBeGreaterThan(three[0]!.y);
  // Four: a two by two grid.
  expect(new Set(four.map((box) => box.y)).size).toBe(2);
  expect(new Set(four.map((box) => box.x)).size).toBe(2);
  const sent = (await calls(page)).filter((call) => call.command === "library_artist_covers");
  expect(new Set(sent.map((call) => call.args.artistId)).size).toBe(sent.length);
});

test("crates: make one, drag tracks onto it, reorder by dragging and keyboard, rename and delete", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const crates = page.getByRole("navigation", { name: "Crates" });
  await crates.getByRole("button", { name: "New crate" }).click();
  await crates.getByRole("textbox", { name: "New crate name" }).fill("Warm-up");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Warm-up" })).toBeVisible();
  await expect(page.getByText("0 tracks")).toBeVisible();
  await expect(page.getByRole("heading", { name: "This crate is empty" })).toBeVisible();

  await page.getByRole("button", { name: "Tracks" }).first().click();
  const crate = crates.getByRole("button", { name: /Warm-up/ });
  for (const n of ["0003", "0001", "0002"]) {
    await page.getByRole("button", { name: new RegExp(`Rust Track ${n},`) }).dragTo(crate);
  }
  await expect(crates.getByRole("status")).toContainText("Added 1 to Warm-up.");
  await page.getByRole("button", { name: /Rust Track 0003,/ }).dragTo(crate);
  await expect(crates.getByRole("status")).toContainText("Already in Warm-up.");
  await expect(crate).toContainText("3");
  // The note goes away on its own.
  await expect(crates.getByRole("status")).toHaveCount(0, { timeout: 5000 });

  await crate.click();
  const titles = () => page.getByTestId("track-row").evaluateAll((rows) => rows.map((row) => row.querySelector("strong")?.textContent));
  await expect.poll(titles).toEqual(["Rust Track 0003", "Rust Track 0001", "Rust Track 0002"]);
  await expect(page.getByText("Drag to reorder.")).toBeVisible();

  // Drop 0002 on the upper half of 0003: it goes first.
  await page.getByRole("button", { name: /Rust Track 0002,/ }).dragTo(page.getByRole("button", { name: /Rust Track 0003,/ }), { targetPosition: { x: 40, y: 6 } });
  await expect.poll(titles).toEqual(["Rust Track 0002", "Rust Track 0003", "Rust Track 0001"]);
  // Drop 0002 on the lower half of 0001: it goes last.
  await page.getByRole("button", { name: /Rust Track 0002,/ }).dragTo(page.getByRole("button", { name: /Rust Track 0001,/ }), { targetPosition: { x: 40, y: 50 } });
  await expect.poll(titles).toEqual(["Rust Track 0003", "Rust Track 0001", "Rust Track 0002"]);

  await page.getByRole("button", { name: /Rust Track 0003,/ }).focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect.poll(titles).toEqual(["Rust Track 0001", "Rust Track 0003", "Rust Track 0002"]);
  await page.getByRole("button", { name: /Rust Track 0003,/ }).focus();
  await page.keyboard.press("Delete");
  await expect.poll(titles).toEqual(["Rust Track 0001", "Rust Track 0002"]);

  const moves = (await calls(page)).filter((call) => call.command === "library_move_in_crate").map((call) => call.args);
  expect(moves).toEqual([
    { crateId: 1, trackIds: [2], beforeId: 3 },
    { crateId: 1, trackIds: [2], beforeId: null },
    { crateId: 1, trackIds: [3], beforeId: 2 },
  ]);

  await crates.getByRole("button", { name: /Warm-up/ }).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Rename" }).click();
  await crates.getByRole("textbox", { name: "Rename Warm-up" }).fill("Opening");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Opening" })).toBeVisible();
  await crates.getByRole("button", { name: /Opening/ }).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Delete" }).click();
  await expect(crates.getByRole("alertdialog")).toContainText("The tracks stay in your library.");
  await crates.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(crates.getByRole("button", { name: /Opening/ })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Tracks", exact: true })).toBeVisible();
});

test("arrange: dragging in Tracks switches to Custom order and keeps the move", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const titles = () => page.getByTestId("track-row").evaluateAll((rows) => rows.slice(0, 4).map((row) => row.querySelector("strong")?.textContent));
  await expect.poll(titles).toEqual(["Rust Track 0001", "Rust Track 0002", "Rust Track 0003", "Rust Track 0004"]);
  await page.getByRole("button", { name: /Rust Track 0003,/ }).dragTo(page.getByRole("button", { name: /Rust Track 0001,/ }), { targetPosition: { x: 40, y: 6 } });
  await expect.poll(titles).toEqual(["Rust Track 0003", "Rust Track 0001", "Rust Track 0002", "Rust Track 0004"]);
  await expect(page.getByRole("combobox", { name: "Sort library" })).toHaveValue("custom");

  // Mid-drag, the rows in between slide up to open a gap and the dragged row's own slot closes.
  // The press lands on the row's time column: clear of the play button that appears at the
  // row's left edge on hover (a press on it starts no drag) and of the artwork and text.
  const source = (await page.getByRole("button", { name: /Rust Track 0003,/ }).boundingBox())!;
  const target = (await page.getByRole("button", { name: /Rust Track 0004,/ }).boundingBox())!;
  const grip = source.width - 120;
  await page.mouse.move(source.x + grip, source.y + source.height / 2);
  await page.mouse.down();
  await page.mouse.move(target.x + grip, target.y + target.height * 0.8, { steps: 8 });
  const slide = (name: RegExp) => page.getByRole("button", { name }).evaluate((button) => getComputedStyle(button.parentElement!).transform);
  await expect.poll(() => slide(/Rust Track 0001,/)).toMatch(/matrix\(1, 0, 0, 1, 0, -\d+/);
  await expect(page.getByRole("button", { name: /Rust Track 0003,/ })).toHaveClass(/is-drag-gap/);
  // A drag that ends without a drop leaves nothing slid or hidden.
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect.poll(() => slide(/Rust Track 0001,/)).toBe("none");
  await expect(page.locator(".is-drag-gap, .is-drag-source")).toHaveCount(0);
  await expect.poll(titles).toEqual(["Rust Track 0003", "Rust Track 0001", "Rust Track 0002", "Rust Track 0004"]);
  await page.getByRole("button", { name: /Rust Track 0001,/ }).dragTo(page.getByRole("button", { name: /Rust Track 0002,/ }), { targetPosition: { x: 40, y: 50 } });
  await expect.poll(titles).toEqual(["Rust Track 0003", "Rust Track 0002", "Rust Track 0001", "Rust Track 0004"]);
  const moves = (await calls(page)).filter((call) => call.command === "library_move_tracks").map((call) => call.args);
  expect(moves).toEqual([
    { trackIds: [3], beforeId: 1, fromSort: "recent" },
    { trackIds: [1], beforeId: 4, fromSort: "custom" },
  ]);
  // Inside an artist or a playlist there is no whole-library order to change.
  await page.getByRole("combobox", { name: "Sort library" }).selectOption("title");
  await expect(page.getByRole("option", { name: "Custom order" })).toBeAttached();
});

test("sidebar: in a short window the crates keep room of their own", async ({ page }) => {
  await page.setViewportSize({ width: 1300, height: 600 });
  await stubBridge(page, "managed");
  await page.goto("/");
  const crates = page.getByRole("navigation", { name: "Crates" });
  await expect(crates.getByRole("button", { name: "New crate" })).toBeInViewport();
  const box = await page.locator(".sidebar-lists").boundingBox();
  expect(box && box.height).toBeGreaterThanOrEqual(119);
  // Nothing of the sidebar is drawn over the player.
  const [sidebar, player] = await Promise.all([page.getByRole("complementary").boundingBox(), page.locator(".player-bar").boundingBox()]);
  const hint = await crates.locator("xpath=..").boundingBox();
  expect(sidebar && player && sidebar.y + sidebar.height <= player.y + 1).toBe(true);
  expect(hint).not.toBeNull();
});

test("sidebar: crates are as wide as the navigation, and the width can be dragged and is remembered", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 850 });
  await stubBridge(page, "managed");
  await page.goto("/");
  const crates = page.getByRole("navigation", { name: "Crates" });
  await crates.getByRole("button", { name: "New crate" }).click();
  await crates.getByRole("textbox", { name: "New crate name" }).fill("Short");
  await page.keyboard.press("Enter");
  const navWidth = (await page.getByRole("button", { name: "Tracks" }).first().boundingBox())!.width;
  const crateWidth = (await crates.getByRole("button", { name: /Short/ }).boundingBox())!.width;
  expect(Math.abs(crateWidth - navWidth)).toBeLessThanOrEqual(2);

  const sidebar = page.getByRole("complementary");
  const before = (await sidebar.boundingBox())!.width;
  const handle = (await page.getByRole("separator", { name: "Resize sidebar" }).boundingBox())!;
  await page.mouse.move(handle.x + handle.width / 2, handle.y + 200);
  await page.mouse.down();
  await page.mouse.move(handle.x + handle.width / 2 + 100, handle.y + 200, { steps: 6 });
  await page.mouse.up();
  const after = (await sidebar.boundingBox())!.width;
  expect(Math.abs(after - before - 100)).toBeLessThanOrEqual(2);
  // It cannot be dragged past its limits.
  await page.getByRole("separator", { name: "Resize sidebar" }).focus();
  for (let i = 0; i < 30; i += 1) await page.keyboard.press("Shift+ArrowRight");
  expect((await sidebar.boundingBox())!.width).toBeLessThanOrEqual(420);
  await page.keyboard.press("Home");
  expect(Math.abs((await sidebar.boundingBox())!.width - 218)).toBeLessThanOrEqual(2);
  await page.keyboard.press("Shift+ArrowRight");
  await page.reload();
  expect(Math.abs((await sidebar.boundingBox())!.width - 258)).toBeLessThanOrEqual(2);
});

test("the Import screen shows what has finished downloading and what was already there", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { localStorage.setItem("opencrate.importView", "releases"); });
  await page.goto("/");
  await page.getByRole("button", { name: "Import" }).first().click();
  await expect(page.getByRole("heading", { name: "No download folders yet" })).toBeVisible();
  await page.getByRole("button", { name: "Choose in Settings..." }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Add download folder..." }).click();
  await expect(dialog.getByText("/Users/dj/Downloads")).toBeVisible();
  await page.keyboard.press("Escape");

  await expect(page.getByText("1 of 3 releases have finished downloading.")).toBeVisible();
  const fresh = page.getByRole("listitem").filter({ hasText: "Between Spaces" });
  await expect(fresh).toContainText("8 tracks");
  await expect(fresh).toContainText("Ready");
  await expect(page.getByRole("listitem").filter({ hasText: "Night Drive EP" })).toContainText("Still downloading");
  await expect(page.getByRole("listitem").filter({ hasText: "Bought on Bandcamp.zip" })).toContainText("1 archive");
  await expect(page.getByRole("heading", { name: "Already there when you added the folder" })).toBeVisible();
  await expect(page.getByRole("listitem").filter({ hasText: "Old Favourites" })).toContainText("12 tracks");

  await page.evaluate(() => { (window as any).__watchState = "offline"; });
  await expect(page.getByText("Downloads cannot be reached right now.")).toBeVisible({ timeout: 6000 });
});

test("a held disappearance is explained and only confirmed on request", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__heldMissing = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Add folder..." }).click();
  await dialog.getByRole("button", { name: "Scan Music" }).click();
  await expect(dialog.getByText("12 files vanished at once")).toBeVisible();
  const scans = () => calls(page).then((all) => all.filter((call) => call.command === "library_scan").map((call) => call.args));
  expect(await scans()).toEqual([{ rootId: 7 }]);

  await dialog.getByRole("button", { name: "Mark them missing" }).click();
  await expect.poll(scans).toEqual([{ rootId: 7 }, { rootId: 7, confirmMissing: true }]);
});

test("a track dropped on a rekordbox playlist goes into rekordbox once writing is on", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__rbEnabled = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await page.getByRole("button", { name: "Choose export..." }).click();
  await page.getByRole("button", { name: "Tracks", exact: true }).click();
  const sidebar = page.getByRole("complementary");
  const closers = sidebar.getByRole("navigation", { name: "Playlists" }).getByRole("button", { name: "Closers" });
  const row = page.getByTestId("track-row").filter({ hasText: "Rust Track 0004" });

  // Writing off: nothing is written, and the note says where to turn it on.
  await row.dragTo(closers);
  await expect(sidebar.getByRole("alert")).toContainText("Turn it on in Settings");
  expect(await page.evaluate(() => (window as any).__rbAdded)).toBeUndefined();

  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("checkbox", { name: /Let openCrate write to rekordbox's library/ }).click();
  await dialog.getByRole("button", { name: "I understand, let openCrate write" }).click();
  await expect(dialog.getByText("Writing is on for rekordbox 7.2.19.0342.")).toBeVisible();
  await dialog.getByRole("button", { name: "Close settings" }).click();

  await row.dragTo(closers);
  await expect(sidebar.getByRole("status")).toContainText("Added 1 to Closers in rekordbox.");
  expect(await page.evaluate(() => (window as any).__rbAdded)).toEqual([{ playlistId: 4, trackIds: [4] }]);
});

test("BPM and key say whether rekordbox analysed them or they come from the file's tags", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const bpm = (n: string) => page.getByTestId("track-row").filter({ hasText: `Rust Track ${n}` }).locator(".track-bpm");
  await expect(bpm("0003")).toHaveText("120");
  await expect(bpm("0003")).toHaveAttribute("title", "Analysed by rekordbox");
  await expect(bpm("0003")).toHaveClass(/is-analysed/);
  await expect(bpm("0002")).toHaveText("120");
  await expect(bpm("0002")).toHaveAttribute("title", "From the file's tags. rekordbox has not analysed this track yet.");
  await expect(bpm("0002")).toHaveClass(/from-tags/);
  await expect(bpm("0001")).toHaveText("--");
  await expect(bpm("0001")).not.toHaveClass(/is-analysed|from-tags/);
  // rekordbox can have a key without a BPM; the key says where it came from on its own.
  const key = (n: string) => page.getByTestId("track-row").filter({ hasText: `Rust Track ${n}` }).locator(".track-key");
  await expect(key("0001")).toHaveAttribute("title", "From rekordbox");
  await expect(key("0001")).not.toHaveClass(/from-tags/);
  await expect(key("0002")).toHaveAttribute("title", "From the file's tags. rekordbox has no key for this track yet.");
  await expect(key("0002")).toHaveClass(/from-tags/);
});

test("names and tags are typed as they are: no autocorrect, capitals or spelling marks", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("navigation", { name: "Crates" }).getByRole("button", { name: "New crate" }).click();
  for (const field of [page.getByRole("textbox", { name: "Search library" }), page.getByRole("textbox", { name: "New crate name" })]) {
    await expect(field).toHaveAttribute("autocorrect", "off");
    await expect(field).toHaveAttribute("autocapitalize", "off");
    await expect(field).toHaveAttribute("spellcheck", "false");
  }
});

test("a rekordbox folder is made from the sidebar, at the top or inside another folder", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__rbEnabled = true; (window as any).__rbWriting = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await page.getByRole("button", { name: "Choose export..." }).click();
  const playlists = page.getByRole("complementary").getByRole("navigation", { name: "Playlists" });

  // Escape gives up without writing.
  await playlists.getByRole("button", { name: "New rekordbox folder" }).click();
  await playlists.getByRole("textbox", { name: "New rekordbox folder name" }).fill("Gone");
  await page.keyboard.press("Escape");
  await expect(playlists.getByRole("textbox")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__rbFolders)).toBeUndefined();

  // A taken name comes back numbered.
  await playlists.getByRole("button", { name: "New rekordbox folder" }).click();
  await playlists.getByRole("textbox", { name: "New rekordbox folder name" }).fill("Sets");
  await page.keyboard.press("Enter");
  await expect(playlists.getByRole("status")).toContainText("Made the folder Sets (2) at the top of the playlists in rekordbox.");

  await playlists.getByRole("button", { name: "Sets", exact: true }).click({ button: "right" });
  await page.getByRole("menu", { name: "Sets actions" }).getByRole("menuitem", { name: "New folder inside" }).click();
  await playlists.getByRole("textbox", { name: "New folder in Sets" }).fill("Friday");
  await page.keyboard.press("Enter");
  await expect(playlists.getByRole("status")).toContainText("Made the folder Friday in Sets in rekordbox.");
  expect(await page.evaluate(() => (window as any).__rbFolders)).toEqual([{ name: "Sets", folderId: null }, { name: "Friday", folderId: 2 }]);
});

test("crates: a crate dropped on a rekordbox folder becomes a playlist there once the owner says yes", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__rbEnabled = true; (window as any).__rbWriting = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await page.getByRole("button", { name: "Choose export..." }).click();
  await page.getByRole("button", { name: "Tracks", exact: true }).click();
  const sidebar = page.getByRole("complementary");
  const crates = sidebar.getByRole("navigation", { name: "Crates" });
  await crates.getByRole("button", { name: "New crate" }).click();
  await crates.getByRole("textbox", { name: "New crate name" }).fill("Warm-up");
  await page.keyboard.press("Enter");
  await page.getByRole("button", { name: "Back to tracks" }).click();
  const crate = crates.getByRole("button", { name: /Warm-up/ });
  await page.getByTestId("track-row").filter({ hasText: "Rust Track 0004" }).dragTo(crate);
  await expect(crate).toContainText("1");
  const playlists = sidebar.getByRole("navigation", { name: "Playlists" });

  // A crate is not tracks: a playlist does not take it.
  await crate.dragTo(playlists.getByRole("button", { name: "Closers" }));
  await expect(playlists.getByRole("alertdialog")).toHaveCount(0);

  // On a folder it asks first; Cancel writes nothing.
  await crate.dragTo(playlists.getByRole("button", { name: "Sets" }));
  const ask = playlists.getByRole("alertdialog", { name: "Make a rekordbox playlist from Warm-up?" });
  await expect(ask).toContainText("in Sets? The crate stays as it is.");
  await ask.getByRole("button", { name: "Cancel" }).click();
  await expect(ask).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__rbCratePlaylists)).toBeUndefined();

  await crate.dragTo(playlists.getByRole("button", { name: "Sets" }));
  await ask.getByRole("button", { name: "Make playlist" }).click();
  await expect(playlists.getByRole("status")).toContainText("Made Warm-up in Sets in rekordbox with 1 track.");
  // On the heading, it goes at the top of the tree.
  await crate.dragTo(playlists.getByText("PLAYLISTS", { exact: true }));
  await expect(ask).toContainText("at the top of rekordbox's playlists?");
  await ask.getByRole("button", { name: "Make playlist" }).click();
  await expect(playlists.getByRole("status")).toContainText("Made Warm-up at the top of the playlists in rekordbox with 1 track.");
  await expect.poll(() => page.evaluate(() => (window as any).__rbCratePlaylists)).toEqual([{ crateId: 1, folderId: 2 }, { crateId: 1, folderId: null }]);
  // The crate is left as it was.
  await expect(crate).toContainText("1");
});

test("rekordbox sync: changes on either side are reviewed and only ticked ones are applied", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__rbEnabled = true; (window as any).__rbWriting = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  const card = page.getByRole("region", { name: "Sync with rekordbox" });
  await card.getByRole("button", { name: "Review changes" }).click();

  const fromRekordbox = card.getByRole("group", { name: "rekordbox to openCrate" });
  const toRekordbox = card.getByRole("group", { name: "openCrate to rekordbox" });
  const both = card.getByRole("group", { name: "Changed on both sides" });
  await expect(fromRekordbox.getByRole("checkbox", { name: /Night Drive, title/ })).toBeChecked();
  await expect(fromRekordbox).toContainText("Night Drive (Original Mix)");
  await expect(toRekordbox.getByRole("checkbox", { name: /Soft Focus, label/ })).toBeChecked();
  await expect(toRekordbox).toContainText("(empty)");
  // Both changed: nothing is chosen until the owner picks a side.
  await expect(both.getByRole("checkbox", { name: /Late Train, year/ })).not.toBeChecked();
  await expect(both).toContainText("was 2018");
  await both.getByRole("radio", { name: "Keep rekordbox's: 2020" }).check();
  await expect(both.getByRole("checkbox", { name: /Late Train, year/ })).toBeChecked();

  await toRekordbox.getByRole("checkbox", { name: /Soft Focus, label/ }).uncheck();
  await card.getByRole("button", { name: "Sync 2 changes" }).click();
  await expect(card.getByRole("status")).toContainText("2 changes came from rekordbox");
  expect(await page.evaluate(() => (window as any).__mergeChoices)).toEqual([
    { rbKey: 101, field: "title", keep: "rekordbox", ours: "Night Drive", theirs: "Night Drive (Original Mix)" },
    { rbKey: 103, field: "year", keep: "rekordbox", ours: "2019", theirs: "2020" },
  ]);
  await expect(card).toContainText("openCrate and rekordbox agree");
});

test("playlists from the export appear in the sidebar and open as an ordered track view", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const sidebar = page.getByRole("complementary");
  await expect(sidebar.getByText("Read rekordbox on the rekordbox screen to see its playlists here.")).toBeVisible();

  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await page.getByRole("button", { name: "Choose export..." }).click();
  await expect(page.getByRole("heading", { name: "collection.xml" })).toBeVisible();

  const tree = sidebar.getByRole("navigation", { name: "Playlists" });
  await expect(tree.getByRole("button", { name: "Closers" })).toBeVisible();
  await expect(tree.getByText("ROOT")).toHaveCount(0);
  await expect(tree.getByRole("button", { name: "Sets" })).toHaveAttribute("aria-expanded", "true");
  await expect(tree.getByRole("button", { name: "Friday warm-up" })).toBeVisible();
  await page.screenshot({ path: "test-results/playlists-sidebar.png" });

  await tree.getByRole("button", { name: "Sets" }).click();
  await expect(tree.getByRole("button", { name: "Friday warm-up" })).toHaveCount(0);
  await tree.getByRole("button", { name: "Sets" }).click();

  await tree.getByRole("button", { name: "Friday warm-up" }).click();
  await expect(page.getByRole("heading", { name: "Friday warm-up" })).toBeVisible();
  await expect(page.getByText("38 of 40 tracks are in your library.")).toBeVisible();
  await expect(tree.getByRole("button", { name: "Friday warm-up" })).toHaveAttribute("aria-current", "true");
  await expect(page.getByLabel("Sort library")).toHaveValue("playlist");
  await expect.poll(async () => (await calls(page)).filter((call) => call.command === "library_read").at(-1)?.args.query)
    .toEqual({ view: "tracks", search: "", sort: "playlist", format: null, artistId: null, releaseId: null, playlistId: 3, crateId: null });
  await page.screenshot({ path: "test-results/playlists-open.png" });

  await page.getByRole("button", { name: "Back to tracks" }).click();
  await expect(page.getByLabel("Sort library")).toHaveValue("recent");
  await expect.poll(async () => (await calls(page)).filter((call) => call.command === "library_read").at(-1)?.args.query.playlistId).toBeNull();
});

test("a library that could not be opened is explained instead of silently showing the demo", async ({ page }) => {
  await stubBridge(page, "synthetic");
  await page.addInitScript(() => { (window as any).__startupError = "The library folder could not be opened: already owned by another openCrate window."; });
  await page.goto("/");
  await expect(page.getByRole("alert").first()).toContainText("already owned by another openCrate window");
  await expect(page.getByRole("alert").first()).toContainText("Showing the synthetic demo instead");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
});

test("a track whose file is gone is marked, not silently listed", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("1,234 tracks");
  const first = page.getByTestId("track-row").first();
  await expect(first.locator(".missing-tag")).toHaveText("Missing");
  await expect(first).toHaveClass(/is-missing/);
  await expect(page.getByTestId("track-row").nth(1).locator(".missing-tag")).toHaveCount(0);
  await expect(page.getByTestId("track-row").nth(1).locator(".track-format")).toHaveText("AIFF");
});

test("an unplugged folder is explained, the library stays browsable, and a recheck clears it", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Add folder..." }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Close settings" }).click();
  const banner = page.locator(".offline-banner");
  await expect(banner).toHaveCount(0);
  // The drive goes away; the window notices when it regains focus.
  await page.evaluate(() => { (window as any).__folderState = "unavailable"; window.dispatchEvent(new Event("focus")); });
  await expect(banner).toContainText("Music is not available right now (not found).");
  await expect(banner).toContainText("nothing was marked missing");
  await expect(page.getByTestId("track-row").first()).toBeVisible();
  await page.screenshot({ path: "test-results/offline-banner.png" });

  await page.evaluate(() => { (window as any).__folderState = undefined; });
  await banner.getByRole("button", { name: "Check again" }).click();
  await expect(banner).toHaveCount(0);
});

test("a folder that is not the one that was added can be used anyway, only on request", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Add folder..." }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Close settings" }).click();
  await page.evaluate(() => { (window as any).__folderState = "unverified"; window.dispatchEvent(new Event("focus")); });
  const banner = page.locator(".offline-banner");
  await expect(banner).toContainText("is not the folder that was added");
  expect((await calls(page)).filter((call) => call.command === "library_confirm_root")).toHaveLength(0);
  await banner.getByRole("button", { name: "Use it anyway" }).click();
  await expect(banner).toHaveCount(0);
  const confirmed = (await calls(page)).filter((call) => call.command === "library_confirm_root");
  expect(confirmed.map((call) => call.args)).toEqual([{ rootId: 7 }]);
});


test("rekordbox database: the setting warns before it turns on", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  const toggle = dialog.getByRole("checkbox", { name: "Let openCrate read rekordbox's library directly" });
  await expect(toggle).not.toBeChecked();
  await toggle.click();
  const warning = dialog.getByRole("region", { name: "Before you turn this on" });
  await expect(warning).toContainText("backup");
  await expect(warning).toContainText("rekordbox must be closed");
  await warning.getByRole("button", { name: "Cancel" }).click();
  await expect(toggle).not.toBeChecked();
  expect((await calls(page)).some((call) => call.command === "rekordbox_db_enable")).toBe(false);
  await toggle.click();
  await dialog.getByRole("button", { name: "I understand, turn it on" }).click();
  await expect(toggle).toBeChecked();
  await expect(dialog.getByText("On for rekordbox 7.2.19.0342.")).toBeVisible();
  await page.screenshot({ path: "test-results/rekordbox-database-on.png" });
  await toggle.click();
  await expect(toggle).not.toBeChecked();
  const sent = (await calls(page)).filter((call) => call.command === "rekordbox_db_enable").map((call) => call.args.enabled);
  expect(sent).toEqual([true, false]);
});

test("rekordbox database: reading fills the playlists and keeps a backup", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__rbEnabled = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await page.getByRole("button", { name: "Read from rekordbox" }).click();
  await expect(page.getByText("Read 40 entries. 38 match your files, so 38 tracks now show rekordbox's tempo and key.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "rekordbox library" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Friday warm-up" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Read again" })).toHaveCount(0);
  await page.screenshot({ path: "test-results/rekordbox-database-read.png" });
  await page.getByRole("button", { name: "Settings" }).click();
  const backups = page.getByRole("dialog", { name: "Settings" }).getByRole("list", { name: "rekordbox backups" });
  await expect(backups.getByRole("listitem")).toHaveCount(1);
  await expect(backups).toContainText("Before reading");
  await expect(backups).toContainText("40 tracks, 3 playlists");
});

test("rekordbox database: a running rekordbox is reported and nothing is read", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__rbEnabled = true; (window as any).__rbRunning = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await expect(page.getByText("rekordbox is running. Quit it to read its library.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Read from rekordbox" })).toBeDisabled();
  expect((await calls(page)).some((call) => call.command === "rekordbox_db_read")).toBe(false);
});

test("rekordbox database: restore asks first, and a new rekordbox version must be confirmed again", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__rbEnabled = true; });
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await page.getByRole("button", { name: "Read from rekordbox" }).click();
  await expect(page.getByRole("heading", { name: "rekordbox library" })).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  const backups = dialog.getByRole("list", { name: "rekordbox backups" });
  await backups.getByRole("button", { name: /^Pin backup/ }).click();
  await expect(backups.getByRole("button", { name: /^Unpin backup/ })).toBeVisible();
  await backups.getByRole("button", { name: /^Restore backup/ }).click();
  const confirm = dialog.getByRole("region", { name: "Restore this backup?" });
  await expect(confirm).toContainText("saved as a backup first");
  await confirm.getByRole("button", { name: "Cancel" }).click();
  expect((await calls(page)).some((call) => call.command === "rekordbox_restore")).toBe(false);
  await backups.getByRole("button", { name: /^Restore backup/ }).click();
  await dialog.getByRole("region", { name: "Restore this backup?" }).getByRole("button", { name: "Restore" }).click();
  await expect(dialog.getByRole("status").filter({ hasText: "Restored" })).toBeVisible();
  await expect(backups.getByRole("listitem")).toHaveCount(2);
  await expect(backups).toContainText("Before a restore");
  await page.evaluate(() => { (window as any).__rbVersion = "7.2.20.0001"; });
  await dialog.getByRole("button", { name: "Close settings" }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(dialog.getByText("rekordbox is now 7.2.20.0001")).toBeVisible();
  await dialog.getByRole("button", { name: "Confirm for 7.2.20.0001" }).click();
  await expect(dialog.getByText("On for rekordbox 7.2.20.0001.")).toBeVisible();
});

const newTracks = () => [
  { fileId: 11, title: "Night Drive", artist: "Studio 001", album: "Between Spaces", durationMs: 301000, container: "AIFF", filedMs: 1, offeredMs: null, hidden: false },
  { fileId: 12, title: "Soft Focus (Dub Mix)", artist: "Studio 001", album: "Between Spaces", durationMs: 402000, container: "AIFF", filedMs: 1, offeredMs: null, hidden: false },
  { fileId: 13, title: "Late Train", artist: "Example Artist", album: "Night Lines", durationMs: 250000, container: "MP3", filedMs: 1, offeredMs: null, hidden: false },
];

test("send to rekordbox: new tracks are listed, ticked and sent", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript((tracks) => { (window as any).__newTracks = tracks; }, newTracks());
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  const card = page.getByRole("region", { name: "New in your library" });
  await expect(card.getByRole("checkbox")).toHaveCount(3);
  await expect(card.getByRole("checkbox", { name: /Late Train/ })).toBeChecked();
  await card.getByRole("checkbox", { name: /Late Train/ }).uncheck();
  await card.getByRole("button", { name: "Send 2 to rekordbox" }).click();
  await expect(card.getByRole("status")).toContainText("2 tracks are in openCrate's rekordbox file");
  await expect(card).toContainText("opencrate.xml");
  await expect(card).toContainText("openCrate new tracks");
  await expect(card.getByText("Sent, waiting for rekordbox")).toHaveCount(2);
  await page.screenshot({ path: "test-results/rekordbox-send.png" });
  const sent = (await calls(page)).filter((call) => call.command === "rekordbox_send").map((call) => call.args.fileIds);
  expect(sent).toEqual([[11, 12]]);
});

test("send to rekordbox: a stale or missing read is pointed out", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript((tracks) => { (window as any).__newTracks = tracks; (window as any).__mirrorReadMs = 5; }, newTracks());
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  const card = page.getByRole("region", { name: "New in your library" });
  await expect(card).toContainText("rekordbox was last read before these were filed");
  await page.evaluate(() => { (window as any).__mirrorReadMs = null; });
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await expect(page.getByRole("region", { name: "New in your library" })).toContainText("rekordbox has not been read yet");
});

test("send to rekordbox: nothing new shows no card", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await expect(page.getByRole("heading", { name: "No export yet" })).toBeVisible();
  await expect(page.getByRole("region", { name: "New in your library" })).toHaveCount(0);
});

test("crates for rekordbox: a crate is switched into the file and the file updated", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  const crates = page.getByRole("navigation", { name: "Crates" });
  await crates.getByRole("button", { name: "New crate" }).click();
  await crates.getByRole("textbox", { name: "New crate name" }).fill("Warm-up");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Warm-up" })).toBeVisible();
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  const card = page.getByRole("region", { name: "Crates for rekordbox" });
  const toggle = card.getByRole("checkbox", { name: "Warm-up in openCrate's rekordbox file" });
  await expect(toggle).not.toBeChecked();
  await expect(card.getByRole("button", { name: "Update file" })).toHaveCount(0);
  await toggle.check();
  await expect(toggle).toBeChecked();
  await expect(card.getByRole("status")).toContainText('"Warm-up" is in the file.');
  await expect(card.getByRole("status")).toContainText("openCrate crates");
  await card.getByRole("button", { name: "Update file" }).click();
  await expect(card.getByRole("status")).toContainText("as they are");
  const sent = (await calls(page)).filter((call) => call.command === "rekordbox_set_crate").map((call) => call.args);
  expect(sent).toEqual([{ crateId: 1, inFile: true }]);
});

test("send to rekordbox: a track kept out is hidden and can be brought back", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript((tracks) => { (window as any).__newTracks = tracks; }, newTracks());
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  const card = page.getByRole("region", { name: "New in your library" });
  await card.getByRole("button", { name: "Keep Late Train out of rekordbox" }).click();
  await expect(card.getByRole("checkbox")).toHaveCount(2);
  await expect(card.getByRole("button", { name: "Send 2 to rekordbox" })).toBeEnabled();
  await card.getByRole("button", { name: "Show 1 hidden track" }).click();
  await card.getByRole("button", { name: "Bring Late Train back" }).click();
  await expect(card.getByRole("checkbox")).toHaveCount(3);
  const hides = (await calls(page)).filter((call) => call.command === "rekordbox_hide").map((call) => call.args);
  expect(hides).toEqual([{ fileIds: [13], hidden: true }, { fileIds: [13], hidden: false }]);
});

test("first start: the welcome walks through the music folder, downloads and rekordbox, and can be put off", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.addInitScript(() => { (window as any).__persistent = true; });
  await page.goto("/");
  const welcome = page.getByRole("region", { name: "Welcome to openCrate" });
  await expect(welcome).toBeVisible();
  await welcome.getByRole("button", { name: "Add your music folder..." }).click();
  await expect(welcome.getByText("Music: 20 files read")).toBeVisible({ timeout: 10_000 });
  await welcome.getByRole("button", { name: "Add a download folder..." }).click();
  await expect(welcome.getByText("Downloads", { exact: true })).toBeVisible();
  await page.screenshot({ path: "test-results/welcome.png" });
  await welcome.getByRole("button", { name: "Open the rekordbox screen" }).click();
  await expect(page.getByRole("heading", { name: "No export yet" })).toBeVisible();
  await page.getByRole("button", { name: "Tracks", exact: true }).click();
  await page.getByRole("region", { name: "Welcome to openCrate" }).getByRole("button", { name: "Done for now" }).click();
  await expect(page.getByRole("region", { name: "Welcome to openCrate" })).toHaveCount(0);
  await page.reload();
  await expect(page.getByRole("status")).toBeVisible();
  await expect(page.getByRole("region", { name: "Welcome to openCrate" })).toHaveCount(0);
});

test("first start: a demo or a library that already has folders shows no welcome", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await expect(page.getByText("Rust Track 0001")).toBeVisible();
  await expect(page.getByRole("region", { name: "Welcome to openCrate" })).toHaveCount(0);
});

test("rekordbox sync: an export changed since it was read is pointed out", async ({ page }) => {
  await stubBridge(page, "managed");
  await page.goto("/");
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await page.getByRole("button", { name: "Choose export..." }).click();
  await expect(page.getByRole("heading", { name: "collection.xml" })).toBeVisible();
  await expect(page.getByText("This export has changed since it was read.")).toHaveCount(0);
  await page.evaluate(() => { (window as any).__exportChanged = true; });
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await expect(page.getByText("This export has changed since it was read. Read it again to bring in the changes.")).toBeVisible();
});

test("accessibility: the main screens, in both themes, have no WCAG A or AA violations", async ({ page }) => {
  const found: string[] = [];
  const scan = async (label: string) => {
    const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
    for (const violation of result.violations) {
      found.push(`${label}: ${violation.id} at ${violation.nodes.map((node) => node.target.join(" ")).join(", ")}`);
    }
  };
  await stubBridge(page, "managed");
  await page.addInitScript((tracks) => { (window as any).__newTracks = tracks; }, newTracks());
  for (const theme of ["dark", "light"]) {
    await page.addInitScript((chosen) => localStorage.setItem("opencrate.appearance.v1", JSON.stringify({ compact: false, reducedMotion: false, theme: chosen })), theme);
    await page.goto("/");
    await page.getByText("Rust Track 0002").dblclick();
    await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
    await scan(`${theme} library and player`);
    await page.getByRole("button", { name: "rekordbox", exact: true }).click();
    await page.getByRole("button", { name: "Choose export..." }).click();
    await expect(page.getByRole("region", { name: "New in your library" })).toBeVisible();
    await scan(`${theme} rekordbox`);
    await page.getByRole("button", { name: "Import", exact: true }).click();
    await scan(`${theme} import`);
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
    await scan(`${theme} settings`);
    await page.keyboard.press("Escape");
  }
  expect(found).toEqual([]);
});
