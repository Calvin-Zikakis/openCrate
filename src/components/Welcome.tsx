import { useCallback, useEffect, useState } from "react";
import type { FolderInfo, LibraryAdmin, WatchFolderInfo } from "../library/types";

/** "started" once shown, "done" once put off: it stays until the owner says so. */
const KEY = "opencrate.welcome";
const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const sleep = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

function remembered(): string | null {
  try { return localStorage.getItem(KEY); } catch { return null; }
}

function remember(value: "started" | "done") {
  try { localStorage.setItem(KEY, value); } catch { /* asked again next time */ }
}

interface Props {
  admin: LibraryAdmin;
  onOpenRekordbox: () => void;
  /** Called when a scan has filled the library. */
  onLibraryChanged: () => void;
}

/** First start of a library on disk: the three things to set up, each done here. */
export function Welcome({ admin, onOpenRekordbox, onLibraryChanged }: Props) {
  const [shown, setShown] = useState<boolean | null>(null);
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  const [downloads, setDownloads] = useState<WatchFolderInfo[]>([]);
  const [hasMirror, setHasMirror] = useState(false);
  const [scanning, setScanning] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    const [nextFolders, nextDownloads, mirrors] = await Promise.all([admin.folders(), admin.watchFolders(), admin.mirrors()]);
    setFolders(nextFolders);
    setDownloads(nextDownloads);
    setHasMirror(mirrors.length > 0);
    return nextFolders;
  }, [admin]);

  useEffect(() => {
    const state = remembered();
    if (!admin.persistent || state === "done") {
      setShown(false);
      return;
    }
    // Shown for a library with no music folder yet; once shown it stays until put off.
    load().then((found) => {
      const show = state === "started" || found.length === 0;
      if (show) remember("started");
      setShown(show);
    }, () => setShown(false));
  }, [admin, load]);

  if (!shown) return null;

  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try {
      await work();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const addMusic = () => act(async () => {
    const added = await admin.addFolder();
    if (!added) return;
    await admin.scan(added.id);
    for (;;) {
      const status = await admin.scanStatus();
      if (status.state === "running") {
        setScanning(status.tagsTotal > 0 ? `Reading ${added.label}: ${status.tagsRead} of ${status.tagsTotal} files` : `Looking through ${added.label}...`);
        await sleep(400);
        continue;
      }
      if (status.state === "failed") throw new Error(status.error ?? "The folder could not be read.");
      break;
    }
    setScanning("");
    await load();
    onLibraryChanged();
  });
  const addDownloads = () => act(async () => {
    if (await admin.addWatchFolder()) await load();
  });
  const finish = () => {
    remember("done");
    setShown(false);
  };

  return <section className="welcome" role="region" aria-label="Welcome to openCrate">
    <h2>Welcome to openCrate</h2>
    <p className="settings-copy">Three things to set up. Each can be changed later in Settings.</p>
    <ol className="welcome-steps">
      <li className={folders.length > 0 ? "is-done" : ""}>
        <strong>Your music folder</strong>
        <small>Where your library lives. openCrate reads it; nothing in it is moved or renamed.</small>
        {folders.map((folder) => <span key={folder.id} className="welcome-done">{`${folder.label}: ${folder.files} files read`}</span>)}
        {scanning && <span role="status">{scanning}</span>}
        <button className="secondary-button" disabled={busy} onClick={() => void addMusic()}>{folders.length > 0 ? "Add another music folder..." : "Add your music folder..."}</button>
      </li>
      <li className={downloads.length > 0 ? "is-done" : ""}>
        <strong>Download folders</strong>
        <small>Where new music lands when you download it. New releases there show on the Import screen; the originals are never changed.</small>
        {downloads.map((folder) => <span key={folder.id} className="welcome-done">{folder.label}</span>)}
        <button className="secondary-button" disabled={busy} onClick={() => void addDownloads()}>{downloads.length > 0 ? "Add another download folder..." : "Add a download folder..."}</button>
      </li>
      <li className={hasMirror ? "is-done" : ""}>
        <strong>rekordbox</strong>
        <small>Bring in your playlists, cues and beat grids from an XML export, or read rekordbox's library directly once it is switched on in Settings.</small>
        <button className="secondary-button" onClick={onOpenRekordbox}>Open the rekordbox screen</button>
      </li>
    </ol>
    {error && <p role="alert" className="inline-error">{error}</p>}
    <button className="import-button" onClick={finish}>Done</button>
  </section>;
}
