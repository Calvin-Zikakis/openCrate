import { useCallback, useEffect, useState } from "react";
import type { LibraryAdmin, RekordboxCrate, RekordboxCrates } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

function summary(crate: RekordboxCrate): string {
  const parts = [];
  if (crate.known > 0) parts.push(`${plural(crate.known, "track", "tracks")} rekordbox has`);
  if (crate.new > 0) parts.push(`${crate.new} new to rekordbox`);
  return parts.length > 0 ? parts.join(", ") : "No tracks that can go to rekordbox";
}

/** The rekordbox screen: crates the owner puts in openCrate's rekordbox file as playlists. */
export function CratesForRekordbox({ admin, refresh }: { admin: LibraryAdmin; refresh: number }) {
  const [listed, setListed] = useState<RekordboxCrates | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      setListed(await admin.rekordboxCrates());
      setError("");
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);
  useEffect(() => { void load(); }, [load, refresh]);

  if (!listed) return error ? <p role="alert" className="inline-error">{error}</p> : null;
  if (listed.crates.length === 0) return <section className="sync-card" role="region" aria-label="Crates for rekordbox">
    <h2>Crates for rekordbox</h2>
    <p className="settings-copy">Crates you make in openCrate can go to rekordbox as playlists from here. Make a crate with the + beside CRATES in the sidebar first.</p>
  </section>;

  const act = async (work: () => Promise<RekordboxCrates>, done: string) => {
    setBusy(true);
    setError("");
    setNote("");
    try {
      setListed(await work());
      setNote(done);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const anyIn = listed.crates.some((crate) => crate.inFile);

  return <section className="sync-card" role="region" aria-label="Crates for rekordbox">
    <h2>Crates for rekordbox</h2>
    <p className="settings-copy">{`A crate switched on goes into openCrate's rekordbox file as a playlist in the folder "${listed.folder}". Tracks rekordbox already has are written exactly as rekordbox exported them, so their cues, grids and tags stay as they are.`}</p>
    <ul className="folder-list">{listed.crates.map((crate) => <li key={crate.crateId} className="folder-row">
      <label className="send-row">
        <input type="checkbox" checked={crate.inFile} disabled={busy} aria-label={`${crate.name} in openCrate's rekordbox file`}
          onChange={(event) => void act(() => admin.setRekordboxCrate(crate.crateId, event.target.checked), event.target.checked ? `"${crate.name}" is in the file.` : `"${crate.name}" was taken out of the file.`)} />
        <span className="folder-text">
          <strong>{crate.name}</strong>
          <small>{summary(crate)}</small>
          {crate.needsExport > 0 && <small className="crate-warning">{`${plural(crate.needsExport, "track is", "tracks are")} left out: read an XML export of rekordbox first, so they can be written exactly as rekordbox has them.`}</small>}
          {crate.noFile > 0 && <small className="crate-warning">{`${plural(crate.noFile, "track is", "tracks are")} left out: the file cannot be reached.`}</small>}
        </span>
      </label>
    </li>)}</ul>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {anyIn && <button className="secondary-button" disabled={busy} onClick={() => void act(() => admin.updateRekordboxFile(), "The file now shows your crates as they are.")}>Update file</button>}
    {note && <div role="status" className="send-steps">
      <p className="settings-copy">{note} <code>{listed.xmlPath}</code></p>
      <p className="settings-copy">{`In rekordbox, open rekordbox xml in the tree (refresh it if it is open), then "${listed.folder}". Drag a playlist into your own playlists to bring it in. These steps are still to be checked on a real rekordbox.`}</p>
    </div>}
  </section>;
}
