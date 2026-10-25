import { useCallback, useEffect, useRef, useState } from "react";
import { bringIn, needsLook, oneAtATime } from "../library/autoImport";
import { startPolling } from "../library/poller";
import { PlanTable, type PlanGroup } from "./PlanTable";
import { Icon } from "./Icon";
import { AutoImportCard, type AutoImport } from "./AutoImport";
import { ImportAll } from "./ImportAll";
import { ImportChoices, loadOptions } from "./ImportChoices";
import type { DownloadFolderState, DownloadRelease, DownloadsSnapshot, FolderInfo, ImportOptions, ImportPlan, LibraryAdmin, ReleaseImport } from "../library/types";

const POLL_MS = 3000;
/** Imported releases listed before "Show all". */
const IMPORTED_SHOWN = 5;
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

const keyOf = (release: DownloadRelease) => `${release.folderId}:${release.loose ? "f" : "d"}:${release.name}`;

/** A plan read for one release, with what it was read against so a stale one is never shown. */
interface Planned { stamp: string; plan: ImportPlan | null; error: string }

/** One line for a closed row: where a single file goes, or how many are ready. */
function summary(plan: ImportPlan): string {
  const only = plan.files.length === 1 ? plan.files[0] : undefined;
  if (only?.destination) return `Goes to ${only.destination}`;
  return `${plan.ready} ${plan.ready === 1 ? "file" : "files"} ready to go into ${plan.rootLabel}`;
}

function folderNote(folder: DownloadFolderState): string | null {
  if (folder.state === "off") return `${folder.label} is switched off in Settings.`;
  if (folder.state === "offline") return `${folder.label} cannot be reached right now. It will be picked up again when it returns.`;
  if (folder.state === "unreadable") return `${folder.label} could not be read.`;
  if (folder.truncated) return `${folder.label} holds more files than are listed here.`;
  if (folder.unreadableDirs > 0) return `${folder.label}: ${folder.unreadableDirs} subfolders could not be read and are left out.`;
  return null;
}

/** One table for every release, or a row per release to open one at a time. Remembered here. */
type View = "table" | "releases";
const VIEW_KEY = "opencrate.importView";
function loadView(): View {
  try { return localStorage.getItem(VIEW_KEY) === "releases" ? "releases" : "table"; } catch { return "table"; }
}
function saveView(view: View) {
  try { localStorage.setItem(VIEW_KEY, view); } catch { /* applied for now, not remembered */ }
}

interface Props {
  admin: LibraryAdmin;
  /** Importing on its own: its settings and what it did, from the app. */
  auto?: AutoImport;
  onOpenSettings: () => void;
  /** Files went into the library, so the lists should be read again. */
  onImported?: () => void;
}

/** The last answer, kept while the app runs, so coming back to this screen shows it at once. */
let remembered: DownloadsSnapshot | null = null;

