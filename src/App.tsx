import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { ArtistArtwork, Artwork } from "./components/Artwork";
import { Icon, type IconName } from "./components/Icon";
import { Analysis } from "./components/Analysis";
import { CleanUp } from "./components/CleanUp";
import { EditTracks } from "./components/EditTracks";
import { Downloads } from "./components/Downloads";
import { FolderLayout } from "./components/FolderLayout";
import { RekordboxDatabase } from "./components/RekordboxDatabase";
import { Welcome } from "./components/Welcome";
import { LibraryCheck } from "./components/LibraryCheck";
import { LookupKeys } from "./components/LookupKeys";
import { Folders } from "./components/Folders";
import { Import } from "./components/Import";
import { Library } from "./components/Library";
import { Offline } from "./components/Offline";
import { Crates } from "./components/Crates";
import { loadSidebarWidth, SidebarResizer } from "./components/SidebarResizer";
import { PlaylistBrowser, Playlists } from "./components/Playlists";
import { CrateBrowser } from "./components/CrateBrowser";
import { useAutoImport } from "./components/AutoImport";
import { PlayerBar, type PlayRequest } from "./components/PlayerBar";
import { Sync } from "./components/Sync";
import { DEFAULT_QUERY, type Format, type FormatCount, type ImportBatch, type LibraryApi, type LibraryItem, type CrateInfo, type PlaylistInfo, type Sort, type View } from "./library/types";
import { DEFAULT_PREFERENCES, loadPreferences, savePreferences, type Preferences } from "./preferences";
import { PLAIN_TEXT } from "./components/plainText";

/** Open or close a dialog to match the state. A close made here is marked, because its "close"
 * event arrives a moment later and must not undo an open that came in between. */
function showOrClose(element: HTMLDialogElement | null, open: boolean) {
  if (!element) return;
  if (open && !element.open) element.showModal();
  if (!open && element.open) {
    element.dataset.closedHere = "true";
    element.close();
  }
}

/** Whether a "close" event came from the browser (Escape, for one) rather than from `showOrClose`. */
function closedByBrowser(element: HTMLDialogElement): boolean {
  const ours = element.dataset.closedHere === "true";
  delete element.dataset.closedHere;
  return !ours;
}

/** Pointer handlers that close a modal dialog when a click starts and ends on its backdrop. A
 * native dialog receives backdrop clicks itself, so the point is checked against its box. */
function closesOutside(close: () => void) {
  const outside = (event: React.PointerEvent<HTMLDialogElement> | React.MouseEvent<HTMLDialogElement>) => {
    if (event.target !== event.currentTarget) return false;
    const box = event.currentTarget.getBoundingClientRect();
    return event.clientX < box.left || event.clientX > box.right || event.clientY < box.top || event.clientY > box.bottom;
  };
  return {
    // Kept on the element, so a render between press and release does not forget it.
    onPointerDown: (event: React.PointerEvent<HTMLDialogElement>) => { event.currentTarget.dataset.pressedOutside = String(outside(event)); },
    onClick: (event: React.MouseEvent<HTMLDialogElement>) => {
      const pressed = event.currentTarget.dataset.pressedOutside === "true";
      delete event.currentTarget.dataset.pressedOutside;
      if (pressed && outside(event)) close();
    },
  };
}

type Screen = View | "import" | "recent" | "sync" | "cleanup" | "playlists" | "crates";
const NAV: { screen: Screen; name: string; icon: IconName }[] = [
  { screen: "tracks", name: "Tracks", icon: "tracks" },
  { screen: "releases", name: "Releases", icon: "releases" },
  { screen: "artists", name: "Artists", icon: "artists" },
  { screen: "import", name: "Import", icon: "import" },
  { screen: "recent", name: "Recently imported", icon: "clock" },
  { screen: "sync", name: "rekordbox", icon: "sync" },
  { screen: "cleanup", name: "Clean up", icon: "clean" },
];

/** One line under the title saying what the page holds. */
const PAGE_NOTES = {
  tracks: "All your tracks live here, from every music folder.",
  releases: "All your releases live here, one card per album or EP.",
  artists: "All your artists live here, with the releases they are on.",
} as const;

type SettingsGroup = "appearance" | "library" | "import" | "rekordbox";
const SETTINGS_GROUPS: { id: SettingsGroup; name: string }[] = [
  { id: "appearance", name: "Appearance" },
  { id: "library", name: "Library" },
  { id: "import", name: "Import" },
  { id: "rekordbox", name: "rekordbox" },
];

/** The formats offered when the backend cannot say which it holds (the demo). */
const DEMO_FORMATS = ["AIFF", "WAV", "MP3", "FLAC"];

/** When an import batch happened, in a few words: "Today 21:14 to 21:40" or the date. */
function describeBatch(batch: ImportBatch): string {
  const started = new Date(batch.startedMs);
  const last = new Date(batch.lastMs);
  const today = new Date();
  const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();
  const time = (date: Date) => date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const day = sameDay(started, today) ? "Today" : started.toLocaleDateString([], { month: "short", day: "numeric", year: sameDay(started, today) || started.getFullYear() === today.getFullYear() ? undefined : "numeric" });
  const span = time(started) === time(last) ? time(started) : `${time(started)} to ${time(last)}`;
  return `${day} ${span}`;
}

/** The library query a screen shows; Back uses it to name the list it returns to. */
function queryOf(screen: Screen, search: string, sort: Sort, format: Format | "all", focus: LibraryItem | null, playlist: PlaylistInfo | null, crate: CrateInfo | null, reversed = false, batch: ImportBatch | null = null) {
  return {
    ...DEFAULT_QUERY, view: screen === "import" || screen === "recent" || screen === "sync" || screen === "cleanup" || screen === "playlists" || screen === "crates" ? "tracks" as const : screen,
    search, sort, reversed, format: screen === "tracks" ? format : "all" as const,
    artistId: focus?.kind === "artists" ? focus.id : null,
    releaseId: focus?.kind === "releases" ? focus.id : null,
    playlistId: playlist?.id ?? null,
    crateId: crate?.id ?? null,
    // No batch yet: an id no batch has, so the list is empty rather than the whole library.
    batchId: screen === "recent" ? batch?.id ?? -1 : null,
  };
}

