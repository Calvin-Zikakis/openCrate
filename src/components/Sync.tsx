import { useCallback, useEffect, useState } from "react";
import { Icon } from "./Icon";
import { CratesForRekordbox } from "./CratesForRekordbox";
import { SendToRekordbox } from "./SendToRekordbox";
import type { AliasSuggestion, ImportResult, LibraryAdmin, MirrorInfo, RekordboxDatabase } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const number = (value: number) => value.toLocaleString("en-US");
const fileName = (path: string) => path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;

function resultText(result: ImportResult): string {
  if (result.unchanged) return "Nothing has changed since the last read.";
  return `Read ${number(result.entries)} entries. ${number(result.matched)} match your files${result.derivedSet > 0 ? `, so ${number(result.derivedSet)} tracks now show rekordbox's tempo and key` : ""}.`;
}

interface Props {
  admin: LibraryAdmin;
  /** Called when a read has changed what the library shows. */
  onChanged: () => void;
}

export function Sync({ admin, onChanged }: Props) {
  const [mirror, setMirror] = useState<MirrorInfo | null>(null);
  const [database, setDatabase] = useState<RekordboxDatabase | null>(null);
  const [reads, setReads] = useState(0);
  const [suggestions, setSuggestions] = useState<AliasSuggestion[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      // The direct read is optional; the screen works without it.
      setDatabase(await admin.rekordboxDatabase().catch(() => null));
      const [first] = await admin.mirrors();
      setMirror(first ?? null);
      setSuggestions(first && first.counts.unmapped > 0 ? await admin.aliasSuggestions(first.sourceId) : []);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setLoaded(true);
    }
  }, [admin]);

  useEffect(() => { void load(); }, [load]);

  const run = async (work: () => Promise<ImportResult | null>) => {
    setBusy(true);
    setNote("");
    setError("");
    try {
      const result = await work();
      if (result) {
        setNote(resultText(result));
        setReads((count) => count + 1);
        onChanged();
      }
      await load();
    } catch (reason) {
      setError(message(reason));
      await load();
    } finally {
      setBusy(false);
    }
  };
  const choose = () => void run(() => admin.importExport());
  const readLibrary = () => void run(() => admin.readRekordboxDatabase());
  const direct = database?.enabled ? database : null;
  const blocked = direct?.running ? "rekordbox is running. Quit it to read its library."
    : direct?.needsConfirmation ? `rekordbox is now ${direct.appVersion ?? "a new version"}. Confirm the warning in Settings before reading it.` : "";
  const readButton = direct && <button className="secondary-button" disabled={busy || blocked !== ""} onClick={readLibrary}>Read from rekordbox</button>;
  const fromLibrary = mirror?.kind === "master_db";
  const again = () => mirror && void run(() => admin.rereadMirror(mirror.sourceId));
  const accept = (suggestion: AliasSuggestion) => mirror && void run(async () => {
    await admin.addAlias(suggestion.rootId, suggestion.prefix);
    return mirror.kind === "master_db" ? admin.readRekordboxDatabase() : admin.rereadMirror(mirror.sourceId);
  });

  return <div className="sync-screen">
    <div className="feature-empty sync-hero">
      <div className="empty-icon"><Icon name="sync" size={36} /></div>
      <h1>rekordbox</h1>
      <p>{direct
        ? "Read rekordbox's library, or an XML export, to bring in tempo, key, cues, My Tags and playlists. openCrate reads a backup copy; rekordbox is never changed."
        : "Read a rekordbox XML export to bring in tempo, key, cues, beat grids and playlists. openCrate only reads it; rekordbox is never changed."}</p>
    </div>
    {blocked && <p role="status" className="inline-error">{blocked}</p>}
    {error && <p role="alert" className="inline-error">{error}</p>}
    {loaded && !mirror && <div className="sync-card">
      <h2>No export yet</h2>
      <p className="settings-copy">In rekordbox, choose File, then Export Collection in xml format. Then pick that file here. Add your music folder in Settings first so entries can be matched.</p>
      {direct && <p className="settings-copy">Or read rekordbox's own library directly. openCrate backs it up first and reads the copy.</p>}
      <div className="sync-actions">
        <button className="secondary-button" disabled={busy} onClick={choose}>Choose export...</button>
        {readButton}
      </div>
    </div>}
    {mirror && <div className="sync-card">
      <div className="sync-source">
        <div>
          <h2>{fromLibrary ? "rekordbox library" : fileName(mirror.path)}</h2>
          <small title={mirror.path}>{mirror.path}</small>
          <small>{mirror.lastReadMs ? `Read ${new Date(mirror.lastReadMs).toLocaleString()}` : "Not read yet"}{mirror.generation > 0 ? ` - version ${mirror.generation}` : ""}</small>
        </div>
        <div className="sync-actions">
          {fromLibrary ? readButton : <button className="secondary-button" disabled={busy} onClick={again}>Read again</button>}
          {!fromLibrary && readButton}
          <button className="secondary-button" disabled={busy} onClick={choose}>{fromLibrary ? "Choose export..." : "Choose another..."}</button>
        </div>
      </div>
      {mirror.lastError && <p role="alert" className="inline-error">The last read failed: {mirror.lastError} Your earlier data is kept.</p>}
      <dl className="sync-stats">
        <div><dt>Match your files</dt><dd>{number(mirror.counts.matched)}</dd></div>
        <div title="Entries pointing outside every folder you added, such as another drive or the recycle bin."><dt>Not in your folders</dt><dd>{number(mirror.counts.unmapped)}</dd></div>
        <div title="Entries inside a folder you added, but with no file indexed at that path. Scan the folder again, or the file was deleted."><dt>Not found in folder</dt><dd>{number(mirror.counts.notIndexed)}</dd></div>
        <div><dt>Playlists</dt><dd>{number(mirror.counts.playlists)}</dd></div>
      </dl>
      {mirror.counts.ambiguous + mirror.counts.unrecognized > 0 && <p className="settings-copy">{number(mirror.counts.ambiguous + mirror.counts.unrecognized)} entries could not be placed safely and were left unmatched.</p>}
      {note && <p role="status" className="settings-copy">{note}</p>}
    </div>}
    {loaded && <SendToRekordbox admin={admin} refresh={reads} />}
    {loaded && <CratesForRekordbox admin={admin} refresh={reads} />}
    {suggestions.length > 0 && <div className="sync-card">
      <h2>These entries look like they belong to a folder</h2>
      <p className="settings-copy">The export was probably made when your music lived somewhere else. Each suggestion is based on files that really exist in your folder.</p>
      <ul className="folder-list">{suggestions.map((suggestion) => <li key={`${suggestion.rootId}:${suggestion.prefix}`} className="folder-row">
        <div className="folder-text">
          <strong>{number(suggestion.entries)} entries start with {suggestion.prefix}</strong>
          <small>They match files in {suggestion.rootLabel}</small>
        </div>
        <button className="secondary-button" disabled={busy} aria-label={`Use ${suggestion.prefix} for ${suggestion.rootLabel}`} onClick={() => accept(suggestion)}>Use this</button>
      </li>)}</ul>
    </div>}
  </div>;
}
