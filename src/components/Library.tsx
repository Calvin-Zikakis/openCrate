import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { carriesTracks, draggedTracks, setDraggedTracks } from "../library/drag";
import type { LibraryApi, LibraryItem, LibraryPage, LibraryQuery } from "../library/types";
import { ArtistArtwork, Artwork } from "./Artwork";
import { Icon } from "./Icon";

const EMPTY_PAGE: LibraryPage = { total: 0, start: 0, items: [] };
const time = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;

interface Props {
  api: LibraryApi;
  query: LibraryQuery;
  compact: boolean;
  selected: LibraryItem | null;
  onSelect: (item: LibraryItem, index: number) => void;
  onOpen: (item: LibraryItem) => void;
  /** Double-click, Enter or the row's play button on a track. */
  onPlay?: (item: LibraryItem, index: number) => void;
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

export function Library({ api, query, compact, selected, onSelect, onOpen, onPlay, onQueue, dragTracks = false, reorder = null, version = 0, restoreScroll = 0 }: Props) {
  /** The row being dragged within this list (null when the drag came from elsewhere), and the
   * gap the drop would fill: the dragged tracks go just before row `insertAt`. */
  const [drag, setDrag] = useState<{ source: number | null; insertAt: number | null } | null>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const space = useRef<HTMLDivElement>(null);
  const [columns, setColumns] = useState(4);
  const [page, setPage] = useState(EMPTY_PAGE);
  const [pending, setPending] = useState(true);
  const [error, setError] = useState("");
  const [start, setStart] = useState(0);
  const [retry, setRetry] = useState(0);
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
    void reorder.move(trackIds, beforeId);
  };
  const restored = useRef(restoreScroll === 0);

