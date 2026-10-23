import { useEffect, useState } from "react";
import { startPolling } from "../library/poller";
import { PlanTable } from "./PlanTable";
import { Icon } from "./Icon";
import { ImportAll } from "./ImportAll";
import { ImportChoices, loadOptions } from "./ImportChoices";
import type { DownloadFolderState, DownloadRelease, DownloadsSnapshot, FolderInfo, ImportOptions, ImportPlan, LibraryAdmin, ReleaseImport } from "../library/types";

const POLL_MS = 3000;
/** How long to wait for the scan that brings imported files into the lists. */
const SCAN_WAIT_MS = 10 * 60_000;
const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

function badge(release: DownloadRelease): { text: string; tone: string } {
  switch (release.state) {
    case "ready": return { text: "Ready", tone: "available" };
    case "downloading": return { text: "Still downloading", tone: "unavailable" };
    case "settling": return { text: `Checking it has finished${release.quietSeconds ? ` (${release.quietSeconds}s)` : ""}`, tone: "unavailable" };
    default: return { text: "No music yet", tone: "" };
  }
}

function contents(release: DownloadRelease): string {
  const parts = [];
  if (release.audioFiles > 0) parts.push(`${release.audioFiles} ${release.audioFiles === 1 ? "track" : "tracks"}`);
  if (release.archives > 0) parts.push(`${release.archives} ${release.archives === 1 ? "archive" : "archives"}`);
  return parts.length > 0 ? parts.join(", ") : "No audio files";
}

function folderNote(folder: DownloadFolderState): string | null {
  if (folder.state === "off") return `${folder.label} is switched off in Settings.`;
  if (folder.state === "offline") return `${folder.label} cannot be reached right now. It will be picked up again when it returns.`;
  if (folder.state === "unreadable") return `${folder.label} could not be read.`;
  if (folder.truncated) return `${folder.label} holds more files than are listed here.`;
  if (folder.unreadableDirs > 0) return `${folder.label}: ${folder.unreadableDirs} subfolders could not be read and are left out.`;
  return null;
}

interface Props {
  admin: LibraryAdmin;
  onOpenSettings: () => void;
  /** Files went into the library, so the lists should be read again. */
  onImported?: () => void;
}

/** The last answer, kept while the app runs, so coming back to this screen shows it at once. */
let remembered: DownloadsSnapshot | null = null;

