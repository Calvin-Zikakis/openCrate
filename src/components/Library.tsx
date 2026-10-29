import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { carriesTracks, draggedTracks, setDraggedTracks } from "../library/drag";
import { showKey, type KeyNotation } from "../library/keys";
import type { CrateInfo, LibraryApi, LibraryItem, LibraryPage, LibraryQuery, Sort } from "../library/types";
import { ArtistArtwork, Artwork } from "./Artwork";
import { Icon } from "./Icon";

const EMPTY_PAGE: LibraryPage = { total: 0, start: 0, items: [] };
const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const time = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;

/** The track list's columns, in order. The number and the title are always there; the rest can
 * be hidden or resized by the owner, and both are remembered in this browser. */
export const TRACK_COLUMNS = [
  { id: "number", label: "#", width: 28, fixed: true },
  { id: "title", label: "Title / Artist", width: 0, fixed: true },
  { id: "release", label: "Release", width: 180 },
  { id: "genre", label: "Genre", width: 110 },
  { id: "label", label: "Label", width: 110 },
  { id: "year", label: "Year", width: 48 },
  { id: "bpm", label: "BPM", width: 44 },
  { id: "key", label: "Key", width: 40 },
  { id: "time", label: "Time", width: 42 },
  { id: "format", label: "Format", width: 48 },
  { id: "added", label: "Added", width: 92 },
] as const;
export type TrackColumn = (typeof TRACK_COLUMNS)[number]["id"];
const DEFAULT_HIDDEN_COLUMNS: TrackColumn[] = ["genre", "label", "year"];
const COLUMNS_KEY = "opencrate.trackColumns";
const COLUMN_MIN = 30;
const COLUMN_MAX = 600;

interface ColumnSettings { hidden: TrackColumn[]; widths: Partial<Record<TrackColumn, number>> }
function loadColumns(): ColumnSettings {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(COLUMNS_KEY) ?? "null");
    if (typeof saved === "object" && saved !== null) {
      const { hidden, widths } = saved as Partial<ColumnSettings>;
      const ids = TRACK_COLUMNS.map((column) => column.id as string);
      return {
        hidden: Array.isArray(hidden) ? hidden.filter((id): id is TrackColumn => ids.includes(id) && !("fixed" in (TRACK_COLUMNS.find((column) => column.id === id) ?? {}))) : DEFAULT_HIDDEN_COLUMNS,
        widths: typeof widths === "object" && widths !== null ? Object.fromEntries(Object.entries(widths).filter(([id, width]) => ids.includes(id) && typeof width === "number" && Number.isFinite(width))) as ColumnSettings["widths"] : {},
      };
    }
  } catch { /* storage unavailable or unreadable: the defaults */ }
  return { hidden: DEFAULT_HIDDEN_COLUMNS, widths: {} };
}
function saveColumns(settings: ColumnSettings) {
  try { localStorage.setItem(COLUMNS_KEY, JSON.stringify(settings)); } catch { /* applied, not remembered */ }
}

/** A date in the list: today's time, else the day. */
function addedText(ms: number): string {
  if (ms <= 0) return "";
  const date = new Date(ms);
  const now = new Date();
  if (date.toDateString() === now.toDateString()) return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return date.toLocaleDateString([], { year: date.getFullYear() === now.getFullYear() ? undefined : "2-digit", month: "short", day: "numeric" });
}

interface Props {
  api: LibraryApi;
  query: LibraryQuery;
  compact: boolean;
  /** The header is folded: the list leaves out its how-to hint as well. */
  brief?: boolean;
  /** How keys are shown. */
  keyNotation?: KeyNotation;
  selected: LibraryItem | null;
  onSelect: (item: LibraryItem, index: number) => void;
  onOpen: (item: LibraryItem) => void;
  /** Double-click, Enter or the row's play button on a track. */
  onPlay?: (item: LibraryItem, index: number) => void;
  /** Tracks chosen together (Cmd- or Ctrl-click, Shift-click), for editing or dragging. */
  selection?: ReadonlySet<number>;
  onSelectionChange?: (trackIds: number[]) => void;
  /** Right-click, then "Edit details". */
  onEdit?: (trackIds: number[]) => void;
  /** Take tracks out of the crate shown, in whatever order it is sorted. True when it asks first:
   * the question then has the keyboard, not the list. */
  onRemoveFromCrate?: (trackIds: number[]) => boolean | void;
  /** Open a track's release or artist. */
  onGoTo?: (item: LibraryItem, kind: "releases" | "artists") => void;
  /** The owner's crates, for "Add to a crate" in the track menu. */
  crates?: CrateInfo[];
  onAddToCrate?: (crateId: number, trackIds: number[]) => void;
  /** A heading the list is shown under (a crate, playlist, release or artist), for the summary. */
  within?: string;
  /** A column heading was clicked: sort by it. The heading already sorting the list reverses it. */
  onSort?: (sort: Sort) => void;
  /** The open rekordbox playlist has no entries at all, so an empty list is not a missing folder. */
  playlistEmpty?: boolean;
  /** The loaded track, marked in place of its number. */
  nowPlaying?: { id: number; playing: boolean } | null;
  /** Right-click, then "Play next" (`next`) or "Add to queue". */
  onQueue?: (item: LibraryItem, next: boolean) => void;
  /** Rows can be dragged onto a crate. */
  dragTracks?: boolean;
  /** Set when rows can be dragged to rearrange them: a crate in its own order, or the whole
   * track list. `remove` exists only for a crate. */
  reorder?: {
    move: (trackIds: number[], beforeId: number | null) => Promise<void>;
    remove?: (trackIds: number[]) => Promise<void>;
  } | null;
  /** Scroll here once the first rows arrive (kept across a switch to Custom order). */
  restoreScroll?: number;
  /** Changes when the rows should be read again in place, keeping the scroll position. */
  version?: number;
  /** Told how many rows the list has each time a read finishes (a state setter: it is not a dependency of the read). */
  onTotal?: (total: number) => void;
  /** The empty-library message points here when it has somewhere to send you. */
  onOpenSettings?: () => void;
  onOpenImport?: () => void;
}

