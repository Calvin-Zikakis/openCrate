import { useEffect, useMemo, useRef, useState, type DragEvent } from "react";
import { carriesCrate, carriesTracks, draggedCrate, draggedTracks } from "../library/drag";
import type { LibraryAdmin, PlaylistInfo } from "../library/types";
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
}

/** rekordbox's own tree starts at an invisible root folder; show what is inside it. */
function startOf(rows: PlaylistInfo[]): number | null {
  const top = rows.filter((row) => row.parentId === null);
  const [only] = top;
  return top.length === 1 && only?.isFolder && only.name === "ROOT" ? only.id : null;
}

export function Playlists({ admin, version, activeId, onOpen, onChanged }: Props) {
  const [rows, setRows] = useState<PlaylistInfo[]>([]);
  const [open, setOpen] = useState<Set<number>>(new Set());
  const [failed, setFailed] = useState(false);
  const opened = useRef(false);
  const [over, setOver] = useState<number | null>(null);
  const [note, setNote] = useState<{ text: string; failed: boolean; done: boolean } | null>(null);
  /** A crate dropped on a folder (null: the top of the tree), waiting for the owner to say yes. */
  const [asking, setAsking] = useState<{ crate: { id: number; name: string }; folder: PlaylistInfo | null } | null>(null);
  /** A new rekordbox folder being named, inside `parent` (null: the top of the tree). */
  const [naming, setNaming] = useState<{ parent: PlaylistInfo | null; name: string } | null>(null);
  const [menu, setMenu] = useState<{ folder: PlaylistInfo; x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    menuRef.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    const close = (event: MouseEvent) => { if (!menuRef.current?.contains(event.target as Node)) setMenu(null); };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setMenu(null); };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", escape);
    return () => { window.removeEventListener("mousedown", close); window.removeEventListener("keydown", escape); };
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

  /** Start naming a new folder; one inside a folder opens it so the name field shows. */
  const startFolder = (parent: PlaylistInfo | null) => {
    setMenu(null);
    if (parent) setOpen((current) => new Set(current).add(parent.id));
    setNaming({ parent, name: "" });
  };
  const makeFolder = async () => {
    if (!naming) return;
    const { parent, name } = naming;
    setNaming(null);
    if (!name.trim() || busy) return;
    setNote({ text: `Making the folder ${name.trim()} in rekordbox...`, failed: false, done: false });
    try {
      const made = await admin.createRekordboxFolder(name, parent?.id ?? null);
      setNote({ text: `Made the folder ${made.name} ${parent ? `in ${parent.name}` : "at the top of the playlists"} in rekordbox.`, failed: false, done: true });
      onChanged?.();
    } catch (reason) {
      setNote({ text: message(reason), failed: true, done: true });
    }
  };
  const nameInput = <input {...PLAIN_TEXT}
    className="crate-name-input"
    aria-label={naming?.parent ? `New folder in ${naming.parent.name}` : "New rekordbox folder name"}
    autoFocus
    maxLength={100}
    value={naming?.name ?? ""}
    onChange={(event) => setNaming((current) => current && { ...current, name: event.target.value })}
    onKeyDown={(event) => {
      if (event.key === "Enter") { event.preventDefault(); void makeFolder(); }
      if (event.key === "Escape") { event.preventDefault(); setNaming(null); }
    }}
    onBlur={() => { if (naming?.name.trim()) void makeFolder(); else setNaming(null); }}
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
      <div className={`nav-caption${over === -1 ? " is-drop-target" : ""}`} {...crateTarget(null)}>PLAYLISTS</div>
      <button className="icon-button crate-add" aria-label="New rekordbox folder" title="New rekordbox folder" disabled={busy} onClick={() => startFolder(null)}>+</button>
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
    {menu && <div ref={menuRef} className="column-menu" role="menu" aria-label={`${menu.folder.name} actions`} style={{ left: menu.x, top: menu.y }}>
      <button type="button" role="menuitem" disabled={busy} onClick={() => startFolder(menu.folder)}>New folder inside</button>
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
      <button className={`tree-folder${over === row.id ? " is-drop-target" : ""}`} aria-expanded={open.has(row.id)} style={{ paddingLeft: 12 + depth * 12 }} onClick={() => toggle(row.id)} {...crateTarget(row)}
        onContextMenu={(event) => { event.preventDefault(); setMenu({ folder: row, x: event.clientX, y: event.clientY }); }}
        onKeyDown={(event) => {
          if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
            event.preventDefault();
            const box = event.currentTarget.getBoundingClientRect();
            setMenu({ folder: row, x: box.left + 12, y: box.bottom });
          }
        }}>
        <span aria-hidden="true">{open.has(row.id) ? "v" : ">"}</span><span className="truncate">{row.name}</span>
      </button>
      {open.has(row.id) && <ul>{naming?.parent?.id === row.id && <li style={{ paddingLeft: 12 + (depth + 1) * 12 }}>{nameInput}</li>}{render(row.id, depth + 1)}</ul>}
    </li>
    : <li key={row.id}>
      <button className={`tree-playlist${over === row.id ? " is-drop-target" : ""}`} aria-current={activeId === row.id ? "true" : undefined} style={{ paddingLeft: 12 + depth * 12 }}
        title={`${row.inLibrary} of ${row.entries} entries are in your library`} onClick={() => onOpen(row)}
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
  return <nav className="playlist-tree" aria-label="Playlists">
    {caption}<ul>{render(startOf(rows), 0)}</ul>
    {footer}
  </nav>;
}
