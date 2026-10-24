import { useState } from "react";
import type { LibraryAdmin, LibraryHealth, TidiedFolder } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

type Untidy = LibraryHealth["untidy"][number];

/** Settings: a look for folders named from untidy tags and recordings filed twice, and tidying
 * a folder when asked. */
export function LibraryCheck({ admin, onChanged }: { admin: LibraryAdmin; onChanged?: () => void }) {
  const [health, setHealth] = useState<LibraryHealth | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState<Untidy | null>(null);
  const [tidied, setTidied] = useState<TidiedFolder | null>(null);
  const [tidyError, setTidyError] = useState("");

  const tidy = async (folder: Untidy) => {
    setChecking(true);
    setTidyError("");
    try {
      setTidied(await admin.tidyFolder(folder.rootId, folder.folder));
      setConfirming(null);
      setHealth(await admin.checkLibrary());
      onChanged?.();
    } catch (reason) {
      setTidyError(message(reason));
    } finally {
      setChecking(false);
    }
  };

  const check = async () => {
    setChecking(true);
    setError("");
    try {
      setHealth(await admin.checkLibrary());
    } catch (reason) {
      setError(message(reason));
    } finally {
      setChecking(false);
    }
  };

  return <>
    <h3>Library check</h3>
    <p className="settings-copy">Looks for folders named from untidy tags (such as "Artist;") and the same recording filed more than once. Checking only reads what the last scan found. Tidy moves a folder's files to the tidy name, and tells rekordbox where any it uses went, so they keep their cues and playlists.</p>
    <button className="secondary-button" disabled={checking} onClick={() => void check()}>{checking ? "Checking..." : "Check library"}</button>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {health && <div className="library-check" role="region" aria-label="Library check results">
      {tidyError && <p role="alert" className="inline-error">{tidyError}</p>}
      {tidied && <div role="status" className="settings-copy">
        <p>{`Moved ${plural(tidied.moved, "file", "files")} to ${tidied.suggested}${tidied.relocated > 0 ? `; rekordbox was told where ${tidied.relocated} went` : ""}.`}</p>
        {tidied.left.length > 0 && <ul className="skipped-list">{tidied.left.map((why) => <li key={why}>{why}</li>)}</ul>}
      </div>}
      <p className="settings-copy">{`Looked at ${plural(health.files, "file", "files")}.`}{health.untidy.length === 0 && health.duplicates.length === 0 ? " Nothing to tidy." : ""}</p>
      {health.untidy.length > 0 && <>
        <h4>{plural(health.untidy.length, "folder name looks untidy", "folder names look untidy")}</h4>
        <ul className="check-list">{health.untidy.map((item) => <li key={`${item.rootLabel}/${item.folder}`}>
          <code>{item.folder}</code>{" would be "}<code>{item.suggested}</code> <small>{`${item.rootLabel}, ${plural(item.files, "file", "files")}`}</small>
          <button className="link-button" disabled={checking} aria-label={`Tidy ${item.folder}`} onClick={() => { setConfirming(item); setTidyError(""); setTidied(null); }}>Tidy...</button>
          {confirming === item && <div className="warning-panel" role="region" aria-label={`Tidy ${item.folder}?`}>
            <p className="settings-copy">{`${plural(item.files, "file moves", "files move")} to ${item.suggested}, beside the folder it is in now. A file whose new place is already taken stays where it is. If rekordbox uses any of them, openCrate backs up rekordbox's library and tells it where they went; this needs writing to rekordbox on, and rekordbox closed.`}</p>
            <div className="warning-actions">
              <button className="import-button" disabled={checking} onClick={() => void tidy(item)}>Tidy</button>
              <button className="secondary-button" disabled={checking} onClick={() => setConfirming(null)}>Cancel</button>
            </div>
          </div>}
        </li>)}</ul>
      </>}
      {health.duplicates.length > 0 && <>
        <h4>{plural(health.duplicates.length, "recording is filed more than once", "recordings are filed more than once")}</h4>
        <ul className="check-list">{health.duplicates.map((item) => <li key={item.files.join("|")}>
          <strong>{item.title}</strong> <small>{item.artist}</small>
          <ul>{item.files.map((file) => <li key={file}><code>{file}</code></li>)}</ul>
        </li>)}</ul>
      </>}
    </div>}
  </>;
}
