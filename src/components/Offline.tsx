import { useCallback, useEffect, useState } from "react";
import type { FolderInfo, LibraryAdmin } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

interface Props {
  admin: LibraryAdmin;
  /** Changes when a scan or an import may have changed things, so the folders are looked at again. */
  version: number;
}

/**
 * Says so when a library folder is not reachable (an unplugged drive) or is not the folder that
 * was added. Checked on load, when the window regains focus and after scans. The library stays
 * browsable and nothing is marked missing.
 */
export function Offline({ admin, version }: Props) {
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const check = useCallback(async () => {
    try {
      setFolders(await admin.checkFolders());
      setError("");
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);

  useEffect(() => { void check(); }, [check, version]);
  useEffect(() => {
    const again = () => { if (document.visibilityState === "visible") void check(); };
    window.addEventListener("focus", again);
    document.addEventListener("visibilitychange", again);
    return () => {
      window.removeEventListener("focus", again);
      document.removeEventListener("visibilitychange", again);
    };
  }, [check]);

  const confirm = async (folder: FolderInfo) => {
    setBusy(true);
    try {
      await admin.confirmFolder(folder.id);
      await check();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };

  const away = folders.filter((folder) => folder.availability !== "available");
  if (away.length === 0 && !error) return null;
  return <div className="offline-banner" role="status">
    {away.map((folder) => <div key={folder.id} className="offline-row">
      {folder.availability === "unverified"
        ? <span><strong>{folder.label}</strong> is not the folder that was added: it may be a different drive or a recreated folder. Nothing was changed. If it is your library, use it anyway.</span>
        : <span><strong>{folder.label}</strong> is not available right now ({folder.reason}). Your library is still here to browse, and nothing was marked missing.</span>}
      {folder.availability === "unverified" && <button className="secondary-button" disabled={busy} onClick={() => void confirm(folder)}>Use it anyway</button>}
      <button className="link-button" disabled={busy} onClick={() => void check()}>Check again</button>
    </div>)}
    {error && <span role="alert">{error}</span>}
  </div>;
}
