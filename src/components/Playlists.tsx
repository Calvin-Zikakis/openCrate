import { useEffect, useMemo, useRef, useState } from "react";
import type { LibraryAdmin, PlaylistInfo } from "../library/types";

interface Props {
  admin: LibraryAdmin;
  /** Changes when the library may have changed, so the list is read again. */
  version: number;
  activeId: number | null;
  onOpen: (playlist: PlaylistInfo) => void;
}

/** rekordbox's own tree starts at an invisible root folder; show what is inside it. */
function startOf(rows: PlaylistInfo[]): number | null {
  const top = rows.filter((row) => row.parentId === null);
  const [only] = top;
  return top.length === 1 && only?.isFolder && only.name === "ROOT" ? only.id : null;
}

export function Playlists({ admin, version, activeId, onOpen }: Props) {
  const [rows, setRows] = useState<PlaylistInfo[]>([]);
  const [open, setOpen] = useState<Set<number>>(new Set());
  const [failed, setFailed] = useState(false);
  const opened = useRef(false);

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
      <span>{failed ? "Try reading your export again." : "Read a rekordbox export on the rekordbox screen to see its playlists here."}</span></div>;
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
      <button className="tree-playlist" aria-current={activeId === row.id ? "true" : undefined} style={{ paddingLeft: 12 + depth * 12 }}
        title={`${row.inLibrary} of ${row.entries} entries are in your library`} onClick={() => onOpen(row)}>
        <span className="truncate">{row.name}</span><small>{row.inLibrary}/{row.entries}</small>
      </button>
    </li>);
  return <nav className="playlist-tree" aria-label="Playlists"><div className="nav-caption">PLAYLISTS</div><ul>{render(startOf(rows), 0)}</ul></nav>;
}