export function App({ api }: { api: LibraryApi }) {
  const [screen, setScreen] = useState<Screen>("tracks");
  /** The folder the playlist browser shows (null: the top), and whether the open playlist came from it. */
  const [browseFolder, setBrowseFolder] = useState<number | null>(null);
  const [fromBrowser, setFromBrowser] = useState(false);
  /** The open crate came from the crates page, so Back returns there. */
  const [fromCrates, setFromCrates] = useState(false);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<Sort>("recent");
  /** The sort the other way round (oldest first, Z to A, BPM high to low). */
  const [reversed, setReversed] = useState(false);
  const [format, setFormat] = useState<Format | "all">("all");
  const [focus, setFocus] = useState<LibraryItem | null>(null);
  const [playlist, setPlaylist] = useState<PlaylistInfo | null>(null);
  const [crate, setCrate] = useState<CrateInfo | null>(null);
  const [crateVersion, setCrateVersion] = useState(0);
  /** Import batches, newest first, and the one Recently imported shows. */
  const [batches, setBatches] = useState<ImportBatch[]>([]);
  const [batch, setBatch] = useState<ImportBatch | null>(null);
  /** The containers the library holds, for the format filter; the demo offers a fixed few. */
  const [formatList, setFormatList] = useState<FormatCount[]>([]);
  const [selected, setSelected] = useState<LibraryItem | null>(null);
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const [playRequest, setPlayRequest] = useState<PlayRequest | null>(null);
  /** Tracks to play before the list carries on. */
  const [queue, setQueue] = useState<LibraryItem[]>([]);
  /** Tracks chosen together in the list, and the ones open in the editor. */
  const [selection, setSelection] = useState<number[]>([]);
  const [editing, setEditing] = useState<number[] | null>(null);
  /** What the last export did. */
  const [exportNote, setExportNote] = useState("");
  /** Rows in the list on show, once known; an empty list has nothing to export. */
  const [listTotal, setListTotal] = useState<number | null>(null);
  const exportList = async (format: "m3u8" | "csv") => {
    if (!api.admin) return;
    setExportNote("");
    try {
      const done = await api.admin.exportTracks(query, format, heading);
      if (done) setExportNote(`${done.tracks} ${done.tracks === 1 ? "track" : "tracks"} exported to ${done.path}.${done.withoutFile > 0 ? ` ${done.withoutFile} ${done.withoutFile === 1 ? "has" : "have"} no file that can be reached.` : ""}`);
    } catch (reason) {
      setExportNote(reason instanceof Error ? reason.message : "Something went wrong.");
    }
  };
  /** What the last cover change on a release page did. */
  const [coverNote, setCoverNote] = useState("");
  const [coverBusy, setCoverBusy] = useState(false);
  /** What the cover change is doing right now, while it takes a moment. */
  const [coverStep, setCoverStep] = useState("");
  /** What the last list action did (tracks added to or removed from a crate), under the heading. */
  const [listNote, setListNote] = useState("");
  /** Removing several tracks from a crate waits for a yes. */
  const [removeAsk, setRemoveAsk] = useState<{ crateId: number; ids: number[] } | null>(null);
  /** The owner's crates, for Add to a crate in the track menu. */
  const [crateList, setCrateList] = useState<CrateInfo[]>([]);
  /** Removing a release's cover waits for a yes: it cannot be undone. */
  const [coverAsk, setCoverAsk] = useState(false);
  const changeCover = async (releaseId: number, action: "choose" | "online" | "remove") => {
    if (!api.admin) return;
    setCoverBusy(true);
    setCoverNote("");
    setCoverStep(action === "online" ? "Looking for a cover online..." : action === "remove" ? "Removing the cover from the files..." : "");
    try {
      const done = action === "remove" ? await api.admin.removeReleaseArt(releaseId) : await api.admin.setReleaseArt(releaseId, action);
      if (done) {
        setCoverNote(action === "remove" ? `The cover was removed from ${done.files} ${done.files === 1 ? "file" : "files"}.` : `${done.files} ${done.files === 1 ? "file now has" : "files now have"} the cover.`);
        refreshLibrary();
      }
    } catch (reason) {
      setCoverNote(reason instanceof Error ? reason.message : "Something went wrong.");
    } finally {
      setCoverBusy(false);
      setCoverStep("");
    }
  };
  /** The loaded track and whether it plays, marked in the list. */
  const [nowPlaying, setNowPlaying] = useState<{ id: number; playing: boolean; item: LibraryItem | null } | null>(null);
  /** Cues and grid follow what the player bar shows: the loaded track, else the selected one. */
  const analysed = nowPlaying?.item?.kind === "tracks" ? nowPlaying.item : selected?.kind === "tracks" ? selected : null;
  const [preferences, setPreferences] = useState(DEFAULT_PREFERENCES);
  const [settingsError, setSettingsError] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  /** Settings: the words typed into its search, the group in view, and how many rows match. */
  const [settingsQuery, setSettingsQuery] = useState("");
  const [settingsGroup, setSettingsGroup] = useState<SettingsGroup>("appearance");
  const [settingsMatches, setSettingsMatches] = useState(0);
  const settingsBody = useRef<HTMLDivElement>(null);
  const trackSettingsGroup = () => {
    const body = settingsBody.current;
    if (!body) return;
    const top = body.getBoundingClientRect().top + 40;
    let current: SettingsGroup = "appearance";
    for (const group of body.querySelectorAll<HTMLElement>("[data-group]")) {
      if (group.getBoundingClientRect().top <= top) current = group.dataset.group as SettingsGroup;
    }
    setSettingsGroup(current);
  };
  // The search hides every row and section whose words do not match; headings of a matching
  // section stay, so a hit keeps its context.
  useLayoutEffect(() => {
    const body = settingsBody.current;
    if (!body) return;
    const query = settingsQuery.trim().toLowerCase();
    let matches = 0;
    for (const section of body.querySelectorAll<HTMLElement>(".settings-section")) {
      const rows = [...section.querySelectorAll<HTMLElement>(".setting-row, .theme-choice, .settings-row")];
      let shown = 0;
      if (query === "") {
        for (const row of rows) row.hidden = false;
        section.hidden = false;
        continue;
      }
      const heading = section.querySelector("h3")?.textContent?.toLowerCase() ?? "";
      const headingHit = heading.includes(query);
      for (const row of rows) {
        const hit = headingHit || (row.textContent ?? "").toLowerCase().includes(query);
        row.hidden = !hit;
        if (hit) shown += 1;
      }
      // A section without rows (folders, lookup keys) matches on its labels, not its prose, so
      // "rekordbox" in a sentence does not drag every section in.
      const labels = [...section.querySelectorAll<HTMLElement>("strong, button, label, legend, summary")].map((item) => item.textContent?.toLowerCase() ?? "").join(" ");
      section.hidden = !(headingHit || shown > 0 || (rows.length === 0 && labels.includes(query)));
      if (!section.hidden) matches += rows.length === 0 ? 1 : shown;
    }
    for (const group of body.querySelectorAll<HTMLElement>(".settings-group")) {
      group.hidden = query !== "" && [...group.querySelectorAll<HTMLElement>(".settings-section")].every((section) => section.hidden);
    }
    setSettingsMatches(matches);
    if (query !== "") body.scrollTo({ top: 0 });
  }, [settingsQuery, settingsOpen, api.admin]);
  // Cmd+, (Ctrl+, on Windows) opens Settings, as does the macOS app menu, which sends this event.
  useEffect(() => {
    const open = () => setSettingsOpen(true);
    const key = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && !event.altKey && event.key === ",") {
        event.preventDefault();
        open();
      }
    };
    window.addEventListener("opencrate:open-settings", open);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("opencrate:open-settings", open);
      window.removeEventListener("keydown", key);
    };
  }, []);
  const [analysisOpen, setAnalysisOpen] = useState(false);
  const analysisDialog = useRef<HTMLDialogElement>(null);
  const [libraryVersion, setLibraryVersion] = useState(0);
  const dialog = useRef<HTMLDialogElement>(null);
  const searchInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    try { setPreferences(loadPreferences(window.localStorage)); }
    catch (error) { setSettingsError(error instanceof Error ? error.message : "Appearance settings could not be loaded."); }
  }, []);
  useEffect(() => {
    document.documentElement.dataset.reduceMotion = String(preferences.reducedMotion);
  }, [preferences.reducedMotion]);
  useEffect(() => {
    document.documentElement.dataset.theme = preferences.theme;
  }, [preferences.theme]);
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (dialog.current?.open) return;
      const target = event.target;
      const editing = target instanceof HTMLElement && (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) || target.isContentEditable);
      if (event.key === "/" && !editing) {
        event.preventDefault();
        searchInput.current?.focus();
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, []);
  useEffect(() => { showOrClose(analysisDialog.current, analysisOpen); }, [analysisOpen]);
  useEffect(() => { showOrClose(dialog.current, settingsOpen); }, [settingsOpen]);

  const updatePreferences = (value: Preferences) => {
    setPreferences(value);
    try {
      savePreferences(window.localStorage, value);
      setSettingsError("");
    } catch {
      setSettingsError("Appearance changed for this session, but could not be saved. Check storage permissions.");
    }
  };
  const navigate = (next: Screen) => {
    // Moving between Tracks, Releases and Artists keeps the search: it looks in all three.
    const views: Screen[] = ["tracks", "releases", "artists"];
    if (!(views.includes(next) && views.includes(screen) && !focus && !playlist && !crate)) setSearch("");
    setScreen(next); setFocus(null); setPlaylist(null); setCrate(null); setFormat("all"); setSort("recent"); setReversed(false); setFromBrowser(false); setFromCrates(false);
  };
  // Keyed on what the query uses, so a refreshed focus (a new name) is not a new list.
  const query = useMemo(() => queryOf(screen, search, sort, format, focus, playlist, crate, reversed, batch), [screen, search, sort, format, focus?.kind, focus?.id, playlist, crate, reversed, batch]);
  /** The grid a release or artist was opened from, so Back returns to it as it was. */
  const [backTo, setBackTo] = useState<{ screen: View; sort: Sort; reversed: boolean; top: number } | { crate: CrateInfo; top: number } | { playlist: PlaylistInfo; top: number } | null>(null);
  // A new list says how long it is once read; until then the last count does not apply.
  useEffect(() => { setListTotal(null); }, [query, libraryVersion]);
  const openPlaylist = (next: PlaylistInfo, browsed = false) => {
    setScreen("tracks"); setFocus(null); setPlaylist(next); setCrate(null); setSearch(""); setFormat("all"); setSort("playlist"); setReversed(false); setFromBrowser(browsed);
  };
  /** Opens a crate's tracks; `browsed` when it came from the crates page, so Back returns there. */
  const openCrate = (next: CrateInfo, browsed = false) => {
    setScreen("tracks"); setFocus(null); setPlaylist(null); setCrate(next); setSearch(""); setFormat("all"); setSort("crate"); setReversed(false); setFromCrates(browsed);
  };
  const crateChanged = useCallback(() => setCrateVersion((value) => value + 1), []);
  // Recently imported reads the batches when opened and whenever the library changes; the
  // newest is shown unless one was chosen.
  useEffect(() => {
    if (!api.admin || screen !== "recent") return;
    let live = true;
    api.admin.importBatches().then((found) => {
      if (!live) return;
      setBatches(found);
      setBatch((current) => found.find((item) => item.id === current?.id) ?? found[0] ?? null);
    }, () => undefined);
    return () => { live = false; };
  }, [api.admin, screen, libraryVersion]);
  useEffect(() => {
    if (!api.admin) return;
    let live = true;
    api.admin.formats().then((found) => { if (live) setFormatList(found); }, () => undefined);
    return () => { live = false; };
  }, [api.admin, libraryVersion]);
  const formatChoices = api.admin ? formatList.map((item) => item.format) : DEMO_FORMATS;
  useEffect(() => {
    if (!api.admin) return;
    let live = true;
    api.admin.crates().then((found) => { if (live) setCrateList(found); }, () => undefined);
    return () => { live = false; };
  }, [api.admin, crateVersion]);
  const addToCrate = async (crateId: number, ids: number[]) => {
    const target = crateList.find((found) => found.id === crateId);
    try {
      const added = await api.admin!.addToCrate(crateId, ids);
      setListNote(added === 0 ? `${target?.name ?? "The crate"} already had ${ids.length === 1 ? "that track" : "those tracks"}.` : `Added ${added} to ${target?.name ?? "the crate"}.`);
      crateChanged();
    } catch (reason) {
      setListNote(`Could not add to ${target?.name ?? "the crate"}: ${reason instanceof Error ? reason.message : "something went wrong"}.`);
    }
  };
  /** Take tracks out of a crate (the open one unless named); several at once ask first, since
   * there is no undo. The question carries its crate, so it can never act on another. */
  const removeFromCrate = async (ids: number[], crateId = crate?.id) => {
    const target = crateList.find((found) => found.id === crateId) ?? (crate?.id === crateId ? crate : null);
    if (!target) return;
    setRemoveAsk(null);
    try {
      const removed = await api.admin!.removeFromCrate(target.id, ids);
      setListNote(`Removed ${removed === 1 ? "1 track" : `${removed} tracks`} from ${target.name}.`);
      crateChanged();
    } catch (reason) {
      setListNote(`Could not remove from ${target.name}: ${reason instanceof Error ? reason.message : "something went wrong"}.`);
    }
  };
  const admin = api.admin;
  /** Where to put a list back after it is read again, for that list only: a different list
   * starts at the top. */
  const [restore, setRestore] = useState<{ query: string; top: number } | null>(null);
  const queryKey = JSON.stringify(query);
  const shownQuery = useRef(queryKey);
  shownQuery.current = queryKey;
  const scrollTop = () => document.querySelector<HTMLElement>("[data-testid=library-scroll]")?.scrollTop ?? 0;
  const [sidebarWidth, setSidebarWidth] = useState(loadSidebarWidth);
  const arrangeable = Boolean(admin) && screen === "tracks" && !crate && !playlist && !focus && sort !== "playlist" && sort !== "crate" && !reversed;
  const libraryOrder = useMemo(() => (admin && arrangeable ? {
    move: async (trackIds: number[], beforeId: number | null) => {
      await admin.moveTracks(trackIds, beforeId, sort);
      if (sort !== "custom") {
        // The list remounts for the new sort; keep the place the person was dragging at.
        setRestore({ query: JSON.stringify({ ...query, sort: "custom" }), top: scrollTop() });
        setSort("custom");
      }
      crateChanged();
    },
  } : null), [admin, arrangeable, sort, crateChanged, query]);
  const crateOrder = useMemo(() => (admin && crate && sort === "crate" ? {
    move: async (trackIds: number[], beforeId: number | null) => {
      // Tracks dragged in from elsewhere join the crate first, then take their place.
      await admin.addToCrate(crate.id, trackIds);
      await admin.moveInCrate(crate.id, trackIds, beforeId);
      crateChanged();
    },
    remove: async (trackIds: number[]) => {
      await admin.removeFromCrate(crate.id, trackIds);
      crateChanged();
    },
  } : null), [admin, crate, sort, crateChanged]);
  const reorder = crateOrder ?? libraryOrder;
  // Read again in place: a save, an import or a cover change keeps the list where it was.
  const refreshLibrary = useCallback(() => {
    setRestore({ query: shownQuery.current, top: scrollTop() });
    api.forgetCovers?.();
    setLibraryVersion((value) => value + 1);
  }, [api]);
  const auto = useAutoImport(api.admin, refreshLibrary);
  // A search stays: an artist or release found by a track's title opens on that track.
  const openItem = (item: LibraryItem) => {
    if (screen === "releases" || screen === "artists") setBackTo({ screen, sort, reversed, top: scrollTop() });
    else if (!crate && !playlist) setBackTo(null);
    // A release or an artist reads in track order, as on the record.
    setFocus(item); setScreen("tracks"); setFormat("all"); setSort("track"); setReversed(false);
  };
  // A release or artist renamed by an edit shows its new name: read it again with the library.
  const focusKey = focus ? `${focus.kind}:${focus.id}` : "";
  useEffect(() => {
    if (!focus || libraryVersion === 0) return;
    const controller = new AbortController();
    const kind = focus.kind;
    const filter = kind === "releases" ? { releaseId: focus.id } : { artistId: focus.id };
    api.read({ ...DEFAULT_QUERY, view: kind, ...filter }, 0, 1, controller.signal).then((found) => {
      const fresh = found.items.find((item) => item.id === focus.id);
      if (fresh) setFocus((current) => (current && current.kind === kind && current.id === fresh.id ? fresh : current));
    }, () => undefined);
    return () => controller.abort();
    // Only the library changing reads it again; `focus` itself is what is refreshed.
  }, [libraryVersion, focusKey, api]);

  /** A track's release or artist, from the track menu. */
  const goTo = (item: LibraryItem, kind: "releases" | "artists") => {
    // Back returns to the crate or playlist the track was found in.
    setBackTo(crate ? { crate, top: scrollTop() } : playlist ? { playlist, top: scrollTop() } : null);
    setPlaylist(null); setCrate(null); setSearch("");
    // Its track count is not known here; the heading uses the list's own count.
    openItem({ ...item, kind, id: kind === "releases" ? item.releaseId : item.artistId, title: kind === "releases" ? item.release : item.artist, trackCount: -1 });
  };
  /** Back from a release or artist: the grid in the order and at the place it was left. */
  const backFromFocus = (kind: View) => {
    if (backTo && "crate" in backTo) {
      // The crate as it is now: renamed since, it keeps its new name; deleted, Back goes to Tracks.
      const again = crateList.find((found) => found.id === backTo.crate.id);
      setBackTo(null);
      if (!again) { navigate("tracks"); return; }
      setRestore({ query: JSON.stringify(queryOf("tracks", "", "crate", "all", null, null, again)), top: backTo.top });
      openCrate(again);
      return;
    }
    if (backTo && "playlist" in backTo) {
      const { playlist: again, top } = backTo;
      setRestore({ query: JSON.stringify(queryOf("tracks", "", "playlist", "all", null, again, null)), top });
      openPlaylist(again, fromBrowser); setBackTo(null);
      return;
    }
    const back = backTo && "screen" in backTo && backTo.screen === kind ? backTo : { screen: kind, sort: "recent" as Sort, reversed: false, top: 0 };
    setRestore({ query: JSON.stringify(queryOf(back.screen, search, back.sort, "all", null, null, null, back.reversed)), top: back.top });
    setScreen(back.screen); setFocus(null); setSort(back.sort); setReversed(back.reversed); setBackTo(null);
  };
  const isLibrary = screen !== "import" && screen !== "sync" && screen !== "cleanup" && screen !== "playlists" && screen !== "crates";
  // Keyed on identity: a release re-read after a cover change is the same page, and keeps its note.
  useEffect(() => { setCoverNote(""); setExportNote(""); setCoverAsk(false); setListNote(""); setRemoveAsk(null); }, [focusKey, crate?.id, playlist?.id, screen]);
  const selectionSet = useMemo(() => new Set(selection), [selection]);
  // A new list starts with nothing chosen.
  useEffect(() => { setSelection([]); setSelected(null); setSelectedIndex(null); }, [query]);
  // Cmd+A (Ctrl+A) outside a text box chooses every track of the list shown, as it does in it.
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey || event.key.toLowerCase() !== "a" || event.defaultPrevented) return;
      if (!isLibrary || query.view !== "tracks" || document.querySelector("dialog[open]")) return;
      const target = event.target as HTMLElement | null;
      if (target && (target.closest("input, textarea, select, [contenteditable=true]"))) return;
      event.preventDefault();
      const key = shownQuery.current;
      api.ids(query).then((ids) => { if (shownQuery.current === key) setSelection(ids); }, (reason: unknown) => setListNote(`Could not select every track: ${reason instanceof Error ? reason.message : "the library could not be asked"}.`));
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [api, isLibrary, query]);
  // Cmd+I (Ctrl+I) edits the chosen tracks, or the selected one.
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.key.toLowerCase() !== "i" || !api.admin) return;
      const ids = selection.length > 0 ? selection : selected?.kind === "tracks" ? [selected.id] : [];
      if (ids.length === 0 || document.querySelector("dialog[open]")) return;
      event.preventDefault();
      setEditing(ids);
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [api.admin, selection, selected]);

  /** Play a whole release, artist, crate or playlist from its first track that can be played. */
  const playAll = async () => {
    const page = await api.read(query, 0, 50, new AbortController().signal).catch(() => null);
    const index = page?.items.findIndex((item) => !item.missing) ?? -1;
    const item = index >= 0 ? page?.items[index] : undefined;
    if (item) setPlayRequest({ item, index, query, nonce: Date.now() });
  };
  const heading = crate?.name ?? playlist?.name ?? focus?.title ?? (screen === "tracks" ? "Tracks" : screen === "releases" ? "Releases" : screen === "recent" ? "Recently imported" : "Artists");
  const batchNote = batches.length === 0
    ? "Nothing has been imported with openCrate yet. Tracks you bring in from the Import screen are listed here, one batch per sitting."
    : batch ? `${batch.tracks === batch.files ? `${batch.tracks} ${batch.tracks === 1 ? "track" : "tracks"}` : `${batch.tracks} of ${batch.files} files`} imported ${describeBatch(batch)}${batch.tracks < batch.files ? ". The rest are not in the library yet: the music folder has not been scanned since, or they were left in the watch folder" : ""}.` : "";

  return <div className="app-shell" style={{ "--sidebar-width": `${sidebarWidth}px` } as CSSProperties}>
    <a className="skip-link" href="#main">Skip to library</a>
    <aside className={`sidebar${preferences.navFolded ? " nav-folded" : ""}`}>
      <a className="brand" href="#main" onClick={() => navigate("tracks")} aria-label="openCrate home"><span className="brand-mark">oc</span><span>openCrate<span className="brand-period">.</span></span></a>
      <div className="nav-head">
        <div className="nav-caption">YOUR LIBRARY</div>
        <button className="icon-button nav-fold" aria-expanded={!preferences.navFolded} aria-label={preferences.navFolded ? "Show the navigation" : "Hide the navigation"} title={preferences.navFolded ? "Show the navigation with its names" : "Hide the navigation behind icons, so crates and playlists get the room"} onClick={() => updatePreferences({ ...preferences, navFolded: !preferences.navFolded })}><Icon name="back" size={14} /></button>
      </div>
      <nav className="main-nav" aria-label="Main navigation">{NAV.map(({ screen: target, name, icon }) => <button key={target} aria-label={name} title={preferences.navFolded ? name : undefined} onClick={() => navigate(target)} aria-current={(focus && !crate && !playlist ? focus.kind : screen) === target ? "page" : undefined} className={target === "import" ? "nav-divider" : ""}><Icon name={icon} /><span>{name}</span>{screen === target && <span className="nav-dot" />}</button>)}</nav>
      <div className="sidebar-lists">
      {api.admin && <Crates admin={api.admin} version={crateVersion} activeId={crate?.id ?? null} onOpen={openCrate} onChanged={crateChanged} onRenamed={(next) => setCrate((current) => (current?.id === next.id ? next : current))} onDeleted={(id) => { if (crate?.id === id) navigate("tracks"); }} onBrowse={() => navigate("crates")} browsing={screen === "crates"} />}
      {api.admin ? <Playlists admin={api.admin} version={libraryVersion} activeId={playlist?.id ?? null} onOpen={(next) => openPlaylist(next)} onChanged={refreshLibrary} browsing={screen === "playlists"} onBrowse={() => { navigate("playlists"); setBrowseFolder(null); }} askFirst={preferences.askBeforePlaylistAdd} onStopAsking={() => updatePreferences({ ...preferences, askBeforePlaylistAdd: false })} /> : <div className="playlist-note"><div className="nav-caption">PLAYLISTS</div><p>From your rekordbox export.</p><span>Needs a library on disk.</span></div>}
      </div>
      <SidebarResizer width={sidebarWidth} onChange={setSidebarWidth} />
      {/* Only the demos need saying: the real library is the normal case. */}
      {!api.admin?.persistent && <div className="sidebar-bottom"><div className="demo-dot" /><div><strong>{api.admin ? "Demo database" : "Demo workspace"}</strong><small>{api.admin ? "A throwaway library; nothing of yours is touched" : "No files connected"}</small></div></div>}
    </aside>
    <div className="workspace">
      <header className="topbar">
        <div className="search-box"><Icon name="search" /><input {...PLAIN_TEXT} ref={searchInput} aria-label="Search library" placeholder="Search tracks, artists, releases" value={search} onChange={(event) => { setSearch(event.target.value); if (!isLibrary) setScreen("tracks"); }} onKeyDown={(event) => {
          // Escape clears the search, and a second one hands the keyboard back to the list.
          if (event.key !== "Escape") return;
          event.preventDefault();
          if (search) setSearch(""); else { event.currentTarget.blur(); document.querySelector<HTMLElement>("[data-testid=track-row]")?.focus(); }
        }} /><kbd>/</kbd>{search && <button className="icon-button" aria-label="Clear search" onClick={() => setSearch("")}><Icon name="close" size={16} /></button>}</div>
        <span className="demo-badge">{api.admin ? (api.admin.persistent ? "LOCAL LIBRARY" : "DEMO DATABASE") : "SYNTHETIC DEMO"}</span>
        <button className="icon-button settings-trigger" aria-label="Settings" title="Settings (Cmd+,)" onClick={() => setSettingsOpen(true)}><Icon name="settings" /></button>
      </header>
      <main id="main" tabIndex={-1} className={preferences.headerFolded ? "header-folded" : undefined}>
        {api.admin && <Offline admin={api.admin} version={libraryVersion} />}
        {api.startupError && <div role="alert" className="inline-error">{api.startupError} Showing the synthetic demo instead; nothing from your library is shown.</div>}
        {settingsError && <div role="alert" className="inline-error">{settingsError}</div>}
        {isLibrary ? <>
          {api.admin && <Welcome admin={api.admin} onOpenRekordbox={() => navigate("sync")} onLibraryChanged={refreshLibrary} />}
          <div className={`library-hero${preferences.headerFolded ? " folded" : ""}`}>
            <div>{focus && <button className="back-link" onClick={() => backFromFocus(focus.kind)}><Icon name="back" size={14} />Back to {backTo && "crate" in backTo ? (crateList.find((found) => found.id === backTo.crate.id)?.name ?? "tracks") : backTo && "playlist" in backTo ? backTo.playlist.name : focus.kind}</button>}{crate && <button className="back-link" onClick={() => { setCrate(null); setSort("recent"); setReversed(false); if (fromCrates) setScreen("crates"); setFromCrates(false); }}><Icon name="back" size={14} />{fromCrates ? "Back to crates" : "Back to tracks"}</button>}{playlist && <button className="back-link" onClick={() => { setPlaylist(null); setSort("recent"); setReversed(false); if (fromBrowser) setScreen("playlists"); setFromBrowser(false); }}><Icon name="back" size={14} />{fromBrowser ? "Back to playlists" : "Back to tracks"}</button>}{(playlist || crate || focus || screen === "recent") && <div className="eyebrow">{playlist ? "REKORDBOX PLAYLIST" : crate ? "CRATE" : focus ? (focus.kind === "artists" ? "ARTIST" : "RELEASE") : "IMPORT BATCH"}</div>}<h1>{heading}</h1>{screen === "recent" ? <p>{batchNote}</p> : playlist ? <p>{playlist.entries === 0 ? "This playlist is empty in rekordbox." : playlist.inLibrary === playlist.entries ? `All ${playlist.entries} ${playlist.entries === 1 ? "track is" : "tracks are"} in your library.` : `${playlist.inLibrary} of ${playlist.entries} tracks are in your library. The rest are not in your folders.`}</p> : focus ? <p>{(() => { const count = focus.trackCount >= 0 ? focus.trackCount : listTotal ?? 0; return `${count} ${count === 1 ? "track" : "tracks"}`; })()}{focus.kind === "releases" ? ` by ${focus.artist}` : ""}</p> : crate ? <p>Kept in openCrate only. rekordbox is not changed.</p> : <p className="page-note">{PAGE_NOTES[screen === "releases" || screen === "artists" ? screen : "tracks"]}</p>}{api.admin && (playlist || crate || focus || (screen === "recent" && batch)) && <button className="play-all" onClick={() => void playAll()}><Icon name="play" size={16} />Play all</button>}{api.admin && screen === "tracks" && listTotal !== 0 && <div className="cover-actions" role="group" aria-label="Export">
              <button className="link-button" onClick={() => void exportList("m3u8")}>Export M3U8 playlist...</button>
              <button className="link-button" onClick={() => void exportList("csv")}>Export CSV...</button>
              {exportNote && <small role="status">{exportNote}</small>}
            </div>}{api.admin && focus?.kind === "releases" && <div className="cover-actions" role="group" aria-label="Cover">
              <button className="link-button" disabled={coverBusy} onClick={() => void changeCover(focus.id, "choose")}>Choose image...</button>
              <button className="link-button" disabled={coverBusy} onClick={() => void changeCover(focus.id, "online")}>Find cover online</button>
              {coverAsk
                ? <span className="crate-confirm" role="alertdialog" aria-label="Remove the cover?" onKeyDown={(event) => { if (event.key === "Escape") setCoverAsk(false); }}>
                  <span>{`The picture goes from every file of ${focus.title}. This cannot be undone.`}</span>
                  <button className="link-button" disabled={coverBusy} onClick={() => { setCoverAsk(false); void changeCover(focus.id, "remove"); }}>Remove cover</button>
                  <button className="link-button" autoFocus onClick={() => setCoverAsk(false)}>Cancel</button>
                </span>
                : <button className="link-button" disabled={coverBusy} onClick={() => setCoverAsk(true)}>Remove cover...</button>}
              {(coverStep || coverNote) && <small role="status">{coverStep || coverNote}</small>}
            </div>}
            {listNote && <small role="status" className="list-note">{listNote}</small>}
            {removeAsk && crate?.id === removeAsk.crateId && <span className="crate-confirm" role="alertdialog" aria-label={`Remove ${removeAsk.ids.length} tracks from ${crate.name}?`} onKeyDown={(event) => { if (event.key === "Escape") setRemoveAsk(null); }}>
              <span>{`${removeAsk.ids.length} tracks leave ${crate.name}. They stay in your library.`}</span>
              <button className="link-button" onClick={() => void removeFromCrate(removeAsk.ids, removeAsk.crateId)}>Remove tracks</button>
              <button className="link-button" autoFocus onClick={() => setRemoveAsk(null)}>Cancel</button>
            </span>}</div>
            <div className="hero-art" aria-hidden="true">{focus?.kind === "artists" ? <ArtistArtwork api={api} artistId={focus.id} color={focus.color} large /> : <Artwork color={focus?.color ?? 1} large src={focus ? api.artworkUrl?.(focus.releaseId) ?? null : null} />}<div className="hero-disc" /></div>
          </div>
          <div className="library-toolbar">
            <div className="format-filters" role="group" aria-label={screen === "recent" ? "Import batch" : "Format filters"}>{screen === "tracks" ? ["all", ...formatChoices].map((value) => <button key={value} aria-pressed={format === value} title={value === "all" ? undefined : `${formatList.find((item) => item.format === value)?.tracks ?? ""} ${value} tracks`.trim()} onClick={() => setFormat(value)}>{value === "all" ? "All tracks" : value}</button>) : screen === "recent" && batches.length > 0 ? <label className="batch-pick"><span className="sr-only">Import batch</span><select value={batch?.id ?? ""} onChange={(event) => setBatch(batches.find((item) => item.id === Number(event.target.value)) ?? null)}>{batches.map((item) => <option key={item.id} value={item.id}>{`${describeBatch(item)} - ${item.tracks} ${item.tracks === 1 ? "track" : "tracks"}`}</option>)}</select></label> : <span />}</div>
            <div className="view-options"><button aria-expanded={!preferences.headerFolded} onClick={() => updatePreferences({ ...preferences, headerFolded: !preferences.headerFolded })}>{preferences.headerFolded ? "Show header" : "Hide header"}</button>{screen === "tracks" && <button aria-pressed={preferences.compact} onClick={() => updatePreferences({ ...preferences, compact: !preferences.compact })}><Icon name="tracks" size={16} />Compact</button>}<label className="sort-label"><span className="sr-only">Sort library</span><select value={sort} onChange={(event) => { setSort(event.target.value as Sort); setReversed(false); }}><option value="recent">Recently added</option><option value="title">Title</option>{screen !== "artists" && <option value="artist">Artist</option>}{screen === "tracks" && <option value="bpm">BPM</option>}{(screen === "tracks" || screen === "recent") && <option value="format">Format</option>}{focus && <option value="track">Track order</option>}{screen === "tracks" && !crate && !playlist && !focus && admin && <option value="custom">Custom order</option>}{playlist && <option value="playlist">Playlist order</option>}{crate && <option value="crate">Crate order</option>}</select></label>{sort !== "playlist" && sort !== "crate" && sort !== "custom" && <button aria-pressed={reversed} aria-label="Reverse the order" title={reversed ? "Back to the usual order" : sort === "recent" ? "Oldest first" : sort === "bpm" ? "BPM high to low" : "Z to A"} onClick={() => setReversed(!reversed)}><span aria-hidden="true" className="reverse-mark">{reversed ? "\u25B2" : "\u25BC"}</span>Reverse</button>}</div>
          </div>
          <Library key={`${JSON.stringify(query)}:${libraryVersion}`} api={api} query={query} compact={preferences.compact} brief={preferences.headerFolded} keyNotation={preferences.keyNotation} selected={selected} onSelect={(item, index) => { setSelected(item); setSelectedIndex(index); }} onOpen={openItem} onPlay={api.admin ? (item, index) => setPlayRequest({ item, index, query, nonce: Date.now() }) : undefined} nowPlaying={nowPlaying} selection={selectionSet} onSelectionChange={setSelection} onEdit={api.admin ? setEditing : undefined} onRemoveFromCrate={api.admin && crate ? (ids) => { if (ids.length > 1) setRemoveAsk({ crateId: crate.id, ids }); else void removeFromCrate(ids); return ids.length > 1; } : undefined} onGoTo={api.admin ? goTo : undefined} crates={crateList} onAddToCrate={api.admin ? (crateId, ids) => void addToCrate(crateId, ids) : undefined} within={crate?.name ?? playlist?.name ?? focus?.title} playlistEmpty={playlist?.entries === 0} onSort={(next) => { if (next === sort) setReversed(!reversed); else { setSort(next); setReversed(false); } }} onQueue={api.admin ? (item, next) => setQueue((current) => (next ? [item, ...current] : [...current, item])) : undefined} dragTracks={Boolean(api.admin)} reorder={reorder} restoreScroll={restore?.query === queryKey ? restore.top : 0} version={crateVersion} onTotal={setListTotal} onOpenSettings={api.admin ? () => setSettingsOpen(true) : undefined} onOpenImport={api.admin ? () => navigate("import") : undefined} />
        </> : <>{screen !== "playlists" && screen !== "crates" && <div className="tool-fold"><button aria-expanded={!preferences.headerFolded} onClick={() => updatePreferences({ ...preferences, headerFolded: !preferences.headerFolded })}>{preferences.headerFolded ? "Show header" : "Hide header"}</button></div>}{screen === "crates" && api.admin ? <CrateBrowser admin={api.admin} crates={crateList} onOpen={(next) => openCrate(next, true)} onChanged={crateChanged} /> : screen === "playlists" && api.admin ? <PlaylistBrowser admin={api.admin} version={libraryVersion} folderId={browseFolder} onFolder={setBrowseFolder} onOpen={(next) => openPlaylist(next, true)} onChanged={refreshLibrary} /> : screen === "sync" && api.admin ? <Sync admin={api.admin} onChanged={refreshLibrary} /> : screen === "cleanup" && api.admin ? <CleanUp admin={api.admin} onChanged={() => { refreshLibrary(); crateChanged(); }} /> : screen === "import" && api.admin ? <Import admin={api.admin} auto={auto} onOpenSettings={() => setSettingsOpen(true)} onImported={refreshLibrary} onCratesChanged={crateChanged} onOpenRecent={() => navigate("recent")} /> : <div className="feature-empty"><div className="empty-icon"><Icon name={screen === "import" ? "folder" : screen === "cleanup" ? "clean" : "sync"} size={36} /></div><h1>{screen === "import" ? "Import" : screen === "cleanup" ? "Clean up" : "rekordbox"}</h1><p>{screen === "import" ? "Watch folders need a library on disk. This demo has none." : screen === "cleanup" ? "Cleaning up needs a library on disk. This demo has none." : "Reading a rekordbox export needs a library on disk. This demo has none."}</p><span className="coming-label">Open the openCrate app to use this.</span><button className="secondary-button" onClick={() => navigate("tracks")}>Back to tracks</button></div>}</>}
      </main>
    </div>
    {editing && api.admin && <EditTracks admin={api.admin} coverUrl={api.coverUrl?.bind(api)} trackIds={editing} onClose={() => setEditing(null)} onSaved={refreshLibrary} />}
    <PlayerBar api={api} version={libraryVersion} selected={selected} selectedIndex={selectedIndex} query={query} request={playRequest} queue={queue} onQueueChange={setQueue} onNowPlaying={setNowPlaying}
      analysis={api.admin && analysed ? <button className="secondary-button analysis-button" onClick={() => setAnalysisOpen(true)}>Cues and grid</button> : null} />
    <dialog ref={analysisDialog} className="settings-dialog" {...closesOutside(() => setAnalysisOpen(false))} aria-labelledby="analysis-title" onCancel={() => setAnalysisOpen(false)} onClose={(event) => { if (closedByBrowser(event.currentTarget)) setAnalysisOpen(false); }}>
      <div className="dialog-heading"><div><div className="eyebrow">FROM REKORDBOX</div><h2 id="analysis-title">Cues and beat grid</h2></div><button autoFocus className="icon-button" aria-label="Close cues and grid" onClick={() => setAnalysisOpen(false)}><Icon name="close" /></button></div>
      <div className="dialog-body">
        {analysisOpen && api.admin && analysed && <><p className="settings-copy"><strong>{analysed.title}</strong> - {analysed.artist}</p><Analysis admin={api.admin} trackId={analysed.id} title={analysed.title} /></>}
      </div>
    </dialog>
    <dialog ref={dialog} className="settings-dialog" {...closesOutside(() => setSettingsOpen(false))} aria-labelledby="settings-title" onCancel={() => setSettingsOpen(false)} onClose={(event) => { if (closedByBrowser(event.currentTarget)) setSettingsOpen(false); }}>
      <div className="dialog-heading"><div><h2 id="settings-title">Settings</h2></div><button autoFocus className="icon-button" aria-label="Close settings" onClick={() => setSettingsOpen(false)}><Icon name="close" /></button></div>
      <div className="dialog-body settings-layout">
      <aside className="settings-side">
        <label className="settings-search"><Icon name="search" size={14} /><input {...PLAIN_TEXT} type="search" aria-label="Search settings" placeholder="Search settings" value={settingsQuery} onChange={(event) => setSettingsQuery(event.target.value)} /></label>
        <nav className="settings-groups" aria-label="Settings groups">{SETTINGS_GROUPS.map((group) => <button key={group.id} type="button" aria-current={settingsGroup === group.id ? "true" : undefined} onClick={() => { setSettingsQuery(""); setSettingsGroup(group.id); requestAnimationFrame(() => settingsBody.current?.querySelector<HTMLElement>(`[data-group="${group.id}"]`)?.scrollIntoView({ block: "start" })); }}>{group.name}</button>)}</nav>
      </aside>
      <div className="settings-main" ref={settingsBody} onScroll={trackSettingsGroup}>
      {settingsQuery.trim() !== "" && <p className="settings-copy settings-match" role="status">{settingsMatches === 0 ? `Nothing in Settings matches "${settingsQuery.trim()}".` : `${settingsMatches} ${settingsMatches === 1 ? "setting matches" : "settings match"} "${settingsQuery.trim()}".`}</p>}
      <div className="settings-group" data-group="appearance"><h2 className="settings-group-title">Appearance</h2>
        <section className="settings-section"><h3>Looks</h3><fieldset className="theme-choice"><legend>Theme</legend>{(["system", "dark", "light"] as const).map((theme) => <label key={theme}><input type="radio" name="theme" value={theme} checked={preferences.theme === theme} onChange={() => updatePreferences({ ...preferences, theme })} />{theme === "system" ? "Match this computer" : theme === "dark" ? "Dark" : "Light"}</label>)}</fieldset><fieldset className="theme-choice"><legend>Key notation</legend>{(["rekordbox", "camelot", "openKey", "musical"] as const).map((notation) => <label key={notation}><input type="radio" name="keyNotation" value={notation} checked={preferences.keyNotation === notation} onChange={() => updatePreferences({ ...preferences, keyNotation: notation })} />{notation === "rekordbox" ? "As rekordbox wrote it" : notation === "camelot" ? "Camelot (8A)" : notation === "openKey" ? "Open Key (1m)" : "Musical (Am)"}</label>)}</fieldset></section>
        <section className="settings-section"><h3>Layout</h3><label className="setting-row"><span><strong>Compact track rows</strong><small>More music in the same space.</small></span><input type="checkbox" checked={preferences.compact} onChange={(event) => updatePreferences({ ...preferences, compact: event.target.checked })} /></label><label className="setting-row"><span><strong>Hide the page header</strong><small>One line for the title, so more rows fit. Also from Hide header above any list.</small></span><input type="checkbox" checked={preferences.headerFolded} onChange={(event) => updatePreferences({ ...preferences, headerFolded: event.target.checked })} /></label><label className="setting-row"><span><strong>Hide the navigation</strong><small>Tracks, Import and the rest as a row of icons, so crates and playlists get the sidebar. Also from the arrow beside YOUR LIBRARY.</small></span><input type="checkbox" checked={preferences.navFolded} onChange={(event) => updatePreferences({ ...preferences, navFolded: event.target.checked })} /></label><label className="setting-row"><span><strong>Reduce motion</strong><small>System reduced-motion preference is also respected.</small></span><input type="checkbox" checked={preferences.reducedMotion} onChange={(event) => updatePreferences({ ...preferences, reducedMotion: event.target.checked })} /></label></section>
      </div>
      {api.admin ? <>
        <div className="settings-group" data-group="library"><h2 className="settings-group-title">Library</h2>
          <section className="settings-section"><Folders admin={api.admin} open={settingsOpen} onChanged={refreshLibrary} /></section>
          <section className="settings-section"><LibraryCheck admin={api.admin} onChanged={refreshLibrary} /></section>
        </div>
        <div className="settings-group" data-group="import"><h2 className="settings-group-title">Import</h2>
          <section className="settings-section"><Downloads admin={api.admin} open={settingsOpen} /></section>
          <section className="settings-section"><FolderLayout admin={api.admin} open={settingsOpen} /></section>
          <section className="settings-section"><LookupKeys admin={api.admin} open={settingsOpen} /></section>
        </div>
        <div className="settings-group" data-group="rekordbox"><h2 className="settings-group-title">rekordbox</h2>
          <section className="settings-section"><RekordboxDatabase admin={api.admin} open={settingsOpen} askFirst={preferences.askBeforePlaylistAdd} onAskFirst={(ask) => updatePreferences({ ...preferences, askBeforePlaylistAdd: ask })} /></section>
        </div>
      </> : <>
        <div className="settings-group" data-group="library"><h2 className="settings-group-title">Library</h2><section className="settings-section"><h3>Music folders</h3><p className="settings-copy">Download and library folders will be chosen with the system folder picker. Folder selection is not connected in this demo.</p></section></div>
        <div className="settings-group" data-group="rekordbox"><h2 className="settings-group-title">rekordbox</h2><section className="settings-section"><h3>rekordbox</h3><p className="settings-copy">XML export will be the default read-back. Reading rekordbox's own library will be off by default and read a backup copy only. Neither is enabled here.</p></section></div>
      </>}
      <div className="settings-footnote">{api.admin ? "Nothing is uploaded. Files and rekordbox change only when you choose a change." : "100,000 synthetic tracks. No music, credentials or personal data."}</div>
      </div>
      </div>
    </dialog>
  </div>;
}
