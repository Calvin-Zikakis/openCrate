import { useEffect, useState } from "react";
import type { CrateInfo, ImportTarget, LibraryAdmin, PlaylistInfo } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const NEW_CRATE = "new";

interface Props {
  admin: LibraryAdmin;
  target: ImportTarget;
  onChange: (target: ImportTarget) => void;
  /** Writing to rekordbox is on and ready, so a rekordbox playlist can be chosen. */
  rekordboxWriting: boolean;
  /** Crates may have changed (one was made here), so the sidebar reads them again. */
  onCratesChanged?: () => void;
  /** Changes when crates or playlists may have changed elsewhere. */
  version?: number;
}

/** Where imported tracks go once they are in the library: a crate of the owner's, a rekordbox
 * playlist, both or neither. Kept in the library, so it holds across restarts. */
export function ImportTargetCard({ admin, target, onChange, rekordboxWriting, onCratesChanged, version = 0 }: Props) {
  const [crates, setCrates] = useState<CrateInfo[]>([]);
  const [playlists, setPlaylists] = useState<PlaylistInfo[]>([]);
  const [naming, setNaming] = useState<string | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let live = true;
    admin.crates().then((found) => { if (live) setCrates(found); }, () => undefined);
    admin.playlists().then((found) => { if (live) setPlaylists(found.filter((playlist) => !playlist.isFolder)); }, () => undefined);
    return () => { live = false; };
  }, [admin, version]);

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
  const crateName = crates.find((crate) => crate.id === target.crateId)?.name;
  const playlistName = playlists.find((playlist) => playlist.id === target.playlistId)?.name;

  return <div className="import-target" role="group" aria-label="After importing">
    <h2>After importing</h2>
    <label className="setting-row">
      <span><strong>Add the tracks to a crate</strong><small>Every import lands in it, on top of the library. Crates live in openCrate and never change rekordbox.</small></span>
      <select aria-label="Crate for imported tracks" value={naming !== null ? NEW_CRATE : target.crateId ?? ""} onChange={(event) => {
        if (event.target.value === NEW_CRATE) { setNaming(""); return; }
        setNaming(null);
        void change({ ...target, crateId: event.target.value === "" ? null : Number(event.target.value) });
      }}>
        <option value="">None</option>
        {crates.map((crate) => <option key={crate.id} value={crate.id}>{crate.name}</option>)}
        <option value={NEW_CRATE}>New crate...</option>
      </select>
    </label>
    {naming !== null && <form className="target-new-crate" onSubmit={(event) => { event.preventDefault(); void makeCrate(); }}>
      <input aria-label="Name for the new crate" autoFocus value={naming} placeholder="Name the crate" onChange={(event) => setNaming(event.target.value)} />
      <button type="submit" className="secondary-button" disabled={naming.trim() === ""}>Make it</button>
      <button type="button" className="link-button" onClick={() => setNaming(null)}>Cancel</button>
    </form>}
    <label className="setting-row">
      <span><strong>Add them to a rekordbox playlist</strong><small>{rekordboxWriting
        ? "They always join rekordbox's rolling Imported playlist too."
        : "Needs writing to rekordbox, which is off or not yet confirmed in Settings. With it on, every import also joins rekordbox's Imported playlist."}</small></span>
      <select aria-label="rekordbox playlist for imported tracks" disabled={!rekordboxWriting} value={target.playlistId ?? ""} onChange={(event) => void change({ ...target, playlistId: event.target.value === "" ? null : Number(event.target.value) })}>
        <option value="">None</option>
        {playlists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.name}</option>)}
      </select>
    </label>
    <p className="settings-copy" role="status">{target.crateId === null && target.playlistId === null
      ? "Imported tracks only go into your library for now."
      : `Imported tracks go into ${[crateName && `the crate ${crateName}`, playlistName && rekordboxWriting && `the rekordbox playlist ${playlistName}`].filter(Boolean).join(" and ") || "your library"}.`}</p>
    {error && <p role="alert" className="inline-error">{error}</p>}
  </div>;
}
