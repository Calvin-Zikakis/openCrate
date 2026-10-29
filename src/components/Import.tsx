import { useCallback, useEffect, useRef, useState } from "react";
import { bringIn, needsLook, oneAtATime } from "../library/autoImport";
import { acceptedFor as acceptedOf, editCount, withEdit, type Edit, type GridField } from "../library/importEdits";
import { startPolling } from "../library/poller";
import { ImportGrid, type GridRelease } from "./ImportGrid";
import { Icon } from "./Icon";
import { AutoImportCard, type AutoImport } from "./AutoImport";
import { ImportAll } from "./ImportAll";
import { ImportChoices, loadOptions } from "./ImportChoices";
import { ImportTargetCard } from "./ImportTarget";
import type { Accepted, DownloadFolderState, DownloadRelease, DownloadsSnapshot, FolderInfo, ImportOptions, ImportPlan, ImportTarget, LibraryAdmin, ReleaseImport, ReleaseLookup } from "../library/types";

const POLL_MS = 3000;
/** Imported releases listed before "Show all". */
const IMPORTED_SHOWN = 5;
const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

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
  if (release.audioFiles > 0) parts.push(plural(release.audioFiles, "track", "tracks"));
  if (release.archives > 0) parts.push(plural(release.archives, "archive", "archives"));
  return parts.length > 0 ? parts.join(", ") : "No audio files";
}

const keyOf = (release: DownloadRelease) => `${release.folderId}:${release.loose ? "f" : "d"}:${release.name}`;

/** A plan read for one release, with what it was read against so a stale one is never shown. */
interface Planned { stamp: string; plan: ImportPlan | null; error: string }

type Lookup = { state: "asking" } | { state: "done"; result: ReleaseLookup } | { state: "failed"; error: string };

function folderNote(folder: DownloadFolderState): string | null {
  if (folder.state === "off") return `${folder.label} is switched off in Settings.`;
  if (folder.state === "offline") return `${folder.label} cannot be reached right now. It will be picked up again when it returns.`;
  if (folder.state === "unreadable") return `${folder.label} could not be read.`;
  if (folder.truncated) return `${folder.label} holds more files than are listed here.`;
  if (folder.unreadableDirs > 0) return `${folder.label}: ${folder.unreadableDirs} subfolders could not be read and are left out.`;
  return null;
}

const SETUP_KEY = "opencrate.importSetupOpen";
function loadSetupOpen(): boolean {
  try { return localStorage.getItem(SETUP_KEY) === "true"; } catch { return false; }
}
function saveSetupOpen(open: boolean) {
  try { localStorage.setItem(SETUP_KEY, String(open)); } catch { /* applied for now, not remembered */ }
}

/** What a lookup found, in a line. */
function lookupLine(result: ReleaseLookup): string {
  const { found } = result;
  if (!found) return result.note ?? "Nothing found.";
  const unmatched = result.files.filter((file) => file.matchedBy === null).length;
  const details = [found.year, found.label].filter(Boolean).join(", ");
  return `Found on ${found.source}: "${found.title}" by ${found.artist}${details ? ` (${details})` : ""}, ${plural(found.tracks, "track", "tracks")}${found.genre ? `, ${found.genre}` : ""}.${unmatched > 0 ? ` ${plural(unmatched, "file matches", "files match")} no track on it and ${unmatched === 1 ? "keeps its" : "keep their"} tags.` : ""}`;
}

interface Props {
  admin: LibraryAdmin;
  /** Importing on its own: its settings and what it did, from the app. */
  auto?: AutoImport;
  onOpenSettings: () => void;
  /** Files went into the library, so the lists should be read again. */
  onImported?: () => void;
  /** A crate was made here, so the sidebar reads them again. */
  onCratesChanged?: () => void;
  /** Opens the Recently imported screen. */
  onOpenRecent?: () => void;
}

/** The last answer, kept while the app runs, so coming back to this screen shows it at once. */
let remembered: DownloadsSnapshot | null = null;

