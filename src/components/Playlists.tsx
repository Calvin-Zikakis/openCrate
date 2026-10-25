import { useEffect, useMemo, useRef, useState, type DragEvent } from "react";
import { carriesCrate, carriesTracks, draggedCrate, draggedTracks } from "../library/drag";
import type { LibraryAdmin, PlaylistInfo } from "../library/types";
import { Icon } from "./Icon";
import { inWindow } from "./menuPlace";
import { PLAIN_TEXT } from "./plainText";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
/** How long a confirmation stays up; a refusal stays until the next drop. */
const NOTE_MS = 4000;

interface Props {
  admin: LibraryAdmin;
  /** Changes when the library may have changed, so the list is read again. */
  version: number;
  activeId: number | null;
  onOpen: (playlist: PlaylistInfo) => void;
  /** rekordbox's library was written and read again. */
  onChanged?: () => void;
  /** Show the folders and playlists in the main area, as a browser; `active` while it shows. */
  onBrowse?: () => void;
  browsing?: boolean;
}

type Kind = "folder" | "playlist";

/** On a Mac, Control-click is a right-click: it opens the menu and must not also open the row. */
const macRightClick = (event: React.MouseEvent) => event.ctrlKey && /Mac/i.test(navigator.platform);

/** rekordbox's own tree starts at an invisible root folder; show what is inside it. */
export function startOf(rows: PlaylistInfo[]): number | null {
  const top = rows.filter((row) => row.parentId === null);
  const [only] = top;
  return top.length === 1 && only?.isFolder && only.name === "ROOT" ? only.id : null;
}

