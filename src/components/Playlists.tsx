import { useEffect, useMemo, useRef, useState } from "react";
import { carriesTracks, draggedTracks } from "../library/drag";
import type { LibraryAdmin, PlaylistInfo } from "../library/types";

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

  useEffect(() => {
    if (!note?.done || note.failed) return;
    const timer = setTimeout(() => setNote(null), NOTE_MS);
    return () => clearTimeout(timer);
  }, [note]);

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

  useEffect(() => {
    let stale = false;
    admin.playlists().then((next) => {
      if (stale) return;
      setRows(next);
      setFailed(false);
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

  if (rows.length === 0) {
    return <div className="playlist-note"><div className="nav-caption">PLAYLISTS</div>
      <p>{failed ? "Playlists could not be read." : "None yet."}</p>
      <span>{failed ? "Try reading your export again." : "Read rekordbox on the rekordbox screen to see its playlists here."}</span></div>;
  }
  const toggle = (id: number) => setOpen((current) => {
    const next = new Set(current);
    if (!next.delete(id)) next.add(id);
    return next;
  });
  const render = (parent: number | null, depth: number) => (children.get(parent) ?? []).map((row) => row.isFolder
    ? <li key={row.id}>
      <button className="tree-folder" aria-expanded={open.has(row.id)} style={{ paddingLeft: 12 + depth * 12 }} onClick={() => toggle(row.id)}>
        <span aria-hidden="true">{open.has(row.id) ? "v" : ">"}</span><span className="truncate">{row.name}</span>
      </button>
      {open.has(row.id) && <ul>{render(row.id, depth + 1)}</ul>}
    </li>
    : <li key={row.id}>
      <button className={`tree-playlist${over === row.id ? " is-drop-target" : ""}`} aria-current={activeId === row.id ? "true" : undefined} style={{ paddingLeft: 12 + depth * 12 }}
        title={`${row.inLibrary} of ${row.entries} entries are in your library`} onClick={() => onOpen(row)}
        onDragOver={(event) => {
          if (!carriesTracks(event.dataTransfer)) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = "copy";
          setOver(row.id);
        }}
        onDragLeave={() => setOver((current) => (current === row.id ? null : current))}
        onDrop={(event) => {
          setOver(null);
          const ids = draggedTracks(event.dataTransfer);
          if (!ids) return;
          event.preventDefault();
          void drop(row, ids);
        }}>
        <span className="truncate">{row.name}</span><small>{row.inLibrary}/{row.entries}</small>
      </button>
    </li>);
  return <nav className="playlist-tree" aria-label="Playlists"><div className="nav-caption">PLAYLISTS</div><ul>{render(startOf(rows), 0)}</ul>
    {note && <p className={note.failed ? "crate-error" : "crate-note"} role={note.failed ? "alert" : "status"}>{note.text}</p>}
  </nav>;
}