/** What is in the watch folders now, their tags in one table to check and fix, and importing
 * them into a music folder, one release or all at once. */
export function Import({ admin, auto, onOpenSettings, onImported, onCratesChanged, onOpenRecent }: Props) {
  const [snapshot, setSnapshot] = useState<DownloadsSnapshot | null>(remembered);
  const [error, setError] = useState("");
  const [roots, setRoots] = useState<FolderInfo[]>([]);
  const [rootId, setRootId] = useState<number | null>(null);
  const [plans, setPlans] = useState<Map<string, Planned>>(new Map());
  const [importing, setImporting] = useState<{ key: string; step: "copying" | "scanning" } | null>(null);
  const [results, setResults] = useState<Map<string, ReleaseImport>>(new Map());
  const [importErrors, setImportErrors] = useState<Map<string, string>>(new Map());
  /** What happened after the last import: rekordbox, the crate, the playlist. */
  const [afterNote, setAfterNote] = useState("");
  const [options, setOptions] = useState<ImportOptions>(loadOptions);
  const [canConvert, setCanConvert] = useState(false);
  const [allImported, setAllImported] = useState(false);
  const [opened, setOpened] = useState<Set<string>>(new Set());
  const [setupOpen, setSetupOpen] = useState(loadSetupOpen);
  const [target, setTarget] = useState<ImportTarget>({ crateId: null, playlistId: null });
  /** The names behind the target, for the settings summary. */
  const [targetNames, setTargetNames] = useState<{ crate: string | null; playlist: string | null }>({ crate: null, playlist: null });
  const [rekordboxWriting, setRekordboxWriting] = useState(false);
  /** What was typed into the table, by release and then by file. */
  const [edits, setEdits] = useState<Map<string, Map<string, Edit>>>(new Map());
  const [lookups, setLookups] = useState<Map<string, Lookup>>(new Map());
  const [lookingUp, setLookingUp] = useState(false);
  const stopLookups = useRef(false);

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
    admin.importTarget().then((found) => { if (!stopped) setTarget(found); }, () => undefined);
    admin.rekordboxDatabase().then((state) => { if (!stopped) setRekordboxWriting(state.writeEnabled && !state.writeNeedsConfirmation && state.writeTested); }, () => undefined);
    return () => { stopped = true; };
  }, [admin]);

  useEffect(() => {
    let stopped = false;
    void (async () => {
      const crate = target.crateId === null ? null : (await admin.crates().catch(() => [])).find((found) => found.id === target.crateId)?.name ?? null;
      const playlist = target.playlistId === null ? null : (await admin.playlists().catch(() => [])).find((found) => found.id === target.playlistId)?.name ?? null;
      if (!stopped) setTargetNames({ crate, playlist });
    })();
    return () => { stopped = true; };
  }, [admin, target]);

  // Converting needs ffmpeg; without it the choice is shown but not sent.
  const sent: ImportOptions = canConvert ? options : { ...options, convert: { lossless: null, lossy: null } };

  /** Bumped by every import, filed or refused: an import can take the place another release meant
   * to use, so what was read before it, or is being read as it happens, no longer counts. */
  const [generation, setGeneration] = useState(0);
  const readAgain = () => setGeneration((current) => current + 1);
  const rescan = async (root: number, imported: number[]) => {
    readAgain();
    setAfterNote(await bringIn(admin, root, onImported, imported));
  };

  const acceptedFor = (release: DownloadRelease): Accepted[] => {
    const key = keyOf(release);
    const lookup = lookups.get(key);
    return acceptedOf(lookup?.state === "done" ? lookup.result : null, edits.get(key) ?? new Map());
  };
  const optionsFor = (release: DownloadRelease): ImportOptions => {
    const accepted = acceptedFor(release);
    return accepted.length > 0 ? { ...sent, accepted } : sent;
  };

  // Plans are read against these; changing either reads every plan again. A release's own
  // values (lookup, typing) are part of its stamp, so only that release is read again.
  const against = JSON.stringify([sent, rootId, generation]);
  const stampOf = (release: DownloadRelease) => `${release.audioFiles}:${release.newestModifiedMs}:${against}:${JSON.stringify(acceptedFor(release))}`;
  const stamps = useRef(new Map<string, string>());
  const planned = (release: DownloadRelease) => {
    const entry = plans.get(keyOf(release));
    return entry?.stamp === stampOf(release) ? entry : undefined;
  };

  const importNow = async (release: DownloadRelease) => {
    const key = keyOf(release);
    if (rootId === null || importing) return;
    setImporting({ key, step: "copying" });
    setImportErrors((current) => { const next = new Map(current); next.delete(key); return next; });
    try {
      await oneAtATime(async () => {
        const done = await admin.importRelease(release.folderId, release.name, rootId, optionsFor(release));
        setResults((current) => new Map(current).set(key, done));
        if (done.filed === 0) readAgain();
        if (done.filed > 0) {
          setImporting({ key, step: "scanning" });
          await rescan(done.rootId, done.imported);
        }
      });
    } catch (reason) {
      setImportErrors((current) => new Map(current).set(key, message(reason)));
      readAgain();
    } finally {
      setImporting(null);
    }
  };

  /** Look releases up one at a time; each answer goes straight into the table. */
  const lookUp = async (list: DownloadRelease[]) => {
    if (rootId === null || lookingUp) return;
    setLookingUp(true);
    stopLookups.current = false;
    try {
      for (const release of list) {
        if (stopLookups.current) break;
        const key = keyOf(release);
        setLookups((before) => new Map(before).set(key, { state: "asking" }));
        try {
          // Typed values go with the request, so the lookup matches what the table shows.
          const result = await admin.lookupRelease(release.folderId, release.name, rootId, optionsFor(release));
          setLookups((before) => new Map(before).set(key, { state: "done", result }));
        } catch (reason) {
          setLookups((before) => new Map(before).set(key, { state: "failed", error: message(reason) }));
        }
      }
    } finally {
      setLookingUp(false);
    }
  };
  const forgetLookup = (key: string) => setLookups((before) => { const next = new Map(before); next.delete(key); return next; });
  const edit = (key: string, sources: string[], field: GridField, text: string) => setEdits((before) => {
    const next = new Map(before);
    const forRelease = withEdit(before.get(key) ?? new Map(), sources, field, text);
    if (forRelease.size === 0) next.delete(key); else next.set(key, forRelease);
    return next;
  });
  const forgetEdits = (key: string) => setEdits((before) => { const next = new Map(before); next.delete(key); return next; });

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

  // Every finished release has its tags read without asking, one at a time, so the table fills
  // itself; one still downloading waits until it is ready.
  const wanted = releases.filter((release) => !release.imported && release.state === "ready");
  for (const release of wanted) stamps.current.set(keyOf(release), stampOf(release));
  const reading = useRef(new Set<string>());
  const todo = rootId === null ? [] : wanted.filter((release) => !planned(release) && !reading.current.has(`${keyOf(release)}|${stampOf(release)}`));
  const todoKey = todo.map((release) => `${keyOf(release)}|${stampOf(release)}`).join("\n");
  // A run of reads belongs to one set of choices: when they change, or the screen closes, a run
  // still going stops and hands back what it has not read yet.
  const run = useRef(0);
  useEffect(() => {
    run.current += 1;
    return () => { run.current += 1; };
  }, [admin, against]);
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
          entry = { stamp, plan: await admin.planRelease(release.folderId, release.name, rootId, optionsFor(release)), error: "" };
        } catch (reason) {
          entry = { stamp, plan: null, error: message(reason) };
        }
        reading.current.delete(`${keyOf(release)}|${stamp}`);
        // A plan read against values that have since changed is dropped. A read that failed
        // keeps the last plan's rows on screen beside its error, so a bad value never empties
        // a release's rows.
        if (stamps.current.get(keyOf(release)) === stamp) {
          setPlans((current) => new Map(current).set(keyOf(release), entry.plan ? entry : { ...entry, plan: current.get(keyOf(release))?.plan ?? null }));
        }
      }
    })();
    // todoKey names exactly the releases still to read.
  }, [admin, todoKey]);
  const status = (release: DownloadRelease, plan: ImportPlan | null) => release.imported
    ? { text: "Imported", tone: "available" }
    : plan && plan.blocked > 0 ? { text: plan.ready === 0 ? "Cannot go in yet" : `${plural(plan.blocked, "file stays", "files stay")} behind`, tone: "unavailable" }
    : plan && needsLook(plan) ? { text: "Check the tags", tone: "unavailable" }
    : badge(release);
  const available = roots.filter((folder) => folder.availability === "available");
  const noFolder = <p role="alert" className="inline-error">{roots.length > 0
    ? `${roots.map((folder) => folder.label).join(", ")} cannot be reached right now, so there is nowhere to put it. Connect the drive; this screen notices when it is back.`
    : "Add a music folder in Settings first, so there is somewhere to put it."} <button className="link-button" onClick={onOpenSettings}>Settings...</button></p>;
  const rootChoice = available.length > 1 && <label className="plan-root">Music folder <select value={rootId ?? ""} onChange={(event) => setRootId(Number(event.target.value))}>{available.map((folder) => <option key={folder.id} value={folder.id}>{folder.label}</option>)}</select></label>;
  const planOf = (release: DownloadRelease) => planned(release)?.plan ?? null;

  /** Every release in a list as one table: a heading row each, with Look up and Import. */
  const table = (list: DownloadRelease[]) => {
    const rows: GridRelease[] = list.map((release) => {
      const key = keyOf(release);
      const entry = planned(release);
      // While a release is read again (a value typed, a choice changed), its last plan stays on
      // screen, so the rows never vanish under the owner's hands.
      const plan = (entry ?? plans.get(key))?.plan ?? null;
      const step = importing?.key === key ? importing.step : null;
      const result = results.get(key);
      const lookup = lookups.get(key);
      const typed = editCount(edits.get(key) ?? new Map());
      const { text, tone } = status(release, plan);
      const waiting = !entry && wanted.includes(release) && rootId !== null;
      const canAct = plan !== null && release.state === "ready" && (!result || result.filed === 0) && !step;
      return {
        key,
        name: release.name,
        files: plan?.files ?? [],
        editable: canAct && importing === null,
        edits: edits.get(key) ?? new Map(),
        header: <>
          <span className="plan-group-name"><strong>{release.name}</strong><small>{contents(release)} - in {release.folderLabel}</small></span>
          <span className={`folder-badge ${tone}`}>{text}</span>
          {canAct && <button className="secondary-button" disabled={lookingUp || lookup?.state === "asking"} aria-label={`Look up ${release.name}`} title="Ask MusicBrainz (and Discogs with a token) for this release's tags" onClick={() => void lookUp([release])}>{lookup?.state === "asking" ? "Looking it up..." : lookup?.state === "done" ? "Look up again" : "Look up"}</button>}
          {canAct && plan.ready > 0 && <button className="import-button" disabled={importing !== null} aria-label={`Import ${release.name}`} onClick={() => void importNow(release)}>{`Import ${plural(plan.ready, "file", "files")}`}</button>}
          {result && !step && <small role="status">{result.filed === 0 ? "Nothing was imported." : `${plural(result.filed, "file is", "files are")} now in ${result.rootLabel}.`}{result.skipped.length > 0 ? ` Left where they are: ${result.skipped.map((item) => `${item.source} (${item.problem})`).join("; ")}.` : ""}</small>}
          {waiting && <small role="status">Reading the tags...</small>}
          {step && <small role="status">{step === "copying" ? "Copying and checking each file..." : "Adding them to your library..."}</small>}
          {entry?.error && <small role="alert" className="inline-error">{entry.error}</small>}
          {importErrors.get(key) && <small role="alert" className="inline-error">{importErrors.get(key)}</small>}
          {lookup?.state === "done" && <small className="lookup-note" role="status">{lookupLine(lookup.result)} {lookup.result.found && <button className="link-button" onClick={() => forgetLookup(key)}>Undo the lookup</button>}</small>}
          {lookup?.state === "failed" && <small role="alert" className="inline-error">{lookup.error} <button className="link-button" disabled={lookingUp} onClick={() => void lookUp([release])}>Try again</button></small>}
          {typed > 0 && <small className="lookup-note">{`${plural(typed, "value", "values")} typed.`} <button className="link-button" onClick={() => forgetEdits(key)}>Undo the typing</button></small>}
        </>,
      };
    });
    return <div className="plan-panel import-table">
      {rootId === null && noFolder}
      <ImportGrid releases={rows} onEdit={edit} />
    </div>;
  };

  /** An imported release: a line, and what this session's import did when opened. */
  const row = (release: DownloadRelease) => {
    const key = keyOf(release);
    const result = results.get(key);
    const open = opened.has(key);
    return <li key={key} className="folder-row">
      <div className="folder-text">
        <strong>{release.name}</strong>
        <small>{contents(release)} - in {release.folderLabel}</small>
      </div>
      <span className="folder-badge available">Imported</span>
      {result && <button className="secondary-button" aria-expanded={open} aria-label={`What happened to ${release.name}`} onClick={() => setOpened((before) => { const next = new Set(before); if (!next.delete(key)) next.add(key); return next; })}>{open ? "Hide" : "What happened"}</button>}
      {open && result && <div className="plan-panel import-result" role="status">
        <p className="settings-copy">{result.filed === 0 ? "Nothing was imported." : `${plural(result.filed, "file is", "files are")} now in ${result.rootLabel}.`} The originals are still in {release.folderLabel}.</p>
        {result.skipped.length > 0 && <>
          <p className="settings-copy">Left where they are:</p>
          <ul className="skipped-list">{result.skipped.map((item) => <li key={item.source}><strong>{item.source}</strong> {item.problem}</li>)}</ul>
        </>}
      </div>}
    </li>;
  };

  const readyIn = (list: DownloadRelease[]) => list.filter((release) => release.state === "ready");
  const lookUpAll = (list: DownloadRelease[]) => {
    const ready = readyIn(list).filter((release) => planOf(release) !== null);
    return ready.length > 1 && <button className="secondary-button" disabled={lookingUp || rootId === null} onClick={() => void lookUp(ready)}>{lookingUp ? "Looking them up..." : `Look up all ${ready.length}`}</button>;
  };
  const section = (list: DownloadRelease[], label?: (count: number) => string) => <>
    <div className="import-actions">
      <ImportAll admin={admin} releases={readyIn(list)} rootId={rootId} options={sent} acceptedFor={acceptedFor} planOf={planOf} rescan={rescan} onFinished={readAgain} label={label} />
      {lookUpAll(list)}
      {lookingUp && <button className="link-button" onClick={() => { stopLookups.current = true; }}>Stop looking up</button>}
      {rootChoice}
    </div>
    {table(list)}
  </>;
  const setupSummary = [
    sent.convert.lossless || sent.convert.lossy ? `converting ${[sent.convert.lossless && `lossless to ${sent.convert.lossless.toUpperCase()}`, sent.convert.lossy && `lossy to ${sent.convert.lossy.toUpperCase()}`].filter(Boolean).join(" and ")}` : "nothing converted",
    sent.tidy ? "names tidied" : "names as they are",
    (() => {
      const places = [targetNames.crate && `the crate ${targetNames.crate}`, rekordboxWriting && targetNames.playlist && `the rekordbox playlist ${targetNames.playlist}`].filter(Boolean);
      return places.length > 0 ? `tracks go on into ${places.join(" and ")}` : "tracks only go into the library";
    })(),
    auto?.settings.on ? "importing on its own is on" : "importing on its own is off",
  ].join(", ");

  return <div className="sync-screen">
    <div className="feature-empty sync-hero">
      <div className="empty-icon"><Icon name="folder" size={36} /></div>
      <h1>Import</h1>
      <p>New music lands in your watch folders and shows up here with its tags, one row per track. Check the tags, fix any by typing over them or looking the release up, then import. Importing copies the files into your music folder; the originals stay where they are.</p>
    </div>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {!snapshot && !error && <p className="settings-copy" role="status">Looking at your watch folders...</p>}
    {snapshot && snapshot.folders.length === 0 && <div className="sync-card">
      <h2>No watch folders yet</h2>
      <p className="settings-copy">Choose the folders your downloads land in. Anything that arrives in them from then on is listed here.</p>
      <button className="secondary-button" onClick={onOpenSettings}>Choose in Settings...</button>
    </div>}
    {notes.map((note) => <p key={note} className="settings-copy" role="status">{note}</p>)}
    {afterNote && <p className="settings-copy rekordbox-note" role="status">{afterNote} {onOpenRecent && <button className="link-button" onClick={onOpenRecent}>See what came in</button>}</p>}
    {snapshot && snapshot.folders.length > 0 && <div className="sync-card import-setup">
      <div className="import-setup-head">
        <h2>Import settings</h2>
        <button className="link-button" aria-expanded={setupOpen} onClick={() => { setSetupOpen(!setupOpen); saveSetupOpen(!setupOpen); }}>{setupOpen ? "Hide" : "Change..."}</button>
      </div>
      {!setupOpen && <p className="settings-copy">{setupSummary.charAt(0).toUpperCase() + setupSummary.slice(1)}.</p>}
      {setupOpen && <>
        <ImportChoices options={options} canConvert={canConvert} onChange={setOptions} />
        <ImportTargetCard admin={admin} target={target} onChange={setTarget} rekordboxWriting={rekordboxWriting} onCratesChanged={onCratesChanged} />
        {auto && <AutoImportCard settings={auto.settings} onChange={auto.change} outcome={auto.outcome} problem={auto.problem} lastLookMs={auto.lastLookMs} onClear={auto.clear} roots={roots} releases={releases} />}
      </>}
    </div>}
    {snapshot && snapshot.folders.length > 0 && <div className="sync-card">
      <h2>New since you added the folder</h2>
      {fresh.length === 0
        ? <p className="settings-copy">Nothing new yet. Releases that arrive from now on appear here.</p>
        : <>
          <p className="settings-copy">{`${readyIn(fresh).length} of ${plural(fresh.length, "release has", "releases have")} finished downloading.`} Look a release up to fill in what the tags lack; type over anything that is wrong.</p>
          {section(fresh)}
        </>}
    </div>}
    {imported.length > 0 && <div className="sync-card">
      <h2>Imported</h2>
      <p className="settings-copy">Already copied into your music folder. The originals are still in the watch folder.{onOpenRecent && <> <button className="link-button" onClick={onOpenRecent}>Open Recently imported</button></>}</p>
      <ul className="folder-list">{shownImported.map(row)}</ul>
      {(allImported ? imported.length > IMPORTED_SHOWN : shownImported.length < imported.length) && <button className="link-button" aria-expanded={allImported} onClick={() => setAllImported((current) => !current)}>{allImported ? "Show fewer" : `Show all ${imported.length}`}</button>}
    </div>}
    {older.length > 0 && <div className="sync-card">
      <h2>Already there when you added the folder</h2>
      <p className="settings-copy">These are not picked up on their own. They stay out of the way until you bring them in, one at a time or all together.</p>
      {section(older, (count) => `Import all ${plural(count, "release", "releases")} already there...`)}
    </div>}
  </div>;
}
