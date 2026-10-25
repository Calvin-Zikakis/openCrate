import { useState } from "react";
import { oneAtATime } from "../library/autoImport";
import type { Accepted, DownloadRelease, ImportOptions, ImportPlan, LibraryAdmin, PlanFile, ReleaseLookup } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

/** A file that will go in but whose tags deserve a look: a warning, or values guessed from its name. */
export interface Doubt {
  release: string;
  file: string;
  reasons: string[];
}

export function doubtsIn(release: string, files: PlanFile[]): Doubt[] {
  return files.flatMap((file) => {
    if (file.problem !== null || !file.tags) return [];
    const guessed = file.tags.fields.filter((field) => field.guessed).map((field) => field.name.toLowerCase());
    const reasons = [...file.tags.warnings.filter((warning) => !warning.includes("came from the file name"))];
    if (guessed.length > 0) reasons.push(`No ${guessed.join(", ")} in the tags; taken from the file name.`);
    if (reasons.length === 0) return [];
    const title = file.tags.fields.find((field) => field.name === "Title")?.value;
    return [{ release, file: title ?? file.source.split("/").at(-1) ?? file.source, reasons }];
  });
}

const FIELD_KEYS: Record<string, keyof Omit<Accepted, "source">> = {
  Title: "title", Artist: "artist", Album: "album", "Album artist": "albumArtist", Label: "label", Genre: "genre", Year: "year", Track: "track", Disc: "disc",
};
const NUMBERS = new Set(["year", "track", "disc"]);

/** The values to send with an import: every suggested change, except for files the owner unticked. */
export function acceptedFrom(lookup: ReleaseLookup, unticked: ReadonlySet<string>): Accepted[] {
  return lookup.files.filter((file) => file.changes.length > 0 && !unticked.has(file.source)).map((file) => {
    const accepted: Accepted = { source: file.source };
    for (const change of file.changes) {
      const key = FIELD_KEYS[change.field];
      if (!key) continue;
      const value = NUMBERS.has(key) ? Number(change.to) : change.to;
      if (typeof value === "number" && !Number.isFinite(value)) continue;
      (accepted as unknown as Record<string, string | number>)[key] = value;
    }
    return accepted;
  });
}

type Lookup = { state: "asking" } | { state: "done"; result: ReleaseLookup } | { state: "failed"; error: string };
const keyOf = (release: DownloadRelease) => `${release.folderId}:${release.name}`;

type Stage =
  | { kind: "idle" }
  | { kind: "checking"; done: number; total: number }
  | { kind: "review"; plans: { release: DownloadRelease; plan: ImportPlan }[]; failed: { name: string; error: string }[] }
  | { kind: "importing"; done: number; total: number; current: string }
  | { kind: "scanning" }
  | { kind: "finished"; filed: number; skipped: number; failed: { name: string; error: string }[] };

interface Props {
  admin: LibraryAdmin;
  releases: DownloadRelease[];
  rootId: number | null;
  options: ImportOptions;
  /** Brings new files into the lists: a scan of the music folder, then a refresh. */
  rescan: (rootId: number) => Promise<void>;
  /** Called once the run is over, whatever it filed, so releases are read again. */
  onFinished?: () => void;
  /** The button's words; by default "Import all N finished releases...". */
  label?: (count: number) => string;
}

