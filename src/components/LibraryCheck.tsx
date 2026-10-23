import { useState } from "react";
import type { LibraryAdmin, LibraryHealth } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

/** Settings: a read-only look for folders named from untidy tags and recordings filed twice. */
export function LibraryCheck({ admin }: { admin: LibraryAdmin }) {
  const [health, setHealth] = useState<LibraryHealth | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState("");

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
    <p className="settings-copy">Looks for folders named from untidy tags (such as "Artist;") and the same recording filed more than once. It only reads what the last scan found; nothing is moved or deleted.</p>
    <button className="secondary-button" disabled={checking} onClick={() => void check()}>{checking ? "Checking..." : "Check library"}</button>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {health && <div className="library-check" role="region" aria-label="Library check results">
      <p className="settings-copy">{`Looked at ${plural(health.files, "file", "files")}.`}{health.untidy.length === 0 && health.duplicates.length === 0 ? " Nothing to tidy." : ""}</p>
      {health.untidy.length > 0 && <>
        <h4>{plural(health.untidy.length, "folder name looks untidy", "folder names look untidy")}</h4>
        <ul className="check-list">{health.untidy.map((item) => <li key={`${item.rootLabel}/${item.folder}`}>
          <code>{item.folder}</code>{" would be "}<code>{item.suggested}</code> <small>{`${item.rootLabel}, ${plural(item.files, "file", "files")}`}</small>
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