export function Playlists({ admin, version, activeId, onOpen, onChanged, onBrowse, browsing = false }: Props) {
  const [rows, setRows] = useState<PlaylistInfo[]>([]);
  const [open, setOpen] = useState<Set<number>>(new Set());
  const [failed, setFailed] = useState(false);
  const opened = useRef(false);
  const [over, setOver] = useState<number | null>(null);
  const [note, setNote] = useState<{ text: string; failed: boolean; done: boolean } | null>(null);
  /** A crate dropped on a folder (null: the top of the tree), waiting for the owner to say yes. */
  const [asking, setAsking] = useState<{ crate: { id: number; name: string }; folder: PlaylistInfo | null } | null>(null);
  /** A new rekordbox folder or playlist being named, inside `parent` (null: the top of the tree). */
  const [naming, setNaming] = useState<{ parent: PlaylistInfo | null; name: string; kind: Kind } | null>(null);
  /** What can be made, and where: in `parent` (null: the top of the tree). */
  const [menu, setMenu] = useState<{ parent: PlaylistInfo | null; x: number; y: number; playlist?: PlaylistInfo } | null>(null);
  /** Whether a playlist or folder can be made: writing to rekordbox on, and its version confirmed. */
  const [canWrite, setCanWrite] = useState<boolean | null>(null);
  useEffect(() => {
    let live = true;
    admin.rekordboxDatabase().then((status) => {
      if (live) setCanWrite(status.enabled && status.writeEnabled && !status.needsConfirmation && !status.writeNeedsConfirmation);
    }, () => { if (live) setCanWrite(false); });
    return () => { live = false; };
  }, [admin, version]);
  const cannotWrite = canWrite === false ? "Needs writing to rekordbox on in Settings" : undefined;
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    // Focus goes back where the menu was opened from once it closes, unless a choice moved it on.
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const menuElement = menuRef.current;
    menuElement?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    const close = (event: MouseEvent) => { if (!menuRef.current?.contains(event.target as Node)) setMenu(null); };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape" || event.key === "Tab") setMenu(null); };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", escape);
      const at = document.activeElement;
      if (!at || at === document.body || menuElement?.contains(at)) requestAnimationFrame(() => { if (!document.activeElement || document.activeElement === document.body) opener?.focus({ preventScroll: true }); });
    };
  }, [menu]);

  useEffect(() => {
    if (!note?.done || note.failed) return;
    const timer = setTimeout(() => setNote(null), NOTE_MS);
    return () => clearTimeout(timer);
  }, [note]);

  /** A write to rekordbox is under way; another waits until it is done. */
  const busy = note !== null && !note.done;

  /** Tracks dropped on a playlist go into rekordbox's own playlist, when writing is on. */
  const drop = async (row: PlaylistInfo, trackIds: number[]) => {
    setNote({ text: `Adding to ${row.name} in rekordbox...`, failed: false, done: false });
    try {
      const done = await admin.addToRekordboxPlaylist(row.id, trackIds);
      const added = done.added === 0 ? `Already in ${row.name} in rekordbox.` : `Added ${done.added} to ${row.name} in rekordbox.`;
      const left = done.leftOut.length > 0 ? ` ${done.leftOut.length} could not go in: ${done.leftOut.join("; ")}` : "";
      setNote({ text: added + left, failed: false, done: true });
      onChanged?.();
    } catch (reason) {
      setNote({ text: message(reason), failed: true, done: true });
    }
  };

  /** Start naming a new folder or playlist; one inside a folder opens it so the name field shows. */
  const startNaming = (parent: PlaylistInfo | null, kind: Kind) => {
    setMenu(null);
    if (parent) setOpen((current) => new Set(current).add(parent.id));
    setNaming({ parent, name: "", kind });
  };
  const makeNamed = async () => {
    if (!naming) return;
    const { parent, name, kind } = naming;
    // Another write still under way (its note shows) keeps the name typed for Enter again.
    if (busy && name.trim()) return;
    setNaming(null);
    if (!name.trim()) return;
    setNote({ text: `Making the ${kind} ${name.trim()} in rekordbox...`, failed: false, done: false });
    try {
      const made = kind === "folder" ? await admin.createRekordboxFolder(name, parent?.id ?? null) : await admin.createRekordboxPlaylist(name, parent?.id ?? null);
      setNote({ text: `Made the ${kind} ${made.name} ${parent ? `in ${parent.name}` : "at the top of the playlists"} in rekordbox.`, failed: false, done: true });
      onChanged?.();
    } catch (reason) {
      setNote({ text: message(reason), failed: true, done: true });
    }
  };
  /** The menu of what can be made in `parent`, at the pointer or below the focused button. */
  const openMenu = (event: React.MouseEvent | React.KeyboardEvent, parent: PlaylistInfo | null, playlist?: PlaylistInfo) => {
    event.preventDefault();
    event.stopPropagation();
    if ("clientX" in event && event.clientX > 0) setMenu({ parent, x: event.clientX, y: event.clientY, playlist });
    else {
      const box = event.currentTarget.getBoundingClientRect();
      setMenu({ parent, x: box.left + 12, y: box.bottom, playlist });
    }
  };
  const menuKeys = (parent: PlaylistInfo | null, playlist?: PlaylistInfo) => (event: React.KeyboardEvent) => {
    if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) openMenu(event, parent, playlist);
  };
  /** The folder a row sits in, or null at the top of the tree. */
  const parentOf = (row: PlaylistInfo) => rows.find((found) => found.id === row.parentId && found.id !== startOf(rows)) ?? null;
  const nameInput = <input {...PLAIN_TEXT}
    className="crate-name-input"
    aria-label={`New ${naming?.kind ?? "folder"}${naming?.parent ? ` in ${naming.parent.name}` : " name"}`}
    autoFocus
    maxLength={100}
    value={naming?.name ?? ""}
    onChange={(event) => setNaming((current) => current && { ...current, name: event.target.value })}
    onKeyDown={(event) => {
      if (event.key === "Enter") { event.preventDefault(); void makeNamed(); }
      if (event.key === "Escape") { event.preventDefault(); setNaming(null); }
    }}
    onBlur={() => { if (naming?.name.trim()) void makeNamed(); else setNaming(null); }}
  />;

  /** A crate dropped on a folder becomes a new rekordbox playlist there; the crate stays. */
  const makePlaylist = async (crate: { id: number; name: string }, folder: PlaylistInfo | null) => {
    setAsking(null);
    const where = folder ? `in ${folder.name}` : "at the top of the playlists";
    setNote({ text: `Making a playlist from ${crate.name} in rekordbox...`, failed: false, done: false });
    try {
      const done = await admin.crateToRekordboxPlaylist(crate.id, folder?.id ?? null);
      const left = done.leftOut.length > 0 ? ` ${done.leftOut.length} could not go in: ${done.leftOut.join("; ")}` : "";
      setNote({ text: `Made ${done.playlist} ${where} in rekordbox with ${done.added} ${done.added === 1 ? "track" : "tracks"}.${left}`, failed: false, done: true });
      if (folder) setOpen((current) => new Set(current).add(folder.id));
      onChanged?.();
    } catch (reason) {
      setNote({ text: message(reason), failed: true, done: true });
    }
  };
  /** Drag handlers for a place a crate can be dropped: a folder, or the top of the tree (`key` -1). */
  const crateTarget = (folder: PlaylistInfo | null) => {
    const key = folder?.id ?? -1;
    return {
      onDragOver: (event: DragEvent) => {
        if (!carriesCrate(event.dataTransfer) || busy) return;
        event.preventDefault();
        event.stopPropagation();
        event.dataTransfer.dropEffect = "copy";
        setOver(key);
      },
      onDragLeave: () => setOver((current) => (current === key ? null : current)),
      onDrop: (event: DragEvent) => {
        setOver(null);
        const crate = draggedCrate(event.dataTransfer);
        if (!crate || busy) return;
        event.preventDefault();
        event.stopPropagation();
        setAsking({ crate, folder });
      },
    };
  };

  useEffect(() => {
    let stale = false;
    admin.playlists().then((next) => {
      if (stale) return;
      setRows(next);
      setFailed(false);
      // A question about a folder that has gone is dropped; any other stays open.
      setAsking((current) => current?.folder && !next.some((row) => row.id === current.folder?.id) ? null : current);
      if (!opened.current && next.length > 0) {
        opened.current = true;
        const first = startOf(next);
        setOpen(new Set(next.filter((row) => row.isFolder && row.parentId === first).map((row) => row.id)));
      }
    }, () => { if (!stale) setFailed(true); });
    return () => { stale = true; };
  }, [admin, version]);

  const children = useMemo(() => {
    const map = new Map<number | null, PlaylistInfo[]>();
    for (const row of rows) map.set(row.parentId, [...(map.get(row.parentId) ?? []), row]);
    return map;
  }, [rows]);

  const caption = <>
    <div className="crate-head">
      {onBrowse
        ? <button className={`nav-caption caption-button${over === -1 ? " is-drop-target" : ""}`} aria-current={browsing ? "page" : undefined} title="Show every folder and playlist" onClick={onBrowse} onContextMenu={(event) => openMenu(event, null)} onKeyDown={menuKeys(null)} {...crateTarget(null)}>PLAYLISTS</button>
        : <div className={`nav-caption${over === -1 ? " is-drop-target" : ""}`} onContextMenu={(event) => openMenu(event, null)} {...crateTarget(null)}>PLAYLISTS</div>}
      <button className="icon-button crate-add" aria-label="New rekordbox playlist or folder" aria-haspopup="menu" title={cannotWrite ?? "New playlist or folder"} disabled={busy || canWrite === false} onClick={(event) => openMenu(event, null)}>+</button>
    </div>
    {naming && naming.parent === null && nameInput}
  </>;
  const footer = <>
    {asking && <div className="crate-confirm" role="alertdialog" aria-label={`Make a rekordbox playlist from ${asking.crate.name}?`}
      onKeyDown={(event) => { if (event.key === "Escape") setAsking(null); }}>
      <span>Make a rekordbox playlist from {asking.crate.name} {asking.folder ? `in ${asking.folder.name}` : "at the top of rekordbox's playlists"}? The crate stays as it is.</span>
      <button className="link-button" disabled={busy} onClick={() => void makePlaylist(asking.crate, asking.folder)}>Make playlist</button>
      {/* Focus starts on Cancel: Enter alone should not write to rekordbox. */}
      <button className="link-button" autoFocus onClick={() => setAsking(null)}>Cancel</button>
    </div>}
    {note && <p className={note.failed ? "crate-error" : "crate-note"} role={note.failed ? "alert" : "status"}>{note.text}</p>}
    {menu && <div ref={menuRef} className="column-menu" role="menu" aria-label={menu.parent ? `${menu.parent.name} actions` : "New in playlists"} style={inWindow(menu.x, menu.y)}>
      {menu.playlist && <button type="button" role="menuitem" onClick={() => { onOpen(menu.playlist!); setMenu(null); }}>{`Open ${menu.playlist.name}`}</button>}
      <button type="button" role="menuitem" disabled={busy || canWrite === false} title={cannotWrite} onClick={() => startNaming(menu.parent, "playlist")}><Icon name="queue" size={14} />{menu.parent ? `New playlist in ${menu.parent.name}` : "New playlist"}</button>
      <button type="button" role="menuitem" disabled={busy || canWrite === false} title={cannotWrite} onClick={() => startNaming(menu.parent, "folder")}><Icon name="folder" size={14} />{menu.parent ? `New folder in ${menu.parent.name}` : "New folder"}</button>
      {cannotWrite && <p className="settings-copy menu-note">{cannotWrite}.</p>}
    </div>}
  </>;
  if (rows.length === 0) {
    return <div className="playlist-note">{caption}
      <p>{failed ? "Playlists could not be read." : "None yet."}</p>
      <span>{failed ? "Try reading your export again." : "Read rekordbox on the rekordbox screen to see its playlists here."}</span>{footer}</div>;
  }
  const toggle = (id: number) => setOpen((current) => {
    const next = new Set(current);
    if (!next.delete(id)) next.add(id);
    return next;
  });
  const render = (parent: number | null, depth: number) => (children.get(parent) ?? []).map((row) => row.isFolder
    ? <li key={row.id}>
      <button className={`tree-folder${over === row.id ? " is-drop-target" : ""}`} aria-expanded={open.has(row.id)} style={{ paddingLeft: 12 + depth * 12 }} onClick={(event) => { if (!macRightClick(event)) toggle(row.id); }} {...crateTarget(row)}
        onContextMenu={(event) => openMenu(event, row)} onKeyDown={menuKeys(row)}>
        <span aria-hidden="true">{open.has(row.id) ? "\u25BE" : "\u25B8"}</span><span className="truncate" title={row.name}>{row.name}</span>
      </button>
      {open.has(row.id) && <ul>{naming?.parent?.id === row.id && <li style={{ paddingLeft: 12 + (depth + 1) * 12 }}>{nameInput}</li>}{render(row.id, depth + 1)}</ul>}
    </li>
    : <li key={row.id}>
      <button className={`tree-playlist${over === row.id ? " is-drop-target" : ""}`} aria-current={activeId === row.id ? "true" : undefined} style={{ paddingLeft: 12 + depth * 12 }}
        title={`${row.name}: ${row.inLibrary} of ${row.entries} entries are in your library`} onClick={(event) => { if (!macRightClick(event)) onOpen(row); }}
        onContextMenu={(event) => openMenu(event, parentOf(row), row)} onKeyDown={menuKeys(parentOf(row), row)}
        onDragOver={(event) => {
          if (!carriesTracks(event.dataTransfer) || busy) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = "copy";
          setOver(row.id);
        }}
        onDragLeave={() => setOver((current) => (current === row.id ? null : current))}
        onDrop={(event) => {
          setOver(null);
          const ids = draggedTracks(event.dataTransfer);
          if (!ids || busy) return;
          event.preventDefault();
          void drop(row, ids);
        }}>
        <span className="truncate">{row.name}</span><small>{row.inLibrary}/{row.entries}</small>
      </button>
    </li>);
  return <nav className="playlist-tree" aria-label="Playlists" onContextMenu={(event) => openMenu(event, null)}>
    {caption}<ul>{render(startOf(rows), 0)}</ul>
    {footer}
  </nav>;
}

