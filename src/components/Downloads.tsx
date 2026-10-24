import { useCallback, useEffect, useState } from "react";
import type { LibraryAdmin, WatchFolderInfo } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

interface Props {
  admin: LibraryAdmin;
  /** Only talk to the backend while the settings dialog is showing. */
  open: boolean;
}

export function Downloads({ admin, open }: Props) {
  const [folders, setFolders] = useState<WatchFolderInfo[]>([]);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    try {
      setFolders(await admin.checkWatchFolders());
      setError("");
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  const add = async () => {
    setAdding(true);
    try {
      if (await admin.addWatchFolder()) await refresh();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setAdding(false);
    }
  };
  const toggle = async (folder: WatchFolderInfo) => {
    try {
      await admin.setWatchFolderEnabled(folder.id, !folder.enabled);
      await refresh();
    } catch (reason) {
      setError(message(reason));
    }
  };
  const [note, setNote] = useState("");
  const retake = async (folder: WatchFolderInfo) => {
    try {
      await admin.retakeWatchBaseline(folder.id);
      setNote(`Everything in ${folder.label} now counts as already there. Only what arrives from now on is new.`);
      setError("");
    } catch (reason) {
      setError(message(reason));
    }
  };
  const remove = async (folder: WatchFolderInfo) => {
    try {
      await admin.removeWatchFolder(folder.id);
      await refresh();
    } catch (reason) {
      setError(message(reason));
    }
  };

  return <section aria-label="Download folders">
    <h3>Download folders</h3>
    <p className="settings-copy">Where new music lands when you download it. openCrate only reads these folders; it never moves, renames or deletes anything in them.</p>
    {folders.length === 0
      ? <p className="settings-copy">No download folders yet.</p>
      : <ul className="folder-list">{folders.map((folder) => <li key={folder.id} className="folder-row">
        <div className="folder-text">
          <strong>{folder.label}</strong>
          <small title={folder.path}>{folder.path}</small>
        </div>
        <span className={`folder-badge ${folder.online ? "available" : "unavailable"}`}>{folder.online ? (folder.enabled ? "Watching" : "Off") : "Offline"}</span>
        <button className="secondary-button" aria-label={`${folder.enabled ? "Turn off" : "Turn on"} ${folder.label}`} onClick={() => void toggle(folder)}>{folder.enabled ? "Turn off" : "Turn on"}</button>
        <button className="link-button" aria-label={`Count what is in ${folder.label} now as already there`} title="Only what arrives after this counts as new" onClick={() => void retake(folder)}>Count what's here as already there</button>
        <button className="link-button" aria-label={`Remove ${folder.label} from download folders`} onClick={() => void remove(folder)}>Remove</button>
      </li>)}</ul>}
    <button className="secondary-button" disabled={adding} onClick={() => void add()}>Add download folder...</button>
    {note && <p role="status" className="settings-copy">{note}</p>}
    {error && <p role="alert" className="inline-error">{error}</p>}
  </section>;
}
