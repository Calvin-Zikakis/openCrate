import { useState } from "react";
import type { DuplicateCopies, LibraryAdmin, LibraryHealth, ResolvedDuplicate, TidiedFolder } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

type Untidy = LibraryHealth["untidy"][number];
type Duplicate = LibraryHealth["duplicates"][number];

/** A copy's facts: format, then what rekordbox has for it. */
function facts(copy: DuplicateCopies["copies"][number]): string {
  const parts = [copy.format];
  if (copy.inRekordbox) {
    parts.push("in rekordbox");
    if (copy.analysed) parts.push("analysed");
    if (copy.cues > 0) parts.push(plural(copy.cues, "cue", "cues"));
    if (copy.playlists > 0) parts.push(`in ${plural(copy.playlists, "playlist", "playlists")}`);
  } else {
    parts.push("not in rekordbox");
  }
  return parts.join(", ");
}

/** Settings: a look for folders named from untidy tags and recordings filed twice, and tidying
 * a folder when asked. */
export function LibraryCheck({ admin, onChanged }: { admin: LibraryAdmin; onChanged?: () => void }) {
  const [health, setHealth] = useState<LibraryHealth | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState<Untidy | null>(null);
  const [tidied, setTidied] = useState<TidiedFolder | null>(null);
  const [tidyError, setTidyError] = useState("");
  const [resolving, setResolving] = useState<{ duplicate: Duplicate; copies: DuplicateCopies | null; keep: number | null } | null>(null);
  const [resolved, setResolved] = useState<ResolvedDuplicate | null>(null);

  const openResolve = async (duplicate: Duplicate) => {
    setResolving({ duplicate, copies: null, keep: null });
    setResolved(null);
    setTidyError("");
    try {
      const copies = await admin.duplicateCopies(duplicate.fileIds);
      setResolving({ duplicate, copies, keep: copies.suggested });
    } catch (reason) {
      setTidyError(message(reason));
      setResolving(null);
    }
  };
  const resolve = async () => {
    if (!resolving?.copies || resolving.keep === null) return;
    const keep = resolving.keep;
    setChecking(true);
    setTidyError("");
    try {
      setResolved(await admin.resolveDuplicate(keep, resolving.copies.copies.map((copy) => copy.fileId).filter((id) => id !== keep)));
      setResolving(null);
      setHealth(await admin.checkLibrary());
      onChanged?.();
    } catch (reason) {
      setTidyError(message(reason));
    } finally {
      setChecking(false);
    }
  };

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
    <p className="settings-copy">Looks for folders named from untidy tags (such as "Artist;"), the same recording filed more than once, and files whose tags could not be read. Checking only reads what the last scan found. Tidy moves a folder's files to the tidy name, and tells rekordbox where any it uses went, so they keep their cues and playlists.</p>
    <button className="secondary-button" disabled={checking} onClick={() => void check()}>{checking ? "Checking..." : "Check library"}</button>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {health && <div className="library-check" role="region" aria-label="Library check results">
      {tidyError && <p role="alert" className="inline-error">{tidyError}</p>}
      {resolved && <p role="status" className="settings-copy">{`${plural(resolved.archived, "copy", "copies")} moved to ${resolved.archive}${resolved.merged > 0 ? `; ${resolved.merged} merged into the kept copy in rekordbox` : ""}.`}</p>}
      {tidied && <div role="status" className="settings-copy">
        <p>{`Moved ${plural(tidied.moved, "file", "files")} to ${tidied.suggested}${tidied.relocated > 0 ? `; rekordbox was told where ${tidied.relocated} went` : ""}.`}</p>
        {tidied.left.length > 0 && <ul className="skipped-list">{tidied.left.map((why) => <li key={why}>{why}</li>)}</ul>}
      </div>}
      <p className="settings-copy">{`Looked at ${plural(health.files, "file", "files")}.`}{health.untidy.length === 0 && health.duplicates.length === 0 && health.broken.length === 0 ? " Nothing to tidy." : ""}</p>
      {health.broken.length > 0 && <>
        <h4>{plural(health.broken.length, "file could not be read", "files could not be read")}</h4>
        <p className="settings-copy">These are in your folders but their tags could not be read, so they are not in the library. Each says why; a file that is not really audio, or is cut short, is usually a download that went wrong.</p>
        <ul className="check-list" aria-label="Files that could not be read">{health.broken.map((item) => <li key={`${item.rootLabel}/${item.path}`}>
          <code>{item.path}</code> <small>{`${item.rootLabel}: ${item.error}`}</small>
        </li>)}</ul>
      </>}
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
          <button className="link-button" disabled={checking} aria-label={`Resolve ${item.title}`} onClick={() => void openResolve(item)}>Resolve...</button>
          {resolving?.duplicate !== item && <ul>{item.files.map((file) => <li key={file}><code>{file}</code></li>)}</ul>}
          {resolving?.duplicate === item && <div className="warning-panel" role="region" aria-label={`Which ${item.title} to keep?`}>
            {!resolving.copies ? <p className="settings-copy" role="status">Reading rekordbox...</p> : <>
              <p className="settings-copy">The copy kept stays where it is. The others move to an archive folder beside your music folder, never deleted. Where rekordbox has them, their playlist places, My Tags and rating go to the kept copy first (after a backup), then their entries are removed. rekordbox must be closed, with writing on in Settings.</p>
              {resolving.copies.note && <p className="settings-copy">{resolving.copies.note}</p>}
              <fieldset className="copy-choice"><legend className="sr-only">Copy to keep</legend>
                {resolving.copies.copies.map((copy) => <label key={copy.fileId}>
                  <input type="radio" name={`keep-${item.title}`} checked={resolving.keep === copy.fileId} onChange={() => setResolving({ ...resolving, keep: copy.fileId })} />
                  <span><code>{copy.path}</code><small>{facts(copy)}</small></span>
                </label>)}
              </fieldset>
              <div className="warning-actions">
                <button className="import-button" disabled={checking || resolving.keep === null} onClick={() => void resolve()}>Keep this copy</button>
                <button className="secondary-button" disabled={checking} onClick={() => setResolving(null)}>Cancel</button>
              </div>
            </>}
          </div>}
        </li>)}</ul>
      </>}
    </div>}
  </>;
}