interface BrowserProps {
  admin: LibraryAdmin;
  version: number;
  /** The folder shown; null for the top of rekordbox's tree. */
  folderId: number | null;
  onFolder: (folderId: number | null) => void;
  onOpen: (playlist: PlaylistInfo) => void;
}

/** rekordbox's folders and playlists in the main area, a folder at a time, like a file browser:
 * a folder opens in place, a playlist opens its tracks. */
export function PlaylistBrowser({ admin, version, folderId, onFolder, onOpen }: BrowserProps) {
  const [rows, setRows] = useState<PlaylistInfo[] | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let stale = false;
    admin.playlists().then((next) => { if (!stale) { setRows(next); setFailed(false); } }, () => { if (!stale) setFailed(true); });
    return () => { stale = true; };
  }, [admin, version]);

  const start = rows ? startOf(rows) : null;
  // A folder that has gone (removed in rekordbox since) shows the top instead.
  const current = rows?.find((row) => row.id === folderId && row.isFolder) ?? null;
  const shownId = current?.id ?? start;
  const items = (rows ?? []).filter((row) => row.parentId === shownId);
  const trail: PlaylistInfo[] = [];
  for (let at = current; at && at.id !== start; at = rows?.find((row) => row.id === at?.parentId) ?? null) trail.unshift(at);
  const inside = (folder: PlaylistInfo) => (rows ?? []).filter((row) => row.parentId === folder.id).length;

  return <div className="playlist-browser">
    <div className="library-hero folded">
      <div><h1>Playlists</h1></div>
    </div>
    <nav className="browser-trail" aria-label="Folder path">
      <button className="link-button" aria-current={current ? undefined : "page"} onClick={() => onFolder(null)}>rekordbox</button>
      {trail.map((folder) => <span key={folder.id}><span aria-hidden="true"> / </span><button className="link-button" aria-current={folder.id === current?.id ? "page" : undefined} onClick={() => onFolder(folder.id)}>{folder.name}</button></span>)}
    </nav>
    {failed && <p role="alert" className="inline-error">Playlists could not be read. Try reading rekordbox again on the rekordbox screen.</p>}
    {rows && rows.length === 0 && !failed && <p className="settings-copy">No playlists yet. Read rekordbox on the rekordbox screen to see its folders and playlists here.</p>}
    {rows && rows.length > 0 && items.length === 0 && <p className="settings-copy">This folder is empty. Right-click it in the sidebar to make a playlist or folder in it.</p>}
    {items.length > 0 && <ul className="browser-grid" aria-label={current ? `In ${current.name}` : "Playlists and folders"}>{items.map((row) => <li key={row.id}>
      <button className="browser-card" onClick={() => (row.isFolder ? onFolder(row.id) : onOpen(row))}>
        <span className={`browser-icon${row.isFolder ? " is-folder" : ""}`} aria-hidden="true"><Icon name={row.isFolder ? "folder" : "queue"} size={30} /></span>
        <strong className="truncate">{row.name}</strong>
        <small>{row.isFolder ? `${inside(row)} ${inside(row) === 1 ? "item" : "items"}` : row.entries === 0 ? "Empty" : `${row.inLibrary} of ${row.entries} in your library`}</small>
      </button>
    </li>)}</ul>}
  </div>;
}
