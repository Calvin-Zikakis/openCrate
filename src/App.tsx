import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
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
import { useAutoImport } from "./components/AutoImport";
import { PlayerBar, type PlayRequest } from "./components/PlayerBar";
import { Sync } from "./components/Sync";
import { DEFAULT_QUERY, type Format, type LibraryApi, type LibraryItem, type CrateInfo, type PlaylistInfo, type Sort, type View } from "./library/types";
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

type Screen = View | "import" | "sync" | "cleanup" | "playlists";
const NAV: { screen: Screen; name: string; icon: IconName }[] = [
  { screen: "tracks", name: "Tracks", icon: "tracks" },
  { screen: "releases", name: "Releases", icon: "releases" },
  { screen: "artists", name: "Artists", icon: "artists" },
  { screen: "import", name: "Import", icon: "import" },
  { screen: "sync", name: "rekordbox", icon: "sync" },
  { screen: "cleanup", name: "Clean up", icon: "clean" },
];

/** One line under the title saying what the page holds. */
const PAGE_NOTES = {
  tracks: "All your tracks live here, from every music folder.",
  releases: "All your releases live here, one card per album or EP.",
  artists: "All your artists live here, with the releases they are on.",
} as const;

export function App({ api }: { api: LibraryApi }) {
  const [screen, setScreen] = useState<Screen>("tracks");
  /** The folder the playlist browser shows (null: the top), and whether the open playlist came from it. */
  const [browseFolder, setBrowseFolder] = useState<number | null>(null);
  const [fromBrowser, setFromBrowser] = useState(false);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<Sort>("recent");
  const [format, setFormat] = useState<Format | "all">("all");
  const [focus, setFocus] = useState<LibraryItem | null>(null);
  const [playlist, setPlaylist] = useState<PlaylistInfo | null>(null);
  const [crate, setCrate] = useState<CrateInfo | null>(null);
  const [crateVersion, setCrateVersion] = useState(0);
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
  const changeCover = async (releaseId: number, action: "choose" | "online" | "remove") => {
    if (!api.admin) return;
    setCoverBusy(true);
    setCoverNote("");
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
    }
  };
  /** The loaded track and whether it plays, marked in the list. */
  const [nowPlaying, setNowPlaying] = useState<{ id: number; playing: boolean } | null>(null);
  const [preferences, setPreferences] = useState(DEFAULT_PREFERENCES);
  const [settingsError, setSettingsError] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
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
    setScreen(next); setFocus(null); setPlaylist(null); setCrate(null); setSearch(""); setFormat("all"); setSort("recent"); setFromBrowser(false);
  };
  const query = useMemo(() => ({
    ...DEFAULT_QUERY, view: screen === "import" || screen === "sync" || screen === "cleanup" || screen === "playlists" ? "tracks" as const : screen,
    search, sort, format: screen === "tracks" ? format : "all" as const,
    artistId: focus?.kind === "artists" ? focus.id : null,
    releaseId: focus?.kind === "releases" ? focus.id : null,
    playlistId: playlist?.id ?? null,
    crateId: crate?.id ?? null,
  }), [screen, search, sort, format, focus, playlist, crate]);
  // A new list says how long it is once read; until then the last count does not apply.
  useEffect(() => { setListTotal(null); }, [query, libraryVersion]);
  const openPlaylist = (next: PlaylistInfo, browsed = false) => {
    setScreen("tracks"); setFocus(null); setPlaylist(next); setCrate(null); setSearch(""); setFormat("all"); setSort("playlist"); setFromBrowser(browsed);
  };
  const openCrate = (next: CrateInfo) => {
    setScreen("tracks"); setFocus(null); setPlaylist(null); setCrate(next); setSearch(""); setFormat("all"); setSort("crate");
  };
  const crateChanged = useCallback(() => setCrateVersion((value) => value + 1), []);
  const admin = api.admin;
  const [restoreScroll, setRestoreScroll] = useState(0);
  const [sidebarWidth, setSidebarWidth] = useState(loadSidebarWidth);
  const arrangeable = Boolean(admin) && screen === "tracks" && !crate && !playlist && !focus && sort !== "playlist" && sort !== "crate";
  const libraryOrder = useMemo(() => (admin && arrangeable ? {
    move: async (trackIds: number[], beforeId: number | null) => {
      await admin.moveTracks(trackIds, beforeId, sort);
      if (sort !== "custom") {
        // The list remounts for the new sort; keep the place the person was dragging at.
        setRestoreScroll(document.querySelector<HTMLElement>("[data-testid=library-scroll]")?.scrollTop ?? 0);
        setSort("custom");
      }
      crateChanged();
    },
  } : null), [admin, arrangeable, sort, crateChanged]);
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
  const refreshLibrary = useCallback(() => setLibraryVersion((value) => value + 1), []);
  const auto = useAutoImport(api.admin, refreshLibrary);
  // A search stays: an artist or release found by a track's title opens on that track.
  const openItem = (item: LibraryItem) => {
    setFocus(item); setScreen("tracks"); setFormat("all");
  };
  const isLibrary = screen !== "import" && screen !== "sync" && screen !== "cleanup" && screen !== "playlists";
  useEffect(() => { setCoverNote(""); setExportNote(""); }, [focus, crate, playlist, screen]);
  const selectionSet = useMemo(() => new Set(selection), [selection]);
  // A new list starts with nothing chosen.
  useEffect(() => { setSelection([]); }, [query]);
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
  const heading = crate?.name ?? playlist?.name ?? focus?.title ?? (screen === "tracks" ? "Tracks" : screen === "releases" ? "Releases" : "Artists");

  return <div className="app-shell" style={{ "--sidebar-width": `${sidebarWidth}px` } as CSSProperties}>
    <a className="skip-link" href="#main">Skip to library</a>
    <aside className="sidebar">
      <a className="brand" href="#main" onClick={() => navigate("tracks")} aria-label="openCrate home"><span className="brand-mark">oc</span><span>openCrate<span className="brand-period">.</span></span></a>
      <div className="nav-caption">YOUR LIBRARY</div>
      <nav aria-label="Main navigation">{NAV.map(({ screen: target, name, icon }) => <button key={target} aria-label={name} onClick={() => navigate(target)} aria-current={(focus && !crate && !playlist ? focus.kind : screen) === target ? "page" : undefined} className={target === "import" ? "nav-divider" : ""}><Icon name={icon} /><span>{name}</span>{screen === target && <span className="nav-dot" />}</button>)}</nav>
      <div className="sidebar-lists">
      {api.admin && <Crates admin={api.admin} version={crateVersion} activeId={crate?.id ?? null} onOpen={openCrate} onChanged={crateChanged} onRenamed={(next) => setCrate((current) => (current?.id === next.id ? next : current))} onDeleted={(id) => { if (crate?.id === id) navigate("tracks"); }} />}
      {api.admin ? <Playlists admin={api.admin} version={libraryVersion} activeId={playlist?.id ?? null} onOpen={(next) => openPlaylist(next)} onChanged={refreshLibrary} browsing={screen === "playlists"} onBrowse={() => { navigate("playlists"); setBrowseFolder(null); }} /> : <div className="playlist-note"><div className="nav-caption">PLAYLISTS</div><p>From your rekordbox export.</p><span>Needs a library on disk.</span></div>}
      </div>
      <SidebarResizer width={sidebarWidth} onChange={setSidebarWidth} />
      <div className="sidebar-bottom"><div className="demo-dot" /><div><strong>{api.admin ? (api.admin.persistent ? "Local library" : "Demo database") : "Demo workspace"}</strong><small>{api.admin ? "Changes only what you choose" : "No files connected"}</small></div></div>
    </aside>
    <div className="workspace">
      <header className="topbar">
        <div className="search-box"><Icon name="search" /><input {...PLAIN_TEXT} ref={searchInput} aria-label="Search library" placeholder="Search tracks, artists, releases" value={search} onChange={(event) => { setSearch(event.target.value); if (!isLibrary) setScreen("tracks"); }} /><kbd>/</kbd>{search && <button className="icon-button" aria-label="Clear search" onClick={() => setSearch("")}><Icon name="close" size={16} /></button>}</div>
        <span className="demo-badge">{api.admin ? (api.admin.persistent ? "LOCAL LIBRARY" : "DEMO DATABASE") : "SYNTHETIC DEMO"}</span>
        <button className="icon-button settings-trigger" aria-label="Settings" onClick={() => setSettingsOpen(true)}><Icon name="settings" /></button>
      </header>
      <main id="main" tabIndex={-1}>
        {api.admin && <Offline admin={api.admin} version={libraryVersion} />}
        {api.startupError && <div role="alert" className="inline-error">{api.startupError} Showing the synthetic demo instead; nothing from your library is shown.</div>}
        {settingsError && <div role="alert" className="inline-error">{settingsError}</div>}
        {isLibrary ? <>
          {api.admin && <Welcome admin={api.admin} onOpenRekordbox={() => navigate("sync")} onLibraryChanged={refreshLibrary} />}
          <div className={`library-hero${preferences.headerFolded ? " folded" : ""}`}>
            <div>{focus && <button className="back-link" onClick={() => { setScreen(focus.kind); setFocus(null); setSort("recent"); }}><Icon name="back" size={14} />Back to {focus.kind}</button>}{crate && <button className="back-link" onClick={() => { setCrate(null); setSort("recent"); }}><Icon name="back" size={14} />Back to tracks</button>}{playlist && <button className="back-link" onClick={() => { setPlaylist(null); setSort("recent"); if (fromBrowser) setScreen("playlists"); setFromBrowser(false); }}><Icon name="back" size={14} />{fromBrowser ? "Back to playlists" : "Back to tracks"}</button>}{(playlist || crate || focus) && <div className="eyebrow">{playlist ? "REKORDBOX PLAYLIST" : crate ? "CRATE" : focus?.kind === "artists" ? "ARTIST" : "RELEASE"}</div>}<h1>{heading}</h1>{playlist ? <p>{`${playlist.inLibrary} of ${playlist.entries} tracks are in your library. The rest are not in your folders.`}</p> : focus ? <p>{`${focus.trackCount} ${focus.trackCount === 1 ? "track" : "tracks"}`}{focus.kind === "releases" ? ` by ${focus.artist}` : ""}</p> : crate ? <p>Kept in openCrate only. rekordbox is not changed.</p> : <p className="page-note">{PAGE_NOTES[screen === "releases" || screen === "artists" ? screen : "tracks"]}</p>}{api.admin && (playlist || crate || focus) && <button className="play-all" onClick={() => void playAll()}><Icon name="play" size={16} />Play all</button>}{api.admin && screen === "tracks" && listTotal !== 0 && <div className="cover-actions" role="group" aria-label="Export">
              <button className="link-button" onClick={() => void exportList("m3u8")}>Export M3U8 playlist...</button>
              <button className="link-button" onClick={() => void exportList("csv")}>Export CSV...</button>
              {exportNote && <small role="status">{exportNote}</small>}
            </div>}{api.admin && focus?.kind === "releases" && <div className="cover-actions" role="group" aria-label="Cover">
              <button className="link-button" disabled={coverBusy} onClick={() => void changeCover(focus.id, "choose")}>Choose cover...</button>
              <button className="link-button" disabled={coverBusy} onClick={() => void changeCover(focus.id, "online")}>Find cover online</button>
              <button className="link-button" disabled={coverBusy} onClick={() => void changeCover(focus.id, "remove")}>Remove cover</button>
              {coverNote && <small role="status">{coverNote}</small>}
            </div>}</div>
            <div className="hero-art" aria-hidden="true">{focus?.kind === "artists" ? <ArtistArtwork api={api} artistId={focus.id} color={focus.color} large /> : <Artwork color={focus?.color ?? 1} large src={focus ? api.artworkUrl?.(focus.releaseId) ?? null : null} />}<div className="hero-disc" /></div>
          </div>
          <div className="library-toolbar">
            <div className="format-filters" role="group" aria-label="Format filters">{screen === "tracks" ? (["all", "AIFF", "MP3", "FLAC"] as const).map((value) => <button key={value} aria-pressed={format === value} onClick={() => setFormat(value)}>{value === "all" ? "All tracks" : value}</button>) : <span />}</div>
            <div className="view-options"><button aria-expanded={!preferences.headerFolded} onClick={() => updatePreferences({ ...preferences, headerFolded: !preferences.headerFolded })}>{preferences.headerFolded ? "Show header" : "Hide header"}</button>{screen === "tracks" && <button aria-pressed={preferences.compact} onClick={() => updatePreferences({ ...preferences, compact: !preferences.compact })}><Icon name="tracks" size={16} />Compact</button>}<label className="sort-label"><span className="sr-only">Sort library</span><select value={sort} onChange={(event) => setSort(event.target.value as Sort)}><option value="recent">Recently added</option><option value="title">Title</option><option value="artist">Artist</option>{screen === "tracks" && <option value="bpm">BPM</option>}{screen === "tracks" && !crate && !playlist && !focus && admin && <option value="custom">Custom order</option>}{playlist && <option value="playlist">Playlist order</option>}{crate && <option value="crate">Crate order</option>}</select></label></div>
          </div>
          <Library key={`${JSON.stringify(query)}:${libraryVersion}`} api={api} query={query} compact={preferences.compact} brief={preferences.headerFolded} keyNotation={preferences.keyNotation} selected={selected} onSelect={(item, index) => { setSelected(item); setSelectedIndex(index); }} onOpen={openItem} onPlay={api.admin ? (item, index) => setPlayRequest({ item, index, query, nonce: Date.now() }) : undefined} nowPlaying={nowPlaying} selection={selectionSet} onSelectionChange={setSelection} onEdit={api.admin ? setEditing : undefined} onQueue={api.admin ? (item, next) => setQueue((current) => (next ? [item, ...current] : [...current, item])) : undefined} dragTracks={Boolean(api.admin)} reorder={reorder} restoreScroll={restoreScroll} version={crateVersion} onTotal={setListTotal} onOpenSettings={api.admin ? () => setSettingsOpen(true) : undefined} onOpenImport={api.admin ? () => navigate("import") : undefined} />
        </> : screen === "playlists" && api.admin ? <PlaylistBrowser admin={api.admin} version={libraryVersion} folderId={browseFolder} onFolder={setBrowseFolder} onOpen={(next) => openPlaylist(next, true)} /> : screen === "sync" && api.admin ? <Sync admin={api.admin} onChanged={refreshLibrary} /> : screen === "cleanup" && api.admin ? <CleanUp admin={api.admin} onChanged={() => { refreshLibrary(); crateChanged(); }} /> : screen === "import" && api.admin ? <Import admin={api.admin} auto={auto} onOpenSettings={() => setSettingsOpen(true)} onImported={refreshLibrary} /> : <div className="feature-empty"><div className="empty-icon"><Icon name={screen === "import" ? "folder" : screen === "cleanup" ? "clean" : "sync"} size={36} /></div><h1>{screen === "import" ? "Import" : screen === "cleanup" ? "Clean up" : "rekordbox"}</h1><p>{screen === "import" ? "Watching download folders needs a library on disk. This demo has none." : screen === "cleanup" ? "Cleaning up needs a library on disk. This demo has none." : "Reading a rekordbox export needs a library on disk. This demo has none."}</p><span className="coming-label">Open the openCrate app to use this.</span><button className="secondary-button" onClick={() => navigate("tracks")}>Back to tracks</button></div>}
      </main>
    </div>
    {editing && api.admin && <EditTracks admin={api.admin} coverUrl={api.coverUrl?.bind(api)} trackIds={editing} onClose={() => setEditing(null)} onSaved={refreshLibrary} />}
    <PlayerBar api={api} version={libraryVersion} selected={selected} selectedIndex={selectedIndex} query={query} request={playRequest} queue={queue} onQueueChange={setQueue} onNowPlaying={setNowPlaying}
      analysis={api.admin && selected?.kind === "tracks" ? <button className="secondary-button analysis-button" onClick={() => setAnalysisOpen(true)}>Cues and grid</button> : null} />
    <dialog ref={analysisDialog} className="settings-dialog" {...closesOutside(() => setAnalysisOpen(false))} aria-labelledby="analysis-title" onCancel={() => setAnalysisOpen(false)} onClose={(event) => { if (closedByBrowser(event.currentTarget)) setAnalysisOpen(false); }}>
      <div className="dialog-heading"><div><div className="eyebrow">FROM REKORDBOX</div><h2 id="analysis-title">Cues and beat grid</h2></div><button autoFocus className="icon-button" aria-label="Close cues and grid" onClick={() => setAnalysisOpen(false)}><Icon name="close" /></button></div>
      <div className="dialog-body">
        {analysisOpen && api.admin && selected?.kind === "tracks" && <><p className="settings-copy"><strong>{selected.title}</strong> - {selected.artist}</p><Analysis admin={api.admin} trackId={selected.id} title={selected.title} /></>}
      </div>
    </dialog>
    <dialog ref={dialog} className="settings-dialog" {...closesOutside(() => setSettingsOpen(false))} aria-labelledby="settings-title" onCancel={() => setSettingsOpen(false)} onClose={(event) => { if (closedByBrowser(event.currentTarget)) setSettingsOpen(false); }}>
      <div className="dialog-heading"><div><h2 id="settings-title">Settings</h2></div><button autoFocus className="icon-button" aria-label="Close settings" onClick={() => setSettingsOpen(false)}><Icon name="close" /></button></div>
      <div className="dialog-body">
      <h3>Appearance</h3><fieldset className="theme-choice"><legend>Theme</legend>{(["system", "dark", "light"] as const).map((theme) => <label key={theme}><input type="radio" name="theme" value={theme} checked={preferences.theme === theme} onChange={() => updatePreferences({ ...preferences, theme })} />{theme === "system" ? "Match this computer" : theme === "dark" ? "Dark" : "Light"}</label>)}</fieldset><fieldset className="theme-choice"><legend>Key notation</legend>{(["rekordbox", "camelot", "openKey", "musical"] as const).map((notation) => <label key={notation}><input type="radio" name="keyNotation" value={notation} checked={preferences.keyNotation === notation} onChange={() => updatePreferences({ ...preferences, keyNotation: notation })} />{notation === "rekordbox" ? "As rekordbox wrote it" : notation === "camelot" ? "Camelot (8A)" : notation === "openKey" ? "Open Key (1m)" : "Musical (Am)"}</label>)}</fieldset><label className="setting-row"><span><strong>Compact track rows</strong><small>More music in the same space.</small></span><input type="checkbox" checked={preferences.compact} onChange={(event) => updatePreferences({ ...preferences, compact: event.target.checked })} /></label><label className="setting-row"><span><strong>Reduce motion</strong><small>System reduced-motion preference is also respected.</small></span><input type="checkbox" checked={preferences.reducedMotion} onChange={(event) => updatePreferences({ ...preferences, reducedMotion: event.target.checked })} /></label>
      {settingsError && <p role="alert" className="inline-error">{settingsError}</p>}
      {api.admin ? <><Folders admin={api.admin} open={settingsOpen} onChanged={refreshLibrary} /><Downloads admin={api.admin} open={settingsOpen} /><FolderLayout admin={api.admin} open={settingsOpen} /><LibraryCheck admin={api.admin} onChanged={refreshLibrary} /><LookupKeys admin={api.admin} open={settingsOpen} /><RekordboxDatabase admin={api.admin} open={settingsOpen} /></> : <><h3>Music folders</h3><p className="settings-copy">Download and library folders will be chosen with the system folder picker. Folder selection is not connected in this demo.</p><h3>rekordbox</h3><p className="settings-copy">XML export will be the default read-back. Reading rekordbox's own library will be off by default and read a backup copy only. Neither is enabled here.</p></>}
      <div className="settings-footnote">{api.admin ? "Nothing is uploaded. Files and rekordbox change only when you choose a change." : "100,000 synthetic tracks. No music, credentials or personal data."}</div>
      </div>
    </dialog>
  </div>;
}