/** What is in the download folders now, and importing a release from there into a music folder. */
export function Import({ admin, auto, onOpenSettings, onImported }: Props) {
  const [snapshot, setSnapshot] = useState<DownloadsSnapshot | null>(remembered);
  const [error, setError] = useState("");
  const [roots, setRoots] = useState<FolderInfo[]>([]);
  const [rootId, setRootId] = useState<number | null>(null);
  /** Rows opened or closed by hand; the rest open by themselves when something needs a look. */
  const [chosen, setChosen] = useState<Map<string, boolean>>(new Map());
  const [plans, setPlans] = useState<Map<string, Planned>>(new Map());
  const [importing, setImporting] = useState<{ key: string; step: "copying" | "scanning" } | null>(null);
  const [results, setResults] = useState<Map<string, ReleaseImport>>(new Map());
  const [importErrors, setImportErrors] = useState<Map<string, string>>(new Map());
  /** What happened to the new tracks in rekordbox after the last import. */
  const [rekordboxNote, setRekordboxNote] = useState("");
  const [options, setOptions] = useState<ImportOptions>(loadOptions);
  const [canConvert, setCanConvert] = useState(false);
  const [view, setView] = useState<View>(loadView);
  const [allImported, setAllImported] = useState(false);
  const changeView = (next: View) => { setView(next); saveView(next); };

  const readFolders = useCallback(() => admin.folders().then((found) => {
    setRoots(found);
    setRootId((current) => current ?? found.find((folder) => folder.availability === "available")?.id ?? null);
  }, () => undefined), [admin]);
  // With no music folder to import into (none added, or the drive unplugged), look again now and
  // then, so plugging the drive back in is noticed without leaving the screen.
  useEffect(() => {
    if (rootId !== null) return;
    const timer = setInterval(() => void readFolders(), POLL_MS);
    return () => clearInterval(timer);
  }, [rootId, readFolders]);
  useEffect(() => {
    let stopped = false;
    void readFolders();
    admin.folderLayout().then((layout) => { if (!stopped) setCanConvert(layout.canConvert); }, () => undefined);
    return () => { stopped = true; };
  }, [admin]);

  // Converting needs ffmpeg; without it the choice is shown but not sent.
  const sent: ImportOptions = canConvert ? options : { ...options, convert: { lossless: null, lossy: null } };

  /** New files join the library the way any file does: by a scan of the folder. With writing
   * to rekordbox on, they then go straight into rekordbox's Imported playlist. */
  /** Bumped by every import, filed or refused: an import can take the place another release meant
   * to use, so what was read before it, or is being read as it happens, no longer counts. */
  const [generation, setGeneration] = useState(0);
  const readAgain = () => setGeneration((current) => current + 1);
  const rescan = async (root: number) => {
    readAgain();
    setRekordboxNote(await bringIn(admin, root, onImported));
  };

  // Plans are read against these; changing either reads every plan again.
  const against = JSON.stringify([sent, rootId, generation]);
  const latest = useRef(against);
  latest.current = against;
  const stampOf = (release: DownloadRelease) => `${release.audioFiles}:${release.newestModifiedMs}:${against}`;
  const planned = (release: DownloadRelease) => {
    const entry = plans.get(keyOf(release));
    return entry?.stamp === stampOf(release) ? entry : undefined;
  };

  const choose = (key: string, open: boolean) => setChosen((current) => new Map(current).set(key, open));

  const importNow = async (release: DownloadRelease) => {
    const key = keyOf(release);
    if (rootId === null || importing) return;
    setImporting({ key, step: "copying" });
    setImportErrors((current) => { const next = new Map(current); next.delete(key); return next; });
    try {
      await oneAtATime(async () => {
        const done = await admin.importRelease(release.folderId, release.name, rootId, sent);
        setResults((current) => new Map(current).set(key, done));
        if (done.filed === 0) readAgain();
        if (done.filed > 0) {
          setImporting({ key, step: "scanning" });
          await rescan(done.rootId);
        }
      });
    } catch (reason) {
      setImportErrors((current) => new Map(current).set(key, message(reason)));
      readAgain();
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
  // A long history would push what is still waiting out of sight: what was imported just now comes
  // first, and the rest waits behind "Show all".
  const justNow = imported.filter((release) => results.has(keyOf(release)));
  const earlier = imported.filter((release) => !results.has(keyOf(release)));
  const shownImported = allImported ? [...justNow, ...earlier] : [...justNow, ...earlier.slice(0, Math.max(0, IMPORTED_SHOWN - justNow.length))];
  const notes = (snapshot?.folders ?? []).map(folderNote).filter((note): note is string => note !== null);
  const ready = fresh.filter((release) => release.state === "ready").length;

  // Every finished new release has its tags read without asking, one at a time, so problems show
  // in the list; in the one-table view so is every finished release already there. Any other row
  // is read when it is opened.
  const wanted = releases.filter((release) => !release.imported && ((release.state === "ready" && (fresh.includes(release) || view === "table")) || chosen.get(keyOf(release)) === true));
  const reading = useRef(new Set<string>());
  const todo = rootId === null ? [] : wanted.filter((release) => !planned(release) && !reading.current.has(`${keyOf(release)}|${stampOf(release)}`));
  const todoKey = todo.map((release) => `${keyOf(release)}|${stampOf(release)}`).join("\n");
  // A run of reads belongs to one set of choices and one view: when either changes, or the screen
  // closes, a run still going stops and hands back what it has not read yet.
  const run = useRef(0);
  useEffect(() => {
    run.current += 1;
    return () => { run.current += 1; };
  }, [admin, against, view]);
  useEffect(() => {
    if (todo.length === 0 || rootId === null) return;
    for (const release of todo) reading.current.add(`${keyOf(release)}|${stampOf(release)}`);
    const mine = run.current;
    void (async () => {
      for (const [index, release] of todo.entries()) {
        if (run.current !== mine) {
          for (const left of todo.slice(index)) reading.current.delete(`${keyOf(left)}|${stampOf(left)}`);
          return;
        }
        const stamp = stampOf(release);
        let entry: Planned;
        try {
          entry = { stamp, plan: await admin.planRelease(release.folderId, release.name, rootId, sent), error: "" };
        } catch (reason) {
          entry = { stamp, plan: null, error: message(reason) };
        }
        reading.current.delete(`${keyOf(release)}|${stamp}`);
        // A plan read against choices that have since changed is dropped.
        if (stamp.endsWith(latest.current)) {
          setPlans((current) => new Map(current).set(keyOf(release), entry));
        }
      }
    })();
    // todoKey names exactly the releases still to read.
  }, [admin, todoKey]);
  const status = (release: DownloadRelease, plan: ImportPlan | null) => release.imported
    ? { text: "Imported", tone: "available" }
    : plan && plan.blocked > 0 ? { text: "Needs attention", tone: "unavailable" }
    : plan && needsLook(plan) ? { text: "Check the tags", tone: "unavailable" }
    : badge(release);
  const available = roots.filter((folder) => folder.availability === "available");
  const noFolder = <p role="alert" className="inline-error">{roots.length > 0
    ? `${roots.map((folder) => folder.label).join(", ")} cannot be reached right now, so there is nowhere to put it. Connect the drive; this screen notices when it is back.`
    : "Add a music folder in Settings first, so there is somewhere to put it."} <button className="link-button" onClick={onOpenSettings}>Settings...</button></p>;
  const rootChoice = available.length > 1 && <label className="plan-root">Put it in <select value={rootId ?? ""} onChange={(event) => setRootId(Number(event.target.value))}>{available.map((folder) => <option key={folder.id} value={folder.id}>{folder.label}</option>)}</select></label>;

  /** Every release in a list as one table: a header row each, with its own Import button. */
  const table = (list: DownloadRelease[]) => {
    const groups: PlanGroup[] = list.map((release) => {
      const key = keyOf(release);
      const entry = planned(release);
      const plan = entry?.plan ?? null;
      const step = importing?.key === key ? importing.step : null;
      const result = results.get(key);
      const { text, tone } = status(release, plan);
      const waiting = !entry && wanted.includes(release) && rootId !== null;
      return {
        key,
        files: plan?.files ?? [],
        header: <>
          <span className="plan-group-name"><strong>{release.name}</strong><small>{contents(release)} - in {release.folderLabel}</small></span>
          <span className={`folder-badge ${tone}`}>{text}</span>
          {result && !step && <small role="status">{result.filed === 0 ? "Nothing was imported." : `${result.filed} ${result.filed === 1 ? "file is" : "files are"} now in ${result.rootLabel}.`}{result.skipped.length > 0 ? ` Left where they are: ${result.skipped.map((item) => `${item.source} (${item.problem})`).join("; ")}.` : ""}</small>}
          {plan && plan.ready > 0 && (!result || result.filed === 0) && <button className="secondary-button" disabled={importing !== null || release.state !== "ready"} aria-label={`Import ${release.name}`} onClick={() => void importNow(release)}>{`Import ${plan.ready} ${plan.ready === 1 ? "file" : "files"}`}</button>}
          {waiting && <small role="status">Reading the tags...</small>}
          {step && <small role="status">{step === "copying" ? "Copying and checking each file..." : "Adding them to your library..."}</small>}
          {entry?.error && <small role="alert" className="inline-error">{entry.error}</small>}
          {importErrors.get(key) && <small role="alert" className="inline-error">{importErrors.get(key)}</small>}
        </>,
      };
    });
    return <div className="plan-panel import-table">
      {rootId === null && noFolder}
      <p className="settings-copy">Importing copies the files; the originals stay in the download folder. Tags are written as shown: values marked ? come from the file name, and * marks a tidied value.</p>
      <PlanTable groups={groups} />
    </div>;
  };

  const row = (release: DownloadRelease) => {
    const key = keyOf(release);
    const entry = planned(release);
    const plan = release.imported ? null : entry?.plan ?? null;
    const result = results.get(key);
    const step = importing?.key === key ? importing.step : null;
    // Only new releases open by themselves; one already there opens when asked.
    const auto = release.imported ? result !== undefined : fresh.includes(release) && plan !== null && needsLook(plan);
    const open = chosen.get(key) ?? auto;
    const { text, tone } = status(release, plan);
    const waiting = !entry && wanted.includes(release) && rootId !== null;
    return <li key={key} className="folder-row">
      <div className="folder-text">
        <strong>{release.name}</strong>
        <small>{contents(release)} - in {release.folderLabel}</small>
        {!open && plan && <small className="plan-summary">{summary(plan)}</small>}
      </div>
      <span className={`folder-badge ${tone}`}>{text}</span>
      {/* An earlier import has nothing left to preview; one from this session shows what it did. */}
      {(!release.imported || result) && <button className="secondary-button" aria-expanded={open} aria-label={`Preview ${release.name}`} onClick={() => choose(key, !open)}>{open ? "Hide preview" : release.imported ? "What happened" : "Preview..."}</button>}
      {open && <div className="plan-panel">
        {rootId === null && noFolder}
        {waiting && <p className="settings-copy" role="status">Reading the tags...</p>}
        {step && <p className="settings-copy" role="status">{step === "copying" ? "Copying and checking each file..." : "Adding them to your library..."}</p>}
        {entry?.error && !release.imported && <p role="alert" className="inline-error">{entry.error}</p>}
        {importErrors.get(key) && <p role="alert" className="inline-error">{importErrors.get(key)}</p>}
        {result && !step && <div className="import-result" role="status">
          <p className="settings-copy">{result.filed === 0 ? "Nothing was imported." : `${result.filed} ${result.filed === 1 ? "file is" : "files are"} now in ${result.rootLabel}.`} The originals are still in {release.folderLabel}.</p>
          {result.skipped.length > 0 && <>
            <p className="settings-copy">Left where they are:</p>
            <ul className="skipped-list">{result.skipped.map((item) => <li key={item.source}><strong>{item.source}</strong> {item.problem}</li>)}</ul>
          </>}
        </div>}
        {/* An import that filed nothing can be tried again once the cause is dealt with. */}
        {plan && (!result || result.filed === 0) && <>
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
      <p>Releases in your download folders, and whether they have finished downloading. Their tags are read as they arrive and shown together in one table, or release by release. Importing copies files into your music folder; the download folder is never changed.</p>
    </div>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {!snapshot && !error && <p className="settings-copy" role="status">Looking at your download folders...</p>}
    {snapshot && snapshot.folders.length === 0 && <div className="sync-card">
      <h2>No download folders yet</h2>
      <p className="settings-copy">Choose the folders your downloads land in.</p>
      <button className="secondary-button" onClick={onOpenSettings}>Choose in Settings...</button>
    </div>}
    {notes.map((note) => <p key={note} className="settings-copy" role="status">{note}</p>)}
    {rekordboxNote && <p className="settings-copy rekordbox-note" role="status">{rekordboxNote}</p>}
    {snapshot && snapshot.folders.length > 0 && <ImportChoices options={options} canConvert={canConvert} onChange={setOptions} />}
    {snapshot && snapshot.folders.length > 0 && auto && <AutoImportCard settings={auto.settings} onChange={auto.change} outcome={auto.outcome} problem={auto.problem} onClear={auto.clear} roots={roots} releases={releases} />}
    {snapshot && snapshot.folders.length > 0 && <div className="format-filters import-view" role="group" aria-label="Show releases as">
      <button aria-pressed={view === "table"} onClick={() => changeView("table")}>One table</button>
      <button aria-pressed={view === "releases"} onClick={() => changeView("releases")}>By release</button>
    </div>}
    {snapshot && snapshot.folders.length > 0 && <div className="sync-card">
      <h2>New since you added the folder</h2>
      {fresh.length === 0
        ? <p className="settings-copy">Nothing new yet. Releases that arrive from now on appear here.</p>
        : <>
          <p className="settings-copy">{ready} of {fresh.length} {fresh.length === 1 ? "release has" : "releases have"} finished downloading.</p>
          {rootChoice}
          <ImportAll admin={admin} releases={fresh.filter((release) => release.state === "ready")} rootId={rootId} options={sent} rescan={rescan} onFinished={readAgain} />
          {view === "table" ? table(fresh) : <ul className="folder-list">{fresh.map(row)}</ul>}
        </>}
    </div>}
    {imported.length > 0 && <div className="sync-card">
      <h2>Imported</h2>
      <p className="settings-copy">Already copied into your music folder. The originals are still in the download folder.</p>
      <ul className="folder-list">{shownImported.map(row)}</ul>
      {(allImported ? imported.length > IMPORTED_SHOWN : shownImported.length < imported.length) && <button className="link-button" aria-expanded={allImported} onClick={() => setAllImported((current) => !current)}>{allImported ? "Show fewer" : `Show all ${imported.length}`}</button>}
    </div>}
    {older.length > 0 && <div className="sync-card">
      <h2>Already there when you added the folder</h2>
      <p className="settings-copy">These are not picked up on their own. They stay out of the way until you choose to bring them in, one at a time or all together.</p>
      {rootChoice}
      <ImportAll admin={admin} releases={older.filter((release) => release.state === "ready")} rootId={rootId} options={sent} rescan={rescan} onFinished={readAgain}
        label={(count) => `Import all ${count} ${count === 1 ? "release" : "releases"} already there...`} />
      {view === "table" ? table(older) : <ul className="folder-list">{older.map(row)}</ul>}
    </div>}
  </div>;
}