  return <section className="library-results" aria-label={`${query.view} results`} aria-busy={pending}>
    <div className="result-summary" role="status">
      {pending ? "Loading..." : `${page.total.toLocaleString()} ${query.view}`}
      <span> {query.search ? `matching "${query.search}"` : api.admin ? (api.admin.persistent ? "Your library" : "Demo database") : "Synthetic collection"}</span>
      {reorderable && page.total > 1 && <span className="crate-hint">{reorder?.remove
        ? " Drag to reorder. Alt+Up/Down moves the focused track; Delete takes it out of the crate."
        : " Drag tracks to arrange them; the list switches to Custom order. Alt+Up/Down moves the focused track."}</span>}
    </div>
    {!isGrid && <div className="track-header track-columns" aria-hidden="true"><span>#</span><span>Title / Artist</span><span>Release</span><span>BPM</span><span>Key</span><span>Time</span><span>Format</span></div>}
    {error && <div role="alert" className="inline-error">{error} <button onClick={() => setRetry((value) => value + 1)}>Try again</button></div>}
    {!pending && !error && page.total === 0 && (query.crateId !== null && !query.search && query.format === "all"
      ? <div className="empty-state"><Icon name="tracks" size={36} /><h2>This crate is empty</h2><p>Drag tracks from your library onto the crate's name in the sidebar.</p></div>
      : <div className="empty-state"><Icon name="search" size={36} /><h2>No matches this time</h2><p>Try a different title, artist or release, or clear your filters.</p></div>)}
    <div ref={scroll} className="library-scroll" data-testid="library-scroll" role="region" tabIndex={0} aria-label="Scrollable library" onPointerDown={() => {
      focusCleanup.current?.();
      focusCleanup.current = null;
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
                className={isGrid ? "release-card" : `track-row track-columns${selected?.id === item.id ? " is-selected" : ""}${item.missing ? " is-missing" : ""}${isSource ? (drag?.insertAt === null ? " is-drag-source" : " is-drag-gap") : ""}`}
                draggable={!isGrid && (dragTracks || reorderable)}
                onDragStart={(event) => {
                  setDraggedTracks(event.dataTransfer, [item.id], `${item.title} - ${item.artist}`, [item]);
                  dragPreview(event.dataTransfer, item.title, item.artist);
                  if (reorderable) setDrag({ source: index, insertAt: null });
                  else event.dataTransfer.effectAllowed = "copy";
                }}
                onDragEnd={() => setDrag(null)}
                style={isGrid ? undefined : { height: rowHeight }}
                data-index={index} data-testid={isGrid ? "library-card" : "track-row"}
                aria-label={isGrid ? `${item.title}, ${item.trackCount} tracks` : `${item.title}, ${item.artist}`}
                aria-pressed={isGrid ? undefined : selected?.id === item.id}
                onClick={(event) => {
                  // WebKit does not focus a button on click; the keyboard should carry on from here.
                  event.currentTarget.focus();
                  if (isGrid) onOpen(item); else onSelect(item, index);
                }}
                onDoubleClick={() => { if (!isGrid && onPlay && !item.missing) onPlay(item, index); }}
                onContextMenu={(event) => {
                  if (isGrid || !onQueue || item.missing) return;
                  event.preventDefault();
                  onSelect(item, index);
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
                    focusRow(index + delta);
                  }
                  if (reorderable && reorder && event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
                    // Alt+Up/Down moves the track within the crate, for those not using a mouse.
                    event.preventDefault();
                    const up = event.key === "ArrowUp";
                    if (up ? index === 0 : index === page.total - 1) return;
                    void reorder.move([item.id], up ? idAt(index - 1) : idAt(index + 2)).then(() => focusRow(index + (up ? -1 : 1)));
                    return;
                  }
                  if (reorderable && reorder?.remove && (event.key === "Delete" || event.key === "Backspace")) {
                    event.preventDefault();
                    void reorder.remove([item.id]);
                    return;
                  }
                  if (event.key === "Home" || event.key === "End") {
                    event.preventDefault();
                    focusRow(event.key === "Home" ? 0 : page.total - 1);
                  }
                }}
              >
                {isGrid ? <>{item.kind === "artists" ? <ArtistArtwork api={api} artistId={item.id} color={item.color} large /> : <Artwork color={item.color} large src={api.artworkUrl?.(item.releaseId) ?? null} />}<strong>{item.title}</strong><span>{item.kind === "artists" ? `${item.trackCount} ${item.trackCount === 1 ? "track" : "tracks"}` : item.artist}</span>{item.kind === "releases" && <small>{`${item.trackCount} ${item.trackCount === 1 ? "track" : "tracks"}`}</small>}</> :
                  <><span className="track-number">{index + 1}</span><span className="track-identity"><Artwork color={item.color} src={api.artworkUrl?.(item.releaseId) ?? null} /><span className="truncate"><strong>{item.title}</strong><small>{item.artist}</small></span></span><span className="track-release">{item.release}</span><span className="track-bpm">{item.bpm ?? "--"}</span><span className="track-key">{item.musicalKey}</span><span className="track-time">{time(item.duration)}</span><span className="track-format">{item.missing ? <span className="missing-tag" title="The file is not on disk right now">Missing</span> : item.format}</span></>}
              </button>
              {!isGrid && onPlay && !item.missing && <button className="row-play" tabIndex={-1} aria-label={`Play ${item.title}`}
                style={{ height: rowHeight }} onClick={() => { onSelect(item, index); onPlay(item, index); }}><Icon name="play" size={14} /></button>}
            </div>;
          })}
        </div>)}
      </div>
    </div>
    {menu && <RowMenu item={menu.item} x={menu.x} y={menu.y} onClose={() => setMenu(null)} actions={[
      ...(onPlay ? [{ label: "Play now", run: () => onPlay(menu.item, menu.index) }] : []),
      ...(onQueue ? [{ label: "Play next", run: () => onQueue(menu.item, true) }, { label: "Add to queue", run: () => onQueue(menu.item, false) }] : []),
    ]} />}
  </section>;
}

/** A track's right-click menu. It closes on a choice, a click elsewhere, Escape or a scroll, and
 * stays inside the window. */
function RowMenu({ item, x, y, actions, onClose }: { item: LibraryItem; x: number; y: number; actions: { label: string; run: () => void }[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
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
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
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
  return <div ref={ref} className="column-menu row-menu" role="menu" aria-label={item.title} style={{ left: x, top: y }}
    onKeyDown={(event) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      const items = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("[role=menuitem]"));
      const at = items.indexOf(document.activeElement as HTMLElement);
      items[(at + (event.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
    }}>
    {actions.map((action) => <button key={action.label} type="button" role="menuitem" onClick={() => { action.run(); onClose(); }}>{action.label}</button>)}
  </div>;
}