/** How close to the top or bottom edge a drag scrolls the list, and how fast. */
const EDGE = 48;
const EDGE_STEP = 18;

/** A small label to drag instead of a picture of the whole row. */
function dragPreview(data: DataTransfer, title: string, artist: string) {
  const label = document.createElement("div");
  label.className = "drag-preview";
  label.textContent = artist ? `${title} - ${artist}` : title;
  document.body.append(label);
  data.setDragImage(label, 16, 16);
  // The browser copies the image when the drag starts; the element is not needed after that.
  setTimeout(() => label.remove(), 0);
}

/** Where a track's BPM comes from: rekordbox's analysis, or the file's tags until then. */
function tempoNote(item: LibraryItem): string | undefined {
  if (item.bpm === null) return undefined;
  return item.analysed ? "Analysed by rekordbox" : "From the file's tags. rekordbox has not analysed a BPM for it.";
}

/** Where a track's key comes from: rekordbox, or the file's tags until rekordbox has one. */
function keyNote(item: LibraryItem): string | undefined {
  if (!item.musicalKey) return undefined;
  return item.rekordboxKey ? "From rekordbox" : "From the file's tags. rekordbox has no key for this track yet.";
}

export function Library({ api, query, compact, brief = false, keyNotation = "rekordbox", selected, onSelect, onOpen, onPlay, onQueue, selection, onSelectionChange, onEdit, onRemoveFromCrate, onGoTo, crates = [], onAddToCrate, within, onSort, playlistEmpty = false, nowPlaying = null, dragTracks = false, reorder = null, version = 0, restoreScroll = 0, onTotal, onOpenSettings, onOpenImport }: Props) {
  /** The row being dragged within this list (null when the drag came from elsewhere), and the
   * gap the drop would fill: the dragged tracks go just before row `insertAt`. */
  const [drag, setDrag] = useState<{ source: number | null; insertAt: number | null } | null>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const space = useRef<HTMLDivElement>(null);
  const [columns, setColumns] = useState(4);
  const [page, setPage] = useState(EMPTY_PAGE);
  const [columnPrefs, setColumnPrefs] = useState<ColumnSettings>(loadColumns);
  const [columnMenu, setColumnMenu] = useState<{ x: number; y: number } | null>(null);
  const columnMenuRef = useRef<HTMLDivElement>(null);
  const columnDrag = useRef<{ id: TrackColumn; startX: number; startWidth: number } | null>(null);
  const visibleColumns = TRACK_COLUMNS.filter((column) => !columnPrefs.hidden.includes(column.id));
  const widthOf = (id: TrackColumn) => Math.round(Math.min(COLUMN_MAX, Math.max(COLUMN_MIN, columnPrefs.widths[id] ?? TRACK_COLUMNS.find((column) => column.id === id)?.width ?? 60)));
  /** The grid both the heading row and every track row follow: the title takes what is left. */
  const columnTemplate = visibleColumns.map((column) => (column.id === "title" ? "minmax(160px, 1fr)" : `${widthOf(column.id)}px`)).join(" ");
  const toggleColumn = (id: TrackColumn) => setColumnPrefs((before) => {
    const next = { ...before, hidden: before.hidden.includes(id) ? before.hidden.filter((other) => other !== id) : [...before.hidden, id] };
    saveColumns(next);
    return next;
  });
  const setColumnWidth = (id: TrackColumn, width: number | null, remember: boolean) => setColumnPrefs((before) => {
    const widths = { ...before.widths };
    if (width === null) delete widths[id]; else widths[id] = Math.round(Math.min(COLUMN_MAX, Math.max(COLUMN_MIN, width)));
    const next = { ...before, widths };
    if (remember) saveColumns(next);
    return next;
  });
  useEffect(() => {
    if (!columnMenu) return;
    const close = (event: Event) => { if (!(event instanceof MouseEvent && columnMenuRef.current?.contains(event.target as Node))) setColumnMenu(null); };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setColumnMenu(null); };
    window.addEventListener("mousedown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", escape);
    return () => { window.removeEventListener("mousedown", close); window.removeEventListener("scroll", close, true); window.removeEventListener("keydown", escape); };
  }, [columnMenu]);
  const [pending, setPending] = useState(true);
  const [error, setError] = useState("");
  const [start, setStart] = useState(0);
  const [retry, setRetry] = useState(0);
  /** The row a Shift-click extends a selection from. */
  const anchor = useRef<number | null>(null);
  /** The tracks an action on `item` covers: the whole selection when the item is in it. */
  const chosen = (item: LibraryItem) => (selection?.has(item.id) && selection.size > 1 ? [...selection] : [item.id]);
  /** What shows as chosen is what Edit, a drag and the menu act on: the chosen tracks, or the
   * selected one when none are. */
  const isChosen = (item: LibraryItem) => (selection && selection.size > 0 ? selection.has(item.id) : selected?.id === item.id);
  /** The track a right-click menu is open for, and where. */
  const [menu, setMenu] = useState<{ item: LibraryItem; index: number; x: number; y: number } | null>(null);
  const isGrid = query.view !== "tracks";
  const perRow = isGrid ? columns : 1;
  const rowHeight = isGrid ? 250 : compact ? 40 : 56;
  const virtual = useVirtualizer({
    count: Math.ceil(page.total / perRow),
    getScrollElement: () => scroll.current,
    estimateSize: () => rowHeight,
    overscan: isGrid ? 2 : 6,
  });

  useEffect(() => {
    const element = scroll.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setColumns(Math.max(2, Math.floor(entry.contentRect.width / 190)));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useEffect(() => { virtual.measure(); }, [rowHeight, virtual]);
  useEffect(() => {
    const controller = new AbortController();
    setPending(true);
    setError("");
    api.read(query, start, 200, controller.signal).then((next) => {
      if (!controller.signal.aborted) {
        setPage(next);
        setPending(false);
        onTotal?.(next.total);
        if (!restored.current) {
          restored.current = true;
          requestAnimationFrame(() => { if (scroll.current) scroll.current.scrollTop = restoreScroll; });
        }
      }
    }).catch((reason: unknown) => {
      if (!controller.signal.aborted) {
        setError(reason instanceof Error ? reason.message : "The library could not be loaded.");
        setPending(false);
      }
    });
    return () => controller.abort();
  }, [api, query, start, retry, version]);

  const rows = virtual.getVirtualItems();
  const firstIndex = (rows[0]?.index ?? 0) * perRow;
  const lastIndex = ((rows.at(-1)?.index ?? 0) + 1) * perRow - 1;
  useEffect(() => {
    if (firstIndex < page.start || lastIndex >= page.start + 200) {
      setStart(Math.max(0, Math.floor(firstIndex / 100) * 100));
    }
  }, [firstIndex, lastIndex, page.start]);

  const focusRow = (index: number) => {
    focusCleanup.current?.();
    focusCleanup.current = null;
    const bounded = Math.max(0, Math.min(page.total - 1, index));
    virtual.scrollToIndex(Math.floor(bounded / perRow));
    const focus = () => {
      const element = scroll.current?.querySelector<HTMLButtonElement>(`[data-index="${bounded}"]`);
      if (element) {
        element.focus();
        return true;
      }
      return false;
    };
    if (focus()) return;
    const observer = new MutationObserver(() => {
      if (focus()) observer.disconnect();
    });
    if (scroll.current) observer.observe(scroll.current, { childList: true, subtree: true });
    // A later keypress or unmount must not leave a focus observer behind.
    focusCleanup.current = () => observer.disconnect();
  };
  const focusCleanup = useRef<(() => void) | null>(null);
  useEffect(() => () => focusCleanup.current?.(), []);

  const idAt = (index: number) => (index >= 0 && index < page.total ? page.items[index - page.start]?.id ?? null : null);
  const reorderable = !isGrid && reorder !== null;

  /** Where a drop at this pointer height would go, from the list's own geometry: the rows
   * themselves are moving to make room, so hit-testing them would flicker. */
  const insertionAt = (clientY: number) => {
    // Measured from the rows' own container, which already accounts for scrolling and any
    // space above the first row.
    const element = space.current;
    if (!element) return null;
    const y = clientY - element.getBoundingClientRect().top;
    return Math.max(0, Math.min(page.total, Math.round(y / rowHeight)));
  };
  /** How far a row slides to open the gap. */
  const shiftOf = (index: number) => {
    if (!drag || drag.insertAt === null) return 0;
    const { source, insertAt } = drag;
    if (source === null) return index >= insertAt ? rowHeight : 0;
    if (insertAt > source && index > source && index < insertAt) return -rowHeight;
    if (insertAt <= source && index >= insertAt && index < source) return rowHeight;
    return 0;
  };
  /** Show the new order at once, before the library confirms it, so nothing snaps back. */
  const showMoved = (trackIds: number[], beforeId: number | null) => {
    setPage((current) => {
      const ids = current.items.map((item) => item.id);
      if (!trackIds.every((id) => ids.includes(id))) return current;
      if (beforeId !== null && !ids.includes(beforeId)) return current;
      if (beforeId === null && current.start + current.items.length < current.total) return current;
      const picked = current.items.filter((item) => trackIds.includes(item.id));
      const rest = current.items.filter((item) => !trackIds.includes(item.id));
      const at = beforeId === null ? rest.length : rest.findIndex((item) => item.id === beforeId);
      rest.splice(at, 0, ...picked);
      return { ...current, items: rest };
    });
  };
  const dropHere = (clientY: number, trackIds: number[]) => {
    const insertAt = insertionAt(clientY);
    const source = drag?.source ?? null;
    setDrag(null);
    if (!reorder || insertAt === null) return;
    if (source !== null && (insertAt === source || insertAt === source + 1)) return;
    const beforeId = insertAt >= page.total ? null : idAt(insertAt);
    // A target outside the loaded rows would read as "the end"; do nothing rather than guess.
    if (insertAt < page.total && beforeId === null) return;
    showMoved(trackIds, beforeId);
    reorder.move(trackIds, beforeId).catch(moveFailed);
  };
  const restored = useRef(restoreScroll === 0);
  /** A column heading: a button when it can sort (and is not a playlist or crate in its own order). */
  const heading = (text: string, sort: Sort, label: string, active = query.sort === sort) => {
    const fixed = query.sort === "playlist" || query.sort === "crate";
    if (!onSort || fixed) return <span>{text}</span>;
    const again = active && sort === query.sort;
    return <button type="button" className={`column-sort${active ? " is-active" : ""}`} aria-pressed={active} title={again ? (query.reversed ? "Back to the usual order" : "Reverse the order") : label} onClick={() => onSort(sort)}>
      {text}{active && <span aria-hidden="true" className="sort-arrow">{query.reversed ? "\u25B2" : "\u25BC"}</span>}
    </button>;
  };
  /** Every track the list holds, chosen at once: Cmd+A (Ctrl+A) or the menu. */
  const selectAll = async () => {
    if (!onSelectionChange || isGrid) return;
    try {
      onSelectionChange(await api.ids(query));
    } catch (reason) {
      setError(`Could not select every track: ${message(reason)}`);
    }
  };
  /** Rows `low` to `high` chosen (Shift); rows past the window drawn are asked for by id. */
  const selectRange = async (low: number, high: number) => {
    if (!onSelectionChange) return;
    const loaded = page.items.filter((_, at) => at + page.start >= low && at + page.start <= high).map((loaded) => loaded.id);
    if (loaded.length === high - low + 1) { onSelectionChange(loaded); return; }
    try {
      onSelectionChange((await api.ids(query)).slice(low, high + 1));
    } catch (reason) {
      setError(`Could not select those rows: ${message(reason)}`);
    }
  };
  /** A move that the library refused: the real order comes back, with the reason. */
  const moveFailed = (reason: unknown) => {
    setError(`The move did not happen: ${message(reason)}`);
    setRetry((value) => value + 1);
  };

  return <section className="library-results" aria-label={`${query.view} results`} aria-busy={pending}>
    <div className="result-summary" role="status">
      {pending ? "Loading..." : `${page.total.toLocaleString()} ${page.total === 1 ? query.view.replace(/s$/, "") : query.view}`}
      <span> {query.search ? `matching "${query.search}"` : within ? `in ${within}` : api.admin ? (api.admin.persistent ? "Your library" : "Demo database") : "Synthetic collection"}</span>
      {selection && selection.size > 1 && <span className="crate-hint">{` ${selection.size.toLocaleString()} chosen`}</span>}
      {reorderable && page.total > 1 && !brief && <span className="crate-hint">{reorder?.remove
        ? " Drag to reorder. Alt+Up/Down moves the focused track; Delete takes it out of the crate."
        : " Drag a track onto a crate, a rekordbox playlist or the player, or within the list to arrange it (Custom order). Right-click for Play next and Add to queue."}</span>}
    </div>
    {!isGrid && <div className="track-header track-columns" role="group" aria-label="Sort by column" title="Right-click to choose columns" style={{ gridTemplateColumns: columnTemplate }} onContextMenu={(event) => { event.preventDefault(); setColumnMenu({ x: event.clientX, y: event.clientY }); }}>
      {/* Headings sort where the library has that order, as the Sort menu does; the rest are labels.
          Every heading but the last carries a handle to drag its width. */}
      {visibleColumns.map((column) => <span key={column.id} className={`track-heading track-heading-${column.id}`}>
        {column.id === "number" ? heading("#", "recent", "Sort by recently added")
          : column.id === "title" ? <span className="heading-pair">{heading("Title", "title", "Sort by title")}<span aria-hidden="true"> / </span>{heading("Artist", "artist", "Sort by artist")}</span>
          : column.id === "bpm" ? heading("BPM", "bpm", "Sort by BPM")
          : column.id === "format" ? heading("Format", "format", "Sort by format")
          : column.id === "added" ? heading("Added", "recent", "Sort by date added")
          : <span>{column.label}</span>}
        {column.id !== "title" && <span className="col-resizer" role="separator" aria-orientation="vertical" aria-label={`Resize ${column.label}`} aria-valuemin={COLUMN_MIN} aria-valuemax={COLUMN_MAX} aria-valuenow={widthOf(column.id)} tabIndex={0}
          onPointerDown={(event) => { if (event.button !== 0) return; event.preventDefault(); event.stopPropagation(); event.currentTarget.setPointerCapture(event.pointerId); columnDrag.current = { id: column.id, startX: event.clientX, startWidth: widthOf(column.id) }; }}
          onPointerMove={(event) => { const active = columnDrag.current; if (active) setColumnWidth(active.id, active.startWidth + event.clientX - active.startX, false); }}
          onPointerUp={() => { if (!columnDrag.current) return; columnDrag.current = null; setColumnPrefs((current) => { saveColumns(current); return current; }); }}
          onPointerCancel={() => { columnDrag.current = null; }}
          onDoubleClick={() => setColumnWidth(column.id, null, true)}
          onKeyDown={(event) => {
            const step = event.shiftKey ? 50 : 10;
            if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); setColumnWidth(column.id, widthOf(column.id) + (event.key === "ArrowRight" ? step : -step), true); }
            else if (event.key === "Home") { event.preventDefault(); setColumnWidth(column.id, null, true); }
          }} />}
      </span>)}
    </div>}
    {columnMenu && <div ref={columnMenuRef} className="column-menu" role="menu" aria-label="Columns" style={{ left: columnMenu.x, top: columnMenu.y }}>
      {TRACK_COLUMNS.map((column) => {
        const fixed = "fixed" in column;
        const shown = !columnPrefs.hidden.includes(column.id);
        return <button key={column.id} type="button" role="menuitemcheckbox" aria-checked={shown} aria-disabled={fixed} onClick={() => { if (!fixed) toggleColumn(column.id); }}>
          <span className="column-check" aria-hidden="true">{shown ? "\u2713" : ""}</span>{column.label}
        </button>;
      })}
      <div className="column-menu-divider" />
      <button type="button" role="menuitem" onClick={() => { setColumnPrefs((before) => { const next = { ...before, widths: {} }; saveColumns(next); return next; }); setColumnMenu(null); }}>Reset column widths</button>
    </div>}
    {error && <div role="alert" className="inline-error">{error} <button onClick={() => setRetry((value) => value + 1)}>Try again</button></div>}
    {!pending && !error && page.total === 0 && (query.crateId !== null && !query.search && query.format === "all"
      ? <div className="empty-state"><Icon name="tracks" size={36} /><h2>This crate is empty</h2><p>Drag tracks from your library onto the crate's name in the sidebar.</p></div>
      : query.playlistId !== null && !query.search && query.format === "all"
        ? (playlistEmpty
          ? <div className="empty-state"><Icon name="queue" size={36} /><h2>This playlist is empty in rekordbox</h2><p>Drag tracks onto its name in the sidebar to add them, with writing to rekordbox on.</p></div>
          : <div className="empty-state"><Icon name="queue" size={36} /><h2>None of this playlist is in your folders</h2><p>rekordbox has these tracks somewhere else. Add the folder they are in as a music folder in Settings to see them here.</p></div>)
      : query.batchId !== null && !query.search
        ? <div className="empty-state"><Icon name="import" size={36} /><h2>Nothing from this import is in the library yet</h2><p>The files were copied, but the music folder has not been scanned since, or every file was left in the watch folder.</p>
          <div className="empty-actions">{onOpenImport && <button className="secondary-button" onClick={onOpenImport}>Go to Import</button>}</div></div>
      : !query.search && query.format === "all" && query.playlistId === null && query.artistId === null && query.releaseId === null && (onOpenSettings || onOpenImport)
        ? <div className="empty-state"><Icon name="tracks" size={36} /><h2>No music in your library yet</h2><p>Add a music folder in Settings, or bring in music from a watch folder with Import.</p>
          <div className="empty-actions">{onOpenSettings && <button className="secondary-button" onClick={onOpenSettings}>Open Settings</button>}{onOpenImport && <button className="secondary-button" onClick={onOpenImport}>Go to Import</button>}</div></div>
        : <div className="empty-state"><Icon name="search" size={36} /><h2>No matches this time</h2><p>Try a different title, artist or release, or clear your filters.</p></div>)}
    <div ref={scroll} className="library-scroll" data-testid="library-scroll" role="region" tabIndex={0} aria-label="Scrollable library" onPointerDown={() => {
      focusCleanup.current?.();
      focusCleanup.current = null;
    }}
    onKeyDown={(event) => {
      if ((event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "a" && onSelectionChange && !isGrid && page.total > 0) {
        event.preventDefault();
        void selectAll();
      }
    }}
    onDragOver={(event) => {
      // Scroll while a drag is held near the top or bottom edge.
      const box = scroll.current?.getBoundingClientRect();
      if (!box || !(dragTracks || reorderable)) return;
      if (event.clientY < box.top + EDGE) scroll.current?.scrollBy(0, -EDGE_STEP);
      else if (event.clientY > box.bottom - EDGE) scroll.current?.scrollBy(0, EDGE_STEP);
      if (!reorderable || !carriesTracks(event.dataTransfer)) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
      const insertAt = insertionAt(event.clientY);
      setDrag((current) => (current?.insertAt === insertAt ? current : { source: current?.source ?? null, insertAt }));
    }}
    onDragLeave={(event) => {
      if (scroll.current?.contains(event.relatedTarget as Node | null)) return;
      setDrag((current) => (current && current.source !== null ? { ...current, insertAt: null } : null));
    }}
    onDrop={reorderable ? (event) => {
      const trackIds = draggedTracks(event.dataTransfer);
      if (!trackIds) { setDrag(null); return; }
      event.preventDefault();
      // Decide from the drop itself, not from state set by the last dragover.
      dropHere(event.clientY, trackIds);
    } : undefined}
    >
      <div ref={space} className="virtual-space" style={{ height: virtual.getTotalSize() }} role="list" aria-label={query.view}>
        {rows.map((row) => <div className={isGrid ? "virtual-row card-row" : "virtual-row"} key={row.key} style={{ height: row.size, transform: `translateY(${row.start}px)`, gridTemplateColumns: isGrid ? `repeat(${columns}, minmax(0, 1fr))` : undefined }}>
          {Array.from({ length: perRow }, (_, column) => {
            const index = row.index * perRow + column;
            if (index >= page.total) return null;
            const item = page.items[index - page.start];
            if (!item) return <div key={index} className="row-placeholder" aria-hidden="true" />;
            const shift = isGrid ? 0 : shiftOf(index);
            const isSource = drag?.source === index;
            return <div role="listitem" aria-posinset={index + 1} aria-setsize={page.total} key={item.id}
              className={drag ? "reorder-slide" : undefined}
              style={shift ? { transform: `translateY(${shift}px)` } : undefined}>
              <button
                className={isGrid ? "release-card" : `track-row track-columns${isChosen(item) ? " is-selected" : ""}${item.missing ? " is-missing" : ""}${isSource ? (drag?.insertAt === null ? " is-drag-source" : " is-drag-gap") : ""}`}
                draggable={!isGrid && (dragTracks || reorderable)}
                onDragStart={(event) => {
                  // Every chosen track goes, loaded or not (Select all reaches past the rows drawn).
                  const many = selection?.has(item.id) && selection.size > 1 ? page.items.filter((loaded) => selection.has(loaded.id)) : [];
                  if (selection?.has(item.id) && selection.size > 1) setDraggedTracks(event.dataTransfer, [...selection], `${selection.size.toLocaleString()} tracks`, many);
                  else setDraggedTracks(event.dataTransfer, [item.id], `${item.title} - ${item.artist}`, [item]);
                  dragPreview(event.dataTransfer, item.title, item.artist);
                  if (reorderable) setDrag({ source: index, insertAt: null });
                  else event.dataTransfer.effectAllowed = "copy";
                }}
                onDragEnd={() => setDrag(null)}
                style={isGrid ? undefined : { height: rowHeight, gridTemplateColumns: columnTemplate }}
                data-index={index} data-testid={isGrid ? "library-card" : "track-row"}
                aria-label={isGrid ? `${item.title}, ${item.trackCount} tracks` : `${item.title}, ${item.artist}`}
                aria-pressed={isGrid ? undefined : isChosen(item)}
                onClick={(event) => {
                  // WebKit does not focus a button on click; the keyboard should carry on from here.
                  event.currentTarget.focus();
                  if (isGrid) {
                    onOpen(item);
                    return;
                  }
                  if ((event.metaKey || event.ctrlKey) && onSelectionChange) {
                    const next = new Set(selection ?? (selected ? [selected.id] : []));
                    if (!next.delete(item.id)) next.add(item.id);
                    onSelectionChange([...next]);
                    anchor.current = index;
                    return;
                  }
                  if (event.shiftKey && onSelectionChange && anchor.current !== null) {
                    const [from, to] = anchor.current < index ? [anchor.current, index] : [index, anchor.current];
                    void selectRange(from, to);
                    return;
                  }
                  anchor.current = index;
                  onSelectionChange?.([item.id]);
                  onSelect(item, index);
                }}
                onDoubleClick={() => { if (!isGrid && onPlay && !item.missing) onPlay(item, index); }}
                onContextMenu={(event) => {
                  if (isGrid || (!onQueue && !onEdit)) return;
                  event.preventDefault();
                  if (!selection?.has(item.id)) {
                    onSelectionChange?.([item.id]);
                    onSelect(item, index);
                  }
                  setMenu({ item, index, x: event.clientX, y: event.clientY });
                }}
                onKeyDown={(event) => {
                  if (!isGrid && onPlay && event.key === "Enter" && !event.altKey && !item.missing) {
                    event.preventDefault();
                    onSelect(item, index);
                    onPlay(item, index);
                    return;
                  }
                  let delta = 0;
                  if (event.key === "ArrowDown") delta = perRow;
                  else if (event.key === "ArrowUp") delta = -perRow;
                  else if (isGrid && event.key === "ArrowRight") delta = 1;
                  else if (isGrid && event.key === "ArrowLeft") delta = -1;
                  if (delta !== 0) {
                    event.preventDefault();
                    const target = index + delta;
                    const next = page.items[target - page.start];
                    // The keyboard selects as it moves, as a click does; Shift takes the rows between.
                    if (!isGrid && !event.altKey && next && target >= 0 && target < page.total) {
                      if (event.shiftKey && onSelectionChange) {
                        const from = anchor.current ?? index;
                        anchor.current = from;
                        const [low, high] = from < target ? [from, target] : [target, from];
                        void selectRange(low, high);
                      } else {
                        anchor.current = target;
                        onSelectionChange?.([next.id]);
                        onSelect(next, target);
                      }
                    }
                    focusRow(target);
                  }
                  if (!isGrid && event.key === "Escape" && selection && selection.size > 1) {
                    event.preventDefault();
                    onSelectionChange?.([item.id]);
                  }
                  if (!isGrid && (onQueue || onEdit) && (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey))) {
                    event.preventDefault();
                    const box = event.currentTarget.getBoundingClientRect();
                    if (!selection?.has(item.id)) {
                      onSelectionChange?.([item.id]);
                      onSelect(item, index);
                    }
                    setMenu({ item, index, x: box.left + 40, y: box.bottom });
                  }
                  if (reorderable && reorder && event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
                    // Alt+Up/Down moves the track within the crate, for those not using a mouse.
                    event.preventDefault();
                    const up = event.key === "ArrowUp";
                    if (up ? index === 0 : index === page.total - 1) return;
                    // A neighbour outside the rows loaded reads as "the end": wait for it rather than guess.
                    const target = up ? idAt(index - 1) : index + 2 < page.total ? idAt(index + 2) : null;
                    if ((up || index + 2 < page.total) && target === null) return;
                    reorder.move([item.id], target).then(() => focusRow(index + (up ? -1 : 1)), moveFailed);
                    return;
                  }
                  if (onRemoveFromCrate && (event.key === "Delete" || event.key === "Backspace")) {
                    // Every chosen track goes, in any order the crate is sorted; focus moves on.
                    event.preventDefault();
                    if (!onRemoveFromCrate(chosen(item))) requestAnimationFrame(() => focusRow(Math.min(index, page.total - 2)));
                    return;
                  }
                  if (event.key === "Home" || event.key === "End") {
                    event.preventDefault();
                    focusRow(event.key === "Home" ? 0 : page.total - 1);
                  }
                }}
              >
                {isGrid ? <>{item.kind === "artists" ? <ArtistArtwork api={api} artistId={item.id} color={item.color} large /> : <Artwork color={item.color} large src={api.artworkUrl?.(item.releaseId) ?? null} />}<strong title={item.title}>{item.title}</strong><span>{item.kind === "artists" ? `${item.trackCount} ${item.trackCount === 1 ? "track" : "tracks"}` : item.artist}</span>{item.kind === "releases" && <small>{`${item.trackCount} ${item.trackCount === 1 ? "track" : "tracks"}`}</small>}</> :
                  visibleColumns.map((column) => {
                    switch (column.id) {
                      case "number": return <span key={column.id} className="track-number">{nowPlaying?.id === item.id
                        ? <span className={`now-bars${nowPlaying.playing ? " is-playing" : ""}`} role="img" aria-label={nowPlaying.playing ? "Now playing" : "Paused"}><i /><i /><i /></span>
                        : index + 1}</span>;
                      case "title": return <span key={column.id} className="track-identity"><Artwork color={item.color} src={(item.kind === "tracks" ? api.coverUrl?.(item.id) : null) ?? api.artworkUrl?.(item.releaseId) ?? null} /><span className="truncate" title={`${item.title} - ${item.artist}`}><strong>{item.title}</strong>{item.missing && <span className="missing-tag" title="The file is not on disk right now">Missing</span>}<small>{item.artist}</small></span></span>;
                      case "release": return <span key={column.id} className="track-release" title={item.release}>{item.release}</span>;
                      case "genre": return <span key={column.id} className="track-release track-genre" title={item.genre}>{item.genre}</span>;
                      case "label": return <span key={column.id} className="track-release track-label" title={item.label}>{item.label}</span>;
                      case "year": return <span key={column.id} className="track-year">{item.year ?? ""}</span>;
                      case "bpm": return <span key={column.id} className={`track-bpm${item.bpm === null ? "" : item.analysed ? " is-analysed" : " from-tags"}`} title={tempoNote(item)}>{item.bpm ?? "--"}</span>;
                      case "key": return <span key={column.id} className={`track-key${!item.rekordboxKey && item.musicalKey ? " from-tags" : ""}`} title={keyNote(item)}>{showKey(item.musicalKey, keyNotation)}</span>;
                      case "time": return <span key={column.id} className="track-time">{time(item.duration)}</span>;
                      case "format": return <span key={column.id} className="track-format">{item.format}</span>;
                      case "added": return <span key={column.id} className="track-added" title={item.addedMs > 0 ? new Date(item.addedMs).toLocaleString() : undefined}>{addedText(item.addedMs)}</span>;
                      default: return null;
                    }
                  })}
              </button>
              {!isGrid && onPlay && !item.missing && <button className="row-play" tabIndex={-1} aria-label={`Play ${item.title}`}
                style={{ height: rowHeight }} onClick={() => { onSelect(item, index); onPlay(item, index); }}><Icon name="play" size={14} /></button>}
            </div>;
          })}
        </div>)}
      </div>
    </div>
    {menu && <RowMenu item={menu.item} x={menu.x} y={menu.y} onClose={() => setMenu(null)} actions={[
      ...(onPlay && !menu.item.missing ? [{ label: "Play now", run: () => onPlay(menu.item, menu.index) }] : []),
      ...(onQueue && !menu.item.missing ? (() => {
        // With several chosen, all of them, in list order (those with a file to play).
        const many = selection?.has(menu.item.id) && selection.size > 1 ? page.items.filter((loaded) => selection.has(loaded.id) && !loaded.missing) : [menu.item];
        // The queue takes loaded rows: a choice reaching past them says how many it covers.
        const count = many.length > 1 ? ` (${many.length}${selection && selection.size > many.length ? ` of ${selection.size.toLocaleString()} chosen` : ""} tracks)` : "";
        return [
          // Play next puts them at the front in their own order.
          { label: `Play next${count}`, run: () => { for (const item of [...many].reverse()) onQueue(item, true); } },
          { label: `Add to queue${count}`, run: () => { for (const item of many) onQueue(item, false); } },
        ];
      })() : []),
      ...(onEdit ? [{ label: chosen(menu.item).length > 1 ? `Edit details (${chosen(menu.item).length} tracks)...` : "Edit details...", hint: "Cmd+I", run: () => onEdit(chosen(menu.item)) }] : []),
      ...(onAddToCrate && crates.length > 0 ? [{
        label: chosen(menu.item).length > 1 ? `Add ${chosen(menu.item).length} tracks to a crate` : "Add to a crate",
        children: crates.filter((found) => found.id !== query.crateId).map((found) => ({ label: found.name, run: () => onAddToCrate(found.id, chosen(menu.item)) })),
        run: () => undefined,
      }] : []),
      ...(onSelectionChange && page.total > 1 && (selection?.size ?? 0) < page.total ? [{ label: `Select all ${page.total.toLocaleString()} tracks`, hint: "Cmd+A", run: () => void selectAll() }] : []),
      ...(onSelectionChange && selection && selection.size > 1 ? [{ label: "Clear the selection", hint: "Esc", run: () => onSelectionChange([menu.item.id]) }] : []),
      ...(onRemoveFromCrate ? [{ label: chosen(menu.item).length > 1 ? `Remove ${chosen(menu.item).length} tracks from the crate...` : "Remove from the crate", run: () => { const at = menu.index; if (!onRemoveFromCrate(chosen(menu.item))) requestAnimationFrame(() => focusRow(Math.min(at, page.total - 2))); } }] : []),
      ...(onGoTo ? [
        ...(query.releaseId === null ? [{ label: `Go to the release (${menu.item.release})`, run: () => onGoTo(menu.item, "releases") }] : []),
        ...(query.artistId === null ? [{ label: `Go to the artist (${menu.item.artist})`, run: () => onGoTo(menu.item, "artists") }] : []),
      ] : []),
    ]} />}
  </section>;
}

