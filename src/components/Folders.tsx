import { useCallback, useEffect, useRef, useState } from "react";
import type { FolderInfo, LibraryAdmin, ScanStatus, ScanSummary } from "../library/types";

const POLL_MS = 400;
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
const PHASES: Record<string, string> = {
  checking: "Checking the folder",
  walking: "Looking through the folder",
  applying: "Updating the index",
  "reading-tags": "Reading tags",
  finished: "Finishing up",
};

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

function availability(folder: FolderInfo) {
  if (folder.availability === "available") return "Available";
  if (folder.availability === "unavailable") return "Offline";
  return "Check this folder";
}

function summaryText(summary: ScanSummary): string {
  if (summary.availability !== "available") {
    return `This folder is not available right now (${summary.reason}). Your library was left as it was.`;
  }
  if (summary.cancelled) return "Scan cancelled. What was read so far is kept; scan again to finish.";
  const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const parts = [count(summary.tracksCreated, "new track", "new tracks"), `${summary.tracksUpdated} updated`];
  if (summary.markedMissing > 0) parts.push(`${summary.markedMissing} no longer on disk`);
  if (summary.tagErrors > 0) parts.push(`${summary.tagErrors} without readable tags`);
  const held = summary.heldMissing > 0
    ? ` ${count(summary.heldMissing, "file", "files")} vanished at once, so they were left unchanged in case the drive is not fully connected.`
    : "";
  return `Done: ${parts.join(", ")}.${held}`;
}

interface Props {
  admin: LibraryAdmin;
  /** Only talk to the backend while the settings dialog is showing. */
  open: boolean;
  /** Called when a scan has changed what the library holds. */
  onChanged: () => void;
}

