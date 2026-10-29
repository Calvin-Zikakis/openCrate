import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { CrateInfo, FolderInfo, ImportTarget, LibraryAdmin, PlaylistInfo } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
type Section = "folder" | "crate" | "playlist";

interface Props {
  admin: LibraryAdmin;
  /** The music folders imports can go into, and the one chosen. */
  roots: FolderInfo[];
  rootId: number | null;
  onRoot: (rootId: number) => void;
  target: ImportTarget;
  onChange: (target: ImportTarget) => void;
  /** Writing to rekordbox is on and ready, so a rekordbox playlist can be chosen. */
  rekordboxWriting: boolean;
  /** Crates may have changed (one was made here), so the sidebar reads them again. */
  onCratesChanged?: () => void;
  /** Changes when crates or playlists may have changed elsewhere. */
  version?: number;
}

/** Where imports go: the music folder, and on into a crate and a rekordbox playlist. One button
 * that says the choice; it opens a two-level menu (the kind of thing on a website's navigation
 * bar): the kinds on the left, the choices of the kind under the pointer on the right. The crate
 * and playlist are kept in the library, so they hold across restarts. */
export function DestinationMenu({ admin, roots, rootId, onRoot, target, onChange, rekordboxWriting, onCratesChanged, version = 0 }: Props) {
  const [crates, setCrates] = useState<CrateInfo[]>([]);
  const [playlists, setPlaylists] = useState<PlaylistInfo[]>([]);
  const [open, setOpen] = useState(false);
  const [section, setSection] = useState<Section>("crate");
  const [naming, setNaming] = useState<string | null>(null);
  const [error, setError] = useState("");
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let live = true;
    admin.crates().then((found) => { if (live) setCrates(found); }, () => undefined);
    admin.playlists().then((found) => { if (live) setPlaylists(found.filter((playlist) => !playlist.isFolder)); }, () => undefined);
    return () => { live = false; };
  }, [admin, version, open]);

  // Clicking outside or Escape closes the menu.
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!menuRef.current?.contains(event.target as Node)) setOpen(false); };
    const escape = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", escape);
    return () => { window.removeEventListener("mousedown", close); window.removeEventListener("keydown", escape); };
  }, [open]);

  const change = async (next: ImportTarget) => {
    setError("");
    try {
      onChange(await admin.setImportTarget(next));
    } catch (reason) {
      setError(message(reason));
    }
  };
  const makeCrate = async () => {
    const name = naming?.trim() ?? "";
    if (name === "") return;
    setError("");
    try {
      const made = await admin.createCrate(name);
      setCrates((before) => [...before, made]);
      setNaming(null);
      onCratesChanged?.();
      await change({ ...target, crateId: made.id });
    } catch (reason) {
      setError(message(reason));
    }
  };

  const folder = roots.find((root) => root.id === rootId);
  const crate = crates.find((found) => found.id === target.crateId);
  const playlist = rekordboxWriting ? playlists.find((found) => found.id === target.playlistId) : undefined;
  const summary = [folder?.label ?? "no music folder", crate ? `crate ${crate.name}` : null, playlist ? `playlist ${playlist.name}` : null].filter(Boolean).join(" · ");

  const sections: { id: Section; label: string; detail: string }[] = [
    { id: "folder", label: "Music folder", detail: folder?.label ?? "None available" },
    { id: "crate", label: "Crate", detail: crate?.name ?? "None" },
    { id: "playlist", label: "rekordbox playlist", detail: !rekordboxWriting ? "Writing is off" : playlist?.name ?? "None" },
  ];
  const sectionKeys = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const ids = sections.map((item) => item.id);
    if (event.key === "ArrowDown") { event.preventDefault(); setSection(ids[(index + 1) % ids.length]!); }
    if (event.key === "ArrowUp") { event.preventDefault(); setSection(ids[(index + ids.length - 1) % ids.length]!); }
    if (event.key === "ArrowRight") { event.preventDefault(); menuRef.current?.querySelector<HTMLElement>(".destination-choices [role=menuitemradio], .destination-choices [role=menuitem]")?.focus(); }
  };
  const choice = (label: string, checked: boolean, onPick: () => void, disabled = false, key?: string | number) =>
    <button key={key ?? label} type="button" role="menuitemradio" aria-checked={checked} disabled={disabled} onClick={onPick}>
      <span className="column-check" aria-hidden="true">{checked ? "✓" : ""}</span>{label}
    </button>;

  return <div className="destination" ref={menuRef}>
    <button type="button" className="secondary-button destination-button" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
      <span className="destination-label">Import destination</span>
      <span className="destination-summary">{summary}</span>
      <span aria-hidden="true" className="destination-caret">{"▾"}</span>
    </button>
    {open && <div className="destination-menu" role="menu" aria-label="Import destination">
      <div className="destination-kinds" role="group" aria-label="Kind">
        {sections.map((item, index) => <button key={item.id} type="button" role="menuitem" className={section === item.id ? "is-active" : ""} aria-current={section === item.id ? "true" : undefined}
          onMouseEnter={() => setSection(item.id)} onFocus={() => setSection(item.id)} onClick={() => setSection(item.id)} onKeyDown={(event) => sectionKeys(event, index)}>
          <strong>{item.label}</strong><small>{item.detail}</small><span aria-hidden="true" className="destination-chevron">{"›"}</span>
        </button>)}
      </div>
      <div className="destination-choices" role="group" aria-label={sections.find((item) => item.id === section)?.label}>
        {section === "folder" && (roots.length === 0
          ? <p className="settings-copy">No music folder can be reached right now.</p>
          : roots.map((root) => choice(root.label, root.id === rootId, () => onRoot(root.id), false, root.id)))}
        {section === "crate" && <>
          <p className="destination-hint">Every import also lands in this crate. Crates live in openCrate and never change rekordbox.</p>
          {choice("None", target.crateId === null, () => void change({ ...target, crateId: null }))}
          {crates.map((item) => choice(item.name, item.id === target.crateId, () => void change({ ...target, crateId: item.id }), false, item.id))}
          {naming === null
            ? <button type="button" role="menuitem" onClick={() => setNaming("")}><span className="column-check" aria-hidden="true" />New crate...</button>
            : <form className="target-new-crate" onSubmit={(event) => { event.preventDefault(); void makeCrate(); }}>
              <input aria-label="Name for the new crate" autoFocus value={naming} placeholder="Name the crate" onChange={(event) => setNaming(event.target.value)} onKeyDown={(event) => event.stopPropagation()} />
              <button type="submit" className="secondary-button" disabled={naming.trim() === ""}>Make it</button>
              <button type="button" className="link-button" onClick={() => setNaming(null)}>Cancel</button>
            </form>}
        </>}
        {section === "playlist" && (!rekordboxWriting
          ? <p className="destination-hint">Needs writing to rekordbox, which is off or not yet confirmed in Settings. With it on, every import also joins rekordbox's rolling Imported playlist.</p>
          : <>
            <p className="destination-hint">Every import also joins this rekordbox playlist, as well as rekordbox's rolling Imported playlist.</p>
            {choice("None", target.playlistId === null, () => void change({ ...target, playlistId: null }))}
            {playlists.map((item) => choice(item.name, item.id === target.playlistId, () => void change({ ...target, playlistId: item.id }), false, item.id))}
          </>)}
        {error && <p role="alert" className="inline-error">{error}</p>}
      </div>
    </div>}
  </div>;
}