/** Import every finished release at once, after a look at what will happen. */
export function ImportAll({ admin, releases, rootId, options, rescan, onFinished, label }: Props) {
  const [stage, setStage] = useState<Stage>({ kind: "idle" });
  const [lookups, setLookups] = useState<Record<string, Lookup>>({});
  const [unticked, setUnticked] = useState<Set<string>>(new Set());
  const busy = stage.kind === "checking" || stage.kind === "importing" || stage.kind === "scanning";

  const check = async () => {
    if (rootId === null || busy) return;
    const plans: { release: DownloadRelease; plan: ImportPlan }[] = [];
    const failed: { name: string; error: string }[] = [];
    for (const [index, release] of releases.entries()) {
      setStage({ kind: "checking", done: index, total: releases.length });
      try {
        plans.push({ release, plan: await admin.planRelease(release.folderId, release.name, rootId, options) });
      } catch (reason) {
        failed.push({ name: release.name, error: message(reason) });
      }
    }
    setLookups({});
    setUnticked(new Set());
    setStage({ kind: "review", plans, failed });
  };

  /** Look up each release with doubtful tags, one at a time. */
  const lookUp = async (plans: { release: DownloadRelease; plan: ImportPlan }[]) => {
    if (rootId === null) return;
    for (const { release } of plans) {
      const key = keyOf(release);
      setLookups((before) => ({ ...before, [key]: { state: "asking" } }));
      try {
        const result = await admin.lookupRelease(release.folderId, release.name, rootId, options);
        setLookups((before) => ({ ...before, [key]: { state: "done", result } }));
      } catch (reason) {
        setLookups((before) => ({ ...before, [key]: { state: "failed", error: message(reason) } }));
      }
    }
  };

  const run = async (plans: { release: DownloadRelease; plan: ImportPlan }[], earlier: { name: string; error: string }[]) => {
    if (rootId === null) return;
    const going = plans.filter(({ plan }) => plan.ready > 0);
    let filed = 0;
    let skipped = 0;
    const failed = [...earlier];
    for (const [index, { release }] of going.entries()) {
      setStage({ kind: "importing", done: index, total: going.length, current: release.name });
      try {
        const lookup = lookups[keyOf(release)];
        const accepted = lookup?.state === "done" ? acceptedFrom(lookup.result, unticked) : [];
        const done = await oneAtATime(() => admin.importRelease(release.folderId, release.name, rootId, accepted.length > 0 ? { ...options, accepted } : options));
        filed += done.filed;
        skipped += done.skipped.length;
      } catch (reason) {
        failed.push({ name: release.name, error: message(reason) });
      }
    }
    if (filed > 0) {
      setStage({ kind: "scanning" });
      await rescan(rootId);
    }
    setStage({ kind: "finished", filed, skipped, failed });
    // A run that filed something was already followed by a rescan, which reads the rest again.
    if (filed === 0) onFinished?.();
  };

  if (releases.length === 0 && stage.kind === "idle") return null;
  return <div className="import-all">
    {(stage.kind === "idle" || stage.kind === "finished") && releases.length > 0 &&
      <button className="import-button" disabled={rootId === null} onClick={() => void check()}>{label ? label(releases.length) : `Import all ${plural(releases.length, "finished release", "finished releases")}...`}</button>}
    {stage.kind === "checking" && <p className="settings-copy" role="status">{`Reading the tags of release ${stage.done + 1} of ${stage.total}...`}</p>}
    {stage.kind === "review" && <Review stage={stage} lookups={lookups} unticked={unticked}
      onToggle={(source) => setUnticked((before) => { const next = new Set(before); if (next.has(source)) next.delete(source); else next.add(source); return next; })}
      onLookUp={(plans) => void lookUp(plans)}
      onCancel={() => setStage({ kind: "idle" })} onImport={() => void run(stage.plans, stage.failed)} />}
    {stage.kind === "importing" && <p className="settings-copy" role="status">{`Importing ${stage.done + 1} of ${stage.total}: ${stage.current}`}</p>}
    {stage.kind === "scanning" && <p className="settings-copy" role="status">Adding them to your library...</p>}
    {stage.kind === "finished" && <div role="status" className="import-result">
      <p className="settings-copy">{`${plural(stage.filed, "file is", "files are")} now in your music folder.`}{stage.skipped > 0 ? ` ${plural(stage.skipped, "file was", "files were")} left in the download folder; the notes under each release say why.` : ""} The originals are still in the download folders.</p>
      {stage.failed.map((item) => <p key={item.name} className="inline-error">{`${item.name}: ${item.error}`}</p>)}
    </div>}
  </div>;
}