/** A track's right-click menu. It closes on a choice, a click elsewhere, Escape or a scroll, and
 * stays inside the window. */
/** One line of the track menu; `children` open a second list in its place (crates to add to). */
interface MenuAction { label: string; run: () => void; hint?: string; children?: MenuAction[] }

function RowMenu({ item, x, y, actions, onClose }: { item: LibraryItem; x: number; y: number; actions: MenuAction[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  // Focus goes back where the menu was opened from, unless a choice moved it on (a dialog).
  const opener = useRef<HTMLElement | null>(document.activeElement instanceof HTMLElement ? document.activeElement : null);
  useEffect(() => () => {
    const at = document.activeElement;
    if (!at || at === document.body || ref.current?.contains(at)) opener.current?.focus({ preventScroll: true });
  }, []);
  /** The second list shown instead of the first, and its heading. */
  const [inner, setInner] = useState<{ title: string; actions: MenuAction[] } | null>(null);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const box = element.getBoundingClientRect();
    element.style.left = `${Math.max(8, Math.min(x, window.innerWidth - box.width - 8))}px`;
    element.style.top = `${Math.max(8, Math.min(y, window.innerHeight - box.height - 8))}px`;
    element.querySelector<HTMLElement>("[role=menuitem]")?.focus({ preventScroll: true });
  }, [x, y]);
  useEffect(() => {
    // A scroll from just before the menu opened arrives a frame later; only a later one closes it.
    let armed = false;
    let frame = requestAnimationFrame(() => { frame = requestAnimationFrame(() => { armed = true; }); });
    const close = (event: Event) => {
      if (event instanceof MouseEvent && ref.current?.contains(event.target as Node)) return;
      if (event.type === "scroll" && !armed) return;
      onClose();
    };
    const key = (event: KeyboardEvent) => { if (event.key === "Escape" || event.key === "Tab") onClose(); };
    window.addEventListener("mousedown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", key);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("mousedown", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", key);
    };
  }, [onClose]);
  useEffect(() => { ref.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus({ preventScroll: true }); }, [inner]);
  const shown = inner?.actions ?? actions;
  return <div ref={ref} className="column-menu row-menu" role="menu" aria-label={inner ? inner.title : item.title} style={{ left: x, top: y }}
    onKeyDown={(event) => {
      if (event.key === "ArrowLeft" && inner) { event.preventDefault(); setInner(null); return; }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      const items = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("[role=menuitem]"));
      const at = items.indexOf(document.activeElement as HTMLElement);
      items[(at + (event.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
    }}>
    {inner && <button type="button" role="menuitem" className="menu-back" onClick={() => setInner(null)}><Icon name="back" size={12} />{inner.title}</button>}
    {shown.map((action, at) => action.children
      ? <button key={`${at}:${action.label}`} type="button" role="menuitem" aria-haspopup="menu" onClick={() => setInner({ title: action.label, actions: action.children ?? [] })}>{action.label}<span className="menu-hint" aria-hidden="true">{"\u203A"}</span></button>
      : <button key={`${at}:${action.label}`} type="button" role="menuitem" onClick={() => { action.run(); onClose(); }}>{action.label}{action.hint && <span className="menu-hint" aria-hidden="true">{action.hint}</span>}</button>)}
    {inner && shown.length === 0 && <p className="settings-copy">Every crate already has these tracks.</p>}
  </div>;
}
