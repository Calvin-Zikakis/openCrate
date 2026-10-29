import { useState } from "react";
import type { CrateInfo, LibraryAdmin } from "../library/types";
import { Icon } from "./Icon";
import { PLAIN_TEXT } from "./plainText";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

interface Props {
  admin: LibraryAdmin;
  crates: CrateInfo[];
  onOpen: (crate: CrateInfo) => void;
  /** A crate was made, so the lists read them again. */
  onChanged: () => void;
}

/** Every crate as a card, the way the playlist browser shows folders and playlists. */
export function CrateBrowser({ admin, crates, onOpen, onChanged }: Props) {
  const [naming, setNaming] = useState<string | null>(null);
  const [error, setError] = useState("");
  const make = async () => {
    const name = naming?.trim() ?? "";
    setNaming(null);
    if (!name) return;
    setError("");
    try {
      await admin.createCrate(name);
      onChanged();
    } catch (reason) {
      setError(message(reason));
    }
  };
  return <div className="playlist-browser" role="region" aria-label="Crates">
    <div className="library-hero folded">
      <div><h1>Crates</h1></div>
      <div className="browser-actions"><button className="secondary-button" onClick={() => setNaming("")}><Icon name="crate" size={14} />New crate</button></div>
    </div>
    <p className="settings-copy">Crates are lists of your own, kept in openCrate; rekordbox is not changed. Drag tracks onto one in the sidebar, or drag a crate onto a rekordbox folder to make a playlist from it.</p>
    {naming !== null && <input {...PLAIN_TEXT}
      className="crate-name-input browser-name"
      aria-label="New crate"
      placeholder="Name the new crate, then press Return"
      autoFocus
      maxLength={100}
      value={naming}
      onChange={(event) => setNaming(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === "Enter") { event.preventDefault(); void make(); }
        if (event.key === "Escape") { event.preventDefault(); setNaming(null); }
      }}
      onBlur={() => { if (naming.trim()) void make(); else setNaming(null); }}
    />}
    {error && <p role="alert" className="inline-error">{error}</p>}
    {crates.length === 0 && naming === null && <p className="settings-copy">No crates yet. Make one, then drag tracks onto it.</p>}
    {crates.length > 0 && <ul className="browser-grid" aria-label="Crates">{crates.map((crate) => <li key={crate.id}>
      <button className="browser-card" onClick={() => onOpen(crate)}>
        <span className="browser-icon" aria-hidden="true"><Icon name="crate" size={30} /></span>
        <strong className="truncate">{crate.name}</strong>
        <small>{crate.tracks === 0 ? "Empty" : `${crate.tracks} ${crate.tracks === 1 ? "track" : "tracks"}`}</small>
      </button>
    </li>)}</ul>}
  </div>;
}