/** What is in the download folders now, and importing a release from there into a music folder. */
export function Import({ admin, onOpenSettings, onImported }: Props) {
  const [snapshot, setSnapshot] = useState<DownloadsSnapshot | null>(remembered);
  const [error, setError] = useState("");
  const [roots, setRoots] = useState<FolderInfo[]>([]);
  const [rootId, setRootId] = useState<number | null>(null);
  const [previewKey, setPreviewKey] = useState<string | null>(null);
  const [plan, setPlan] = useState<ImportPlan | null>(null);
  const [planning, setPlanning] = useState(false);
  const [planError, setPlanError] = useState("");
  const [importing, setImporting] = useState<"copying" | "scanning" | null>(null);
  const [result, setResult] = useState<ReleaseImport | null>(null);
  const [options, setOptions] = useState<ImportOptions>(loadOptions);
  const [canConvert, setCanConvert] = useState(false);

  useEffect(() => {
    let stopped = false;
    admin.folders().then((found) => {
      if (stopped) return;
      setRoots(found);
      setRootId((current) => current ?? found.find((folder) => folder.availability === "available")?.id ?? null);
    }, () => undefined);
    admin.folderLayout().then((layout) => { if (!stopped) setCanConvert(layout.canConvert); }, () => undefined);
    return () => { stopped = true; };
  }, [admin]);

  // Converting needs ffmpeg; without it the choice is shown but not sent.
  const sent: ImportOptions = canConvert ? options : { ...options, convert: { lossless: null, lossy: null } };

  /** New files join the library the way any file does: by a scan of the folder. */
  const rescan = async (root: number) => {
    await admin.scan(root).catch(() => undefined);
    for (let waited = 0; waited < SCAN_WAIT_MS; waited += 500) {
      if ((await admin.scanStatus().catch(() => null))?.state !== "running") break;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
    onImported?.();
  };

  const preview = async (release: DownloadRelease, key: string) => {
    if (previewKey === key) { setPreviewKey(null); return; }
    setPreviewKey(key);
    setPlan(null);
    setPlanError("");
    setResult(null);
    if (rootId === null) { setPlanError("Add a music folder in Settings first, so there is somewhere to put it."); return; }
    setPlanning(true);
    try {
      setPlan(await admin.planRelease(release.folderId, release.name, rootId, sent));
    } catch (reason) {
      setPlanError(message(reason));
    } finally {
      setPlanning(false);
    }
  };

  const importNow = async (release: DownloadRelease) => {
    if (rootId === null || importing) return;
    setImporting("copying");
    setPlanError("");
    try {
      const done = await admin.importRelease(release.folderId, release.name, rootId, sent);
      setResult(done);
      setPlan(null);
      if (done.filed > 0) {
        setImporting("scanning");
        await rescan(done.rootId);
      }
    } catch (reason) {
      setPlanError(message(reason));
    } finally {
      setImporting(null);
    }
  };

  useEffect(() => startPolling(
    () => admin.downloadReleases(),
    (next) => {
      remembered = next;
      setSnapshot(next);
      setError("");
    },
    (reason) => { setError(message(reason)); },
    POLL_MS,
  ), [admin]);

  const releases = snapshot?.releases ?? [];
  const fresh = releases.filter((release) => !release.imported && !release.alreadyThere && release.state !== "nothing");
  const older = releases.filter((release) => !release.imported && release.alreadyThere && release.state !== "nothing");
  const imported = releases.filter((release) => release.imported);
  const notes = (snapshot?.folders ?? []).map(folderNote).filter((note): note is string => note !== null);
  const ready = fresh.filter((release) => release.state === "ready").length;
  const row = (release: DownloadRelease) => {
    const { text, tone } = release.imported ? { text: "Imported", tone: "available" } : badge(release);
    const key = `${release.folderId}:${release.loose ? "f" : "d"}:${release.name}`;
    return <li key={key} className="folder-row">
      <div className="folder-text">
        <strong>{release.name}</strong>
        <small>{contents(release)} - in {release.folderLabel}</small>
      </div>
      <span className={`folder-badge ${tone}`}>{text}</span>
      <button className="secondary-button" aria-expanded={previewKey === key} aria-label={`Preview ${release.name}`} onClick={() => void preview(release, key)}>{previewKey === key ? "Hide preview" : "Preview..."}</button>
      {previewKey === key && <div className="plan-panel">
        {roots.filter((folder) => folder.availability === "available").length > 1 && <label className="plan-root">Put it in <select value={rootId ?? ""} onChange={(event) => { setRootId(Number(event.target.value)); setPreviewKey(null); }}>{roots.filter((folder) => folder.availability === "available").map((folder) => <option key={folder.id} value={folder.id}>{folder.label}</option>)}</select></label>}
        {planning && <p className="settings-copy" role="status">Reading the tags...</p>}
        {importing && <p className="settings-copy" role="status">{importing === "copying" ? "Copying and checking each file..." : "Adding them to your library..."}</p>}
        {planError && <p role="alert" className="inline-error">{planError}</p>}
        {result && !importing && <div className="import-result" role="status">
          <p className="settings-copy">{result.filed === 0 ? "Nothing was imported." : `${result.filed} ${result.filed === 1 ? "file is" : "files are"} now in ${result.rootLabel}.`} The originals are still in {release.folderLabel}.</p>
          {result.skipped.length > 0 && <>
            <p className="settings-copy">Left where they are:</p>
            <ul className="skipped-list">{result.skipped.map((item) => <li key={item.source}><strong>{item.source}</strong> {item.problem}</li>)}</ul>
          </>}
        </div>}
        {plan && <>
          <p className="settings-copy">{plan.ready} {plan.ready === 1 ? "file is" : "files are"} ready to go into {plan.rootLabel}{plan.blocked > 0 ? `; ${plan.blocked} ${plan.blocked === 1 ? "needs attention and stays" : "need attention and stay"} in the download folder` : ""}. Importing copies them; the originals stay in {release.folderLabel}. Tags are written as shown: values marked ? come from the file name, and * marks a tidied value.</p>
          {plan.ready > 0 && <button className="import-button" disabled={importing !== null || release.state !== "ready"} onClick={() => void importNow(release)}>{`Import ${plan.ready} ${plan.ready === 1 ? "file" : "files"}`}</button>}
          {release.state !== "ready" && <p className="settings-copy">It can be imported once it has finished downloading.</p>}
          <PlanTable files={plan.files} />
        </>}
      </div>}
    </li>;
  };

  return <div className="sync-screen">
    <div className="feature-empty sync-hero">
      <div className="empty-icon"><Icon name="folder" size={36} /></div>
      <h1>Import</h1>
      <p>Releases in your download folders, and whether they have finished downloading. Preview one to check its tags and import it. Importing copies files into your music folder; the download folder is never changed.</p>
    </div>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {!snapshot && !error && <p className="settings-copy" role="status">Looking at your download folders...</p>}
    {snapshot && snapshot.folders.length === 0 && <div className="sync-card">
      <h2>No download folders yet</h2>
      <p className="settings-copy">Choose the folders where Bandcamp and Soulseek put your downloads.</p>
      <button className="secondary-button" onClick={onOpenSettings}>Choose in Settings...</button>
    </div>}
    {notes.map((note) => <p key={note} className="settings-copy" role="status">{note}</p>)}
    {snapshot && snapshot.folders.length > 0 && <ImportChoices options={options} canConvert={canConvert} onChange={(next) => { setOptions(next); setPreviewKey(null); setPlan(null); }} />}
    {snapshot && snapshot.folders.length > 0 && <div className="sync-card">
      <h2>New since you added the folder</h2>
      {fresh.length === 0
        ? <p className="settings-copy">Nothing new yet. Releases that arrive from now on appear here.</p>
        : <>
          <p className="settings-copy">{ready} of {fresh.length} {fresh.length === 1 ? "release has" : "releases have"} finished downloading.</p>
          <ImportAll admin={admin} releases={fresh.filter((release) => release.state === "ready")} rootId={rootId} options={sent} rescan={rescan} />
          <ul className="folder-list">{fresh.map(row)}</ul>
        </>}
    </div>}
    {imported.length > 0 && <div className="sync-card">
      <h2>Imported</h2>
      <p className="settings-copy">Already copied into your music folder. The originals are still in the download folder.</p>
      <ul className="folder-list">{imported.map(row)}</ul>
    </div>}
    {older.length > 0 && <div className="sync-card">
      <h2>Already there when you added the folder</h2>
      <p className="settings-copy">These are not picked up on their own. They stay out of the way until you choose to bring them in.</p>
      <ul className="folder-list">{older.map(row)}</ul>
    </div>}
  </div>;
}