export function Folders({ admin, open, onChanged }: Props) {
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  const [status, setStatus] = useState<ScanStatus | null>(null);
  const [watching, setWatching] = useState(false);
  const [adding, setAdding] = useState(false);
  const [confirming, setConfirming] = useState<number | null>(null);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    try {
      setFolders(await admin.folders());
      setError("");
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void refresh();
    admin.scanStatus().then((next) => {
      if (cancelled) return;
      setStatus(next);
      setWatching(next.state === "running");
    }, (reason: unknown) => { if (!cancelled) setError(message(reason)); });
    return () => { cancelled = true; };
  }, [admin, open, refresh]);

  /** Folders still to scan after the one running: Scan all goes through them one at a time. */
  const queued = useRef<number[]>([]);
  const [queuedCount, setQueuedCount] = useState(0);

  useEffect(() => {
    if (!open || !watching) return;
    let stopped = false;
    const timer = setInterval(() => {
      admin.scanStatus().then((next) => {
        if (stopped) return;
        setStatus(next);
        if (next.state !== "running") {
          void refresh();
          onChanged();
          const following = next.state === "finished" && !next.summary?.cancelled ? queued.current.shift() : undefined;
          if (following === undefined) {
            queued.current = [];
            setQueuedCount(0);
            setWatching(false);
            return;
          }
          setQueuedCount(queued.current.length);
          admin.scan(following, false).then(() => admin.scanStatus()).then((started) => { if (!stopped) setStatus(started); }, (reason: unknown) => {
            if (stopped) return;
            queued.current = [];
            setQueuedCount(0);
            setWatching(false);
            setError(message(reason));
          });
        }
      }, (reason: unknown) => {
        if (stopped) return;
        setWatching(false);
        setError(message(reason));
      });
    }, POLL_MS);
    return () => { stopped = true; clearInterval(timer); };
  }, [admin, open, watching, refresh, onChanged]);

  const scanning = status?.state === "running";
  const add = async () => {
    setAdding(true);
    try {
      const added = await admin.addFolder();
      if (added) {
        await refresh();
        // A new folder is read at once, as on the welcome card, rather than showing 0 files.
        await start(added.id);
      }
    } catch (reason) {
      setError(message(reason));
    } finally {
      setAdding(false);
    }
  };
  const start = async (folderId: number, confirmMissing = false) => {
    try {
      await admin.scan(folderId, confirmMissing);
      setStatus(await admin.scanStatus());
      setWatching(true);
      setError("");
    } catch (reason) {
      setError(message(reason));
    }
  };
  const remove = async (folderId: number) => {
    setConfirming(null);
    try {
      await admin.removeFolder(folderId);
      await refresh();
      onChanged();
    } catch (reason) {
      setError(message(reason));
    }
  };
  /** Scan every folder that is there, one after another; an offline folder is skipped. */
  const scanAll = async () => {
    const [head, ...rest] = folders.filter((folder) => folder.availability !== "unavailable").map((folder) => folder.id);
    if (head === undefined) return;
    queued.current = rest;
    setQueuedCount(rest.length);
    await start(head);
  };
  const cancel = async () => {
    // Cancel ends the run: the folders waiting are not scanned either.
    queued.current = [];
    setQueuedCount(0);
    try { await admin.cancelScan(); } catch (reason) { setError(message(reason)); }
  };

  const running = scanning ? status : null;
  const percent = running && running.tagsTotal > 0 ? Math.min(100, Math.round((running.tagsRead / running.tagsTotal) * 100)) : undefined;
  return <section aria-label="Music folders">
    <h3>Music folders</h3>
    <p className="settings-copy">openCrate reads these folders to build your library. It changes a file only when you choose to: importing into it, tidying a folder name, resolving a duplicate, or keeping a value from rekordbox.</p>
    {!admin.persistent && <p className="settings-copy">This library is temporary and is discarded when the app closes.</p>}
    {folders.length === 0
      ? <p className="settings-copy">No folders yet. Add the folder that holds your music.</p>
      : <ul className="folder-list">{folders.map((folder) => <li key={folder.id} className="folder-row folder-row-stacked">
        <div className="folder-text">
          <strong>{folder.label}</strong>
          <small title={folder.path}>{folder.path}</small>
          <small>{folder.files} files{folder.missing > 0 ? `, ${folder.missing} missing` : ""}</small>
        </div>
        <span className={`folder-badge ${folder.availability}`} title={folder.reason}>{availability(folder)}</span>
        <button className="secondary-button" disabled={scanning} aria-label={`Scan ${folder.label}`} onClick={() => void start(folder.id)}>
          {running?.rootId === folder.id ? "Scanning..." : "Scan"}
        </button>
        <button className="link-button" disabled={scanning} aria-label={`Remove ${folder.label}`} onClick={() => setConfirming(folder.id)}>Remove</button>
        {confirming === folder.id && <div className="folder-confirm" role="alertdialog" aria-label={`Remove ${folder.label}?`}>
          <span>Remove {folder.label} from the library? Your music files are not touched.</span>
          <button className="secondary-button" onClick={() => void remove(folder.id)}>Remove from library</button>
          <button className="link-button" onClick={() => setConfirming(null)}>Keep it</button>
        </div>}
      </li>)}</ul>}
    <div className="merge-actions">
      <button className="secondary-button" disabled={adding || scanning} onClick={() => void add()}>Add folder...</button>
      {folders.length > 1 && <button className="secondary-button" disabled={adding || scanning} onClick={() => void scanAll()}>Scan all folders</button>}
    </div>
    {running && <div className="scan-status" role="status">
      <progress aria-label="Scan progress" max={percent === undefined ? undefined : 100} value={percent} />
      <span>{folders.length > 1 && (queuedCount > 0 || queued.current.length > 0) ? `${folders.find((folder) => folder.id === running.rootId)?.label ?? "Folder"}, ${plural(queuedCount, "more folder", "more folders")} to go. ` : ""}{PHASES[running.phase ?? ""] ?? "Working"}{running.tagsTotal > 0 ? `: ${running.tagsRead} of ${running.tagsTotal}` : ""}</span>
      <button className="link-button" onClick={() => void cancel()}>Cancel</button>
    </div>}
    {status?.state === "finished" && status.summary && <p role="status" className="settings-copy">{summaryText(status.summary)}</p>}
    {status?.state === "finished" && status.summary && status.summary.heldMissing > 0 && status.rootId !== null && !scanning && <button className="secondary-button" onClick={() => void start(status.rootId as number, true)}>Mark them missing</button>}
    {status?.state === "failed" && <p role="alert" className="inline-error">{status.error ?? "The scan failed."}</p>}
    {error && <p role="alert" className="inline-error">{error}</p>}
  </section>;
}
