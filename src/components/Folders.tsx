import { useCallback, useEffect, useState } from "react";
import type { FolderInfo, LibraryAdmin, ScanStatus, ScanSummary } from "../library/types";

const POLL_MS = 400;
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
  const parts = [`${summary.tracksCreated} new tracks`, `${summary.tracksUpdated} updated`];
  if (summary.markedMissing > 0) parts.push(`${summary.markedMissing} no longer on disk`);
  if (summary.tagErrors > 0) parts.push(`${summary.tagErrors} without readable tags`);
  const held = summary.heldMissing > 0
    ? ` ${summary.heldMissing} files vanished at once, so they were left unchanged in case the drive is not fully connected.`
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

  useEffect(() => {
    if (!open || !watching) return;
    let stopped = false;
    const timer = setInterval(() => {
      admin.scanStatus().then((next) => {
        if (stopped) return;
        setStatus(next);
        if (next.state !== "running") {
          setWatching(false);
          void refresh();
          onChanged();
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
      if (added) await refresh();
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
  const cancel = async () => {
    try { await admin.cancelScan(); } catch (reason) { setError(message(reason)); }
  };

  const running = scanning ? status : null;
  const percent = running && running.tagsTotal > 0 ? Math.min(100, Math.round((running.tagsRead / running.tagsTotal) * 100)) : undefined;
  return <section aria-label="Music folders">
    <h3>Music folders</h3>
    <p className="settings-copy">openCrate reads these folders to build your library. It never changes your music files.</p>
    {!admin.persistent && <p className="settings-copy">This library is temporary and is discarded when the app closes.</p>}
    {folders.length === 0
      ? <p className="settings-copy">No folders yet. Add the folder that holds your music.</p>
      : <ul className="folder-list">{folders.map((folder) => <li key={folder.id} className="folder-row">
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
    <button className="secondary-button" disabled={adding || scanning} onClick={() => void add()}>Add folder...</button>
    {running && <div className="scan-status" role="status">
      <progress aria-label="Scan progress" max={percent === undefined ? undefined : 100} value={percent} />
      <span>{PHASES[running.phase ?? ""] ?? "Working"}{running.tagsTotal > 0 ? `: ${running.tagsRead} of ${running.tagsTotal}` : ""}</span>
      <button className="link-button" onClick={() => void cancel()}>Cancel</button>
    </div>}
    {status?.state === "finished" && status.summary && <p role="status" className="settings-copy">{summaryText(status.summary)}</p>}
    {status?.state === "finished" && status.summary && status.summary.heldMissing > 0 && status.rootId !== null && !scanning && <button className="secondary-button" onClick={() => void start(status.rootId as number, true)}>Mark them missing</button>}
    {status?.state === "failed" && <p role="alert" className="inline-error">{status.error ?? "The scan failed."}</p>}
    {error && <p role="alert" className="inline-error">{error}</p>}
  </section>;
}
