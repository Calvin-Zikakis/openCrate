import { useCallback, useEffect, useRef, useState } from "react";
import { carriesTracks, draggedTracks, setDraggedCrate } from "../library/drag";
import type { CrateInfo, LibraryAdmin } from "../library/types";

/** How long "Added 1 to ..." stays up. */
const NOTE_MS = 3000;

interface Props {
  admin: LibraryAdmin;
  /** Changes when crates may have changed, so the list is read again. */
  version: number;
  activeId: number | null;
  onOpen: (crate: CrateInfo) => void;
  /** Tracks were added to a crate. */
  onChanged: () => void;
  onRenamed: (crate: CrateInfo) => void;
  onDeleted: (crateId: number) => void;
}

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

/** The owner's own crates, kept by openCrate only. Drop tracks on a crate to add them. */
export function Crates({ admin, version, activeId, onOpen, onChanged, onRenamed, onDeleted }: Props) {
  const [crates, setCrates] = useState<CrateInfo[]>([]);
  const [naming, setNaming] = useState<{ id: number | null; name: string } | null>(null);
  const [menu, setMenu] = useState<{ crate: CrateInfo; x: number; y: number } | null>(null);
  const [confirming, setConfirming] = useState<CrateInfo | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const menuRef = useRef<HTMLDivElement>(null);

  // A note confirms a drop; it goes away on its own.
  useEffect(() => {
    if (!note) return;
    const timer = setTimeout(() => setNote(""), NOTE_MS);
    return () => clearTimeout(timer);
  }, [note]);

  const load = useCallback(async () => {
    try {
      setCrates(await admin.crates());
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);
  useEffect(() => { void load(); }, [load, version]);

  useEffect(() => {
    if (!menu) return;
    menuRef.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    const close = (event: MouseEvent) => { if (!menuRef.current?.contains(event.target as Node)) setMenu(null); };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setMenu(null); };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", escape);
    return () => { window.removeEventListener("mousedown", close); window.removeEventListener("keydown", escape); };
  }, [menu]);

  const submitName = async () => {
    if (!naming) return;
    const { id, name } = naming;
    try {
      if (id === null) {
        const made = await admin.createCrate(name);
        setNaming(null);
        await load();
        onOpen(made);
      } else {
        await admin.renameCrate(id, name);
        setNaming(null);
        await load();
        const renamed = crates.find((crate) => crate.id === id);
        if (renamed) onRenamed({ ...renamed, name: name.trim() });
      }
      setError("");
    } catch (reason) {
      setError(message(reason));
    }
  };
  const remove = async (crate: CrateInfo) => {
    setConfirming(null);
    try {
      await admin.deleteCrate(crate.id);
      await load();
      onDeleted(crate.id);
    } catch (reason) {
      setError(message(reason));
    }
  };
  const drop = async (crate: CrateInfo, trackIds: number[]) => {
    try {
      const added = await admin.addToCrate(crate.id, trackIds);
      setNote(added === 0 ? `Already in ${crate.name}.` : `Added ${added} to ${crate.name}.`);
      await load();
      if (added > 0) onChanged();
    } catch (reason) {
      setError(message(reason));
    }
  };

  const nameInput = (label: string) => <input
    className="crate-name-input"
    aria-label={label}
    autoFocus
    maxLength={100}
    value={naming?.name ?? ""}
    onChange={(event) => setNaming((current) => current && { ...current, name: event.target.value })}
    onKeyDown={(event) => {
      if (event.key === "Enter") { event.preventDefault(); void submitName(); }
      if (event.key === "Escape") { event.preventDefault(); setNaming(null); setError(""); }
    }}
    onBlur={() => { if (naming?.name.trim()) void submitName(); else setNaming(null); }}
  />;

  return <nav className="crate-list" aria-label="Crates">
    <div className="crate-head">
      <div className="nav-caption">CRATES</div>
      <button className="icon-button crate-add" aria-label="New crate" title="New crate" onClick={() => { setNaming({ id: null, name: "" }); setError(""); }}>+</button>
    </div>
    {crates.length === 0 && !naming && <p className="crate-empty">Make a crate, then drag tracks onto it. Crates stay in openCrate; rekordbox is not changed.</p>}
    <ul>
      {crates.map((crate) => <li key={crate.id}>
        {naming?.id === crate.id ? nameInput(`Rename ${crate.name}`) : <button
          className={`tree-playlist crate-item${over === crate.id ? " is-drop-target" : ""}`}
          aria-current={activeId === crate.id ? "true" : undefined}
          onClick={() => onOpen(crate)}
          draggable
          onDragStart={(event) => setDraggedCrate(event.dataTransfer, crate)}
          title="Drag onto a rekordbox folder to make a playlist from it"
          onContextMenu={(event) => { event.preventDefault(); setMenu({ crate, x: event.clientX, y: event.clientY }); }}
          onKeyDown={(event) => {
            if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
              event.preventDefault();
              const box = event.currentTarget.getBoundingClientRect();
              setMenu({ crate, x: box.left + 12, y: box.bottom });
            }
          }}
          onDragOver={(event) => {
            if (!carriesTracks(event.dataTransfer)) return;
            event.preventDefault();
            event.dataTransfer.dropEffect = "copy";
            setOver(crate.id);
          }}
          onDragLeave={() => setOver((current) => (current === crate.id ? null : current))}
          onDrop={(event) => {
            setOver(null);
            const trackIds = draggedTracks(event.dataTransfer);
            if (!trackIds) return;
            event.preventDefault();
            void drop(crate, trackIds);
          }}
        >
          <span className="truncate">{crate.name}</span><small>{crate.tracks}</small>
        </button>}
        {confirming?.id === crate.id && <div className="crate-confirm" role="alertdialog" aria-label={`Delete ${crate.name}?`}>
          <span>Delete {crate.name}? The tracks stay in your library.</span>
          <button className="link-button" onClick={() => void remove(crate)}>Delete</button>
          <button className="link-button" onClick={() => setConfirming(null)}>Keep it</button>
        </div>}
      </li>)}
      {naming?.id === null && <li>{nameInput("New crate name")}</li>}
    </ul>
    {note && <p className="crate-note" role="status">{note}</p>}
    {error && <p className="crate-error" role="alert">{error}</p>}
    {menu && <div ref={menuRef} className="column-menu" role="menu" aria-label={`${menu.crate.name} actions`} style={{ left: menu.x, top: menu.y }}>
      <button type="button" role="menuitem" onClick={() => { setNaming({ id: menu.crate.id, name: menu.crate.name }); setMenu(null); }}>Rename</button>
      <button type="button" role="menuitem" onClick={() => { setConfirming(menu.crate); setMenu(null); }}>Delete</button>
    </div>}
  </nav>;
}