function Review({ stage, lookups, unticked, onToggle, onLookUp, onCancel, onImport }: {
  stage: Extract<Stage, { kind: "review" }>;
  lookups: Record<string, Lookup>;
  unticked: ReadonlySet<string>;
  onToggle: (source: string) => void;
  onLookUp: (plans: { release: DownloadRelease; plan: ImportPlan }[]) => void;
  onCancel: () => void;
  onImport: () => void;
}) {
  const ready = stage.plans.reduce((sum, { plan }) => sum + plan.ready, 0);
  const blocked = stage.plans.reduce((sum, { plan }) => sum + plan.blocked, 0);
  const releases = stage.plans.filter(({ plan }) => plan.ready > 0).length;
  const doubts = stage.plans.flatMap(({ release, plan }) => doubtsIn(release.name, plan.files));
  const doubtful = stage.plans.filter(({ release, plan }) => doubtsIn(release.name, plan.files).length > 0);
  const asking = Object.values(lookups).some((lookup) => lookup.state === "asking");
  const asked = doubtful.some(({ release }) => lookups[keyOf(release)] !== undefined);
  return <div className="import-review" role="region" aria-label="Before importing">
    <p className="settings-copy">{`${plural(ready, "file", "files")} from ${plural(releases, "release", "releases")} will be imported.`}{blocked > 0 ? ` ${plural(blocked, "file stays", "files stay")} in the download folder (the notes under each release say why).` : ""}</p>
    {stage.failed.map((item) => <p key={item.name} className="inline-error">{`${item.name}: ${item.error}`}</p>)}
    {doubts.length > 0 && <>
      <h3>{`${plural(doubts.length, "file has", "files have")} tags worth a look`}</h3>
      <p className="settings-copy">They will be imported as shown, unless you look them up and keep what the lookup suggests. To change them by hand, import them and use Edit details on the tracks afterwards.</p>
      <ul className="doubt-list">{doubts.map((doubt) => <li key={`${doubt.release}/${doubt.file}`}>
        <strong>{doubt.file}</strong> <small>{doubt.release}</small>
        <ul>{doubt.reasons.map((reason) => <li key={reason}>{reason}</li>)}</ul>
      </li>)}</ul>
    </>}
    {doubtful.length > 0 && !asked && <div className="lookup-offer">
      <button className="secondary-button" onClick={() => onLookUp(doubtful)}>{`Look up ${plural(doubtful.length, "release", "releases")}`}</button>
      <small>Sends the artist and album names to musicbrainz.org (and to Discogs, with your token from Settings), one request a second. Nothing changes until you import.</small>
    </div>}
    {doubtful.map(({ release }) => {
      const lookup = lookups[keyOf(release)];
      if (!lookup) return null;
      return <div key={keyOf(release)} className="lookup-result" role="group" aria-label={`Lookup: ${release.name}`}>
        <h3>{release.name}</h3>
        {lookup.state === "asking" && <p className="settings-copy" role="status">Looking it up...</p>}
        {lookup.state === "failed" && <p className="inline-error">{lookup.error}</p>}
        {lookup.state === "done" && <LookupResult lookup={lookup.result} unticked={unticked} onToggle={onToggle} />}
      </div>;
    })}
    <div className="review-actions">
      <button className="import-button" disabled={ready === 0 || asking} onClick={onImport}>{`Import ${plural(ready, "file", "files")}`}</button>
      <button className="secondary-button" onClick={onCancel}>Cancel</button>
    </div>
  </div>;
}

function LookupResult({ lookup, unticked, onToggle }: { lookup: ReleaseLookup; unticked: ReadonlySet<string>; onToggle: (source: string) => void }) {
  if (!lookup.found) return <p className="settings-copy">{lookup.note ?? "Nothing found."}</p>;
  const { found } = lookup;
  const changing = lookup.files.filter((file) => file.changes.length > 0);
  const unmatched = lookup.files.filter((file) => file.matchedBy === null).length;
  return <>
    <p className="settings-copy">{`Found on ${found.source}: "${found.title}" by ${found.artist}${found.year ? ` (${found.year}` : ""}${found.label ? `${found.year ? ", " : " ("}${found.label}` : ""}${found.year || found.label ? ")" : ""}, ${plural(found.tracks, "track", "tracks")}${found.genre ? `, ${found.genre}` : ""}.`} <span className="lookup-url">{found.url}</span></p>
    {changing.length === 0 && <p className="settings-copy">The tags already agree with it.</p>}
    <ul className="suggestion-list">{changing.map((file) => <li key={file.source}>
      <label><input type="checkbox" checked={!unticked.has(file.source)} onChange={() => onToggle(file.source)} /> <strong>{file.source.split("/").at(-1)}</strong> <small>{`matched by ${file.matchedBy ?? "nothing"}`}</small></label>
      <ul>{file.changes.map((change) => <li key={change.field}>{`${change.field}: `}<span className="from">{change.from ?? "(none)"}</span>{" -> "}<span className="to">{change.to}</span></li>)}</ul>
    </li>)}</ul>
    {unmatched > 0 && <p className="settings-copy">{`${plural(unmatched, "file matches", "files match")} no track on this release and ${unmatched === 1 ? "keeps its" : "keep their"} tags.`}</p>}
  </>;
}
