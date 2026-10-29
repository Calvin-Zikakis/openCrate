import { useRef, useState } from "react";
import { oneAtATime } from "../library/autoImport";
import type { Accepted, DownloadRelease, ImportOptions, ImportPlan, LibraryAdmin } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

type Stage =
  | { kind: "idle" }
  | { kind: "checking"; done: number; total: number }
  | { kind: "confirm"; plans: { release: DownloadRelease; plan: ImportPlan }[]; failed: { name: string; error: string }[] }
  | { kind: "importing"; done: number; total: number; current: string }
  | { kind: "scanning" }
  | { kind: "finished"; filed: number; skipped: number; failed: { name: string; error: string }[]; notTried: number };

interface Props {
  admin: LibraryAdmin;
  releases: DownloadRelease[];
  rootId: number | null;
  options: ImportOptions;
  /** The table's values for a release: what the lookup found and what was typed. */
  acceptedFor: (release: DownloadRelease) => Accepted[];
  /** The plan the table shows for a release, when it has been read. */
  planOf: (release: DownloadRelease) => ImportPlan | null;
  /** Brings new files into the lists (a scan of the music folder) and on to the crate and
   * playlists chosen; `imported` names every file filed in the run. */
  rescan: (rootId: number, imported: number[]) => Promise<void>;
  /** Called once the run is over, whatever it filed, so releases are read again. */
  onFinished?: () => void;
  /** The button's words; by default "Import all N finished releases...". */
  label?: (count: number) => string;
}

/** Import every finished release in one go, after one look at the totals. */
export function ImportAll({ admin, releases, rootId, options, acceptedFor, planOf, rescan, onFinished, label }: Props) {
  const [stage, setStage] = useState<Stage>({ kind: "idle" });
  const busy = stage.kind === "checking" || stage.kind === "importing" || stage.kind === "scanning";
  /** Set by Stop: the run ends after the release in hand. */
  const stopAsked = useRef(false);
  const stop = () => { stopAsked.current = true; };
  const optionsFor = (release: DownloadRelease): ImportOptions => {
    const accepted = acceptedFor(release);
    return accepted.length > 0 ? { ...options, accepted } : options;
  };

  /** The table has most plans already; the rest are read now. */
  const check = async () => {
    if (rootId === null || busy) return;
    stopAsked.current = false;
    const plans: { release: DownloadRelease; plan: ImportPlan }[] = [];
    const failed: { name: string; error: string }[] = [];
    for (const [index, release] of releases.entries()) {
      if (stopAsked.current) { setStage({ kind: "idle" }); return; }
      const known = planOf(release);
      if (known) { plans.push({ release, plan: known }); continue; }
      setStage({ kind: "checking", done: index, total: releases.length });
      try {
        plans.push({ release, plan: await admin.planRelease(release.folderId, release.name, rootId, optionsFor(release)) });
      } catch (reason) {
        failed.push({ name: release.name, error: message(reason) });
      }
    }
    setStage({ kind: "confirm", plans, failed });
  };

  const run = async (plans: { release: DownloadRelease; plan: ImportPlan }[], earlier: { name: string; error: string }[]) => {
    if (rootId === null) return;
    const going = plans.filter(({ plan }) => plan.ready > 0);
    let filed = 0;
    let skipped = 0;
    let notTried = 0;
    const imported: number[] = [];
    const failed = [...earlier];
    stopAsked.current = false;
    for (const [index, { release }] of going.entries()) {
      if (stopAsked.current) { notTried = going.length - index; break; }
      setStage({ kind: "importing", done: index, total: going.length, current: release.name });
      try {
        const done = await oneAtATime(() => admin.importRelease(release.folderId, release.name, rootId, optionsFor(release)));
        filed += done.filed;
        skipped += done.skipped.length;
        imported.push(...done.imported);
      } catch (reason) {
        failed.push({ name: release.name, error: message(reason) });
      }
    }
    if (filed > 0) {
      setStage({ kind: "scanning" });
      await rescan(rootId, imported);
    }
    setStage({ kind: "finished", filed, skipped, failed, notTried });
    // A run that filed something was already followed by a rescan, which reads the rest again.
    if (filed === 0) onFinished?.();
  };

  if (releases.length === 0 && stage.kind === "idle") return null;
  return <div className="import-all">
    {(stage.kind === "idle" || stage.kind === "finished") && releases.length > 0 &&
      <button className="import-button" disabled={rootId === null} onClick={() => void check()}>{label ? label(releases.length) : `Import all ${plural(releases.length, "finished release", "finished releases")}...`}</button>}
    {stage.kind === "checking" && <p className="settings-copy" role="status">{`Reading the tags of release ${stage.done + 1} of ${stage.total}...`} <button className="link-button" onClick={stop}>Stop</button></p>}
    {stage.kind === "confirm" && <Confirm stage={stage} onCancel={() => setStage({ kind: "idle" })} onImport={() => void run(stage.plans, stage.failed)} />}
    {stage.kind === "importing" && <p className="settings-copy" role="status">{`Importing ${stage.done + 1} of ${stage.total}: ${stage.current}`} <button className="link-button" onClick={stop}>Stop after this one</button></p>}
    {stage.kind === "scanning" && <p className="settings-copy" role="status">Adding them to your library...</p>}
    {stage.kind === "finished" && <div role="status" className="import-result">
      <p className="settings-copy">{`${plural(stage.filed, "file is", "files are")} now in your music folder.`}{stage.skipped > 0 ? ` ${plural(stage.skipped, "file was", "files were")} left in the watch folder; the notes under each release say why.` : ""}{stage.notTried > 0 ? ` Stopped with ${plural(stage.notTried, "release", "releases")} not imported.` : ""} The originals are still in the watch folders.</p>
      {stage.failed.map((item) => <p key={item.name} className="inline-error">{`${item.name}: ${item.error}`}</p>)}
    </div>}
  </div>;
}

function Confirm({ stage, onCancel, onImport }: { stage: Extract<Stage, { kind: "confirm" }>; onCancel: () => void; onImport: () => void }) {
  const ready = stage.plans.reduce((sum, { plan }) => sum + plan.ready, 0);
  const blocked = stage.plans.reduce((sum, { plan }) => sum + plan.blocked, 0);
  const releases = stage.plans.filter(({ plan }) => plan.ready > 0).length;
  const guessed = stage.plans.reduce((sum, { plan }) => sum + plan.files.filter((file) => file.problem === null && file.tags?.fields.some((field) => field.guessed)).length, 0);
  const rootLabel = stage.plans[0]?.plan.rootLabel ?? "your music folder";
  return <div className="import-review" role="region" aria-label="Before importing">
    <p className="settings-copy">
      {ready === 0 ? "Nothing is ready to import." : `${plural(ready, "file", "files")} from ${plural(releases, "release", "releases")} will be copied into ${rootLabel}, with the tags as the table shows them.`}
      {blocked > 0 ? ` ${plural(blocked, "file stays", "files stay")} in the watch folder; the ! rows say why.` : ""}
      {guessed > 0 ? ` ${plural(guessed, "file has", "files have")} values taken from the file name (marked ?). They go in as shown; fix them in the table first if they look wrong.` : ""}
    </p>
    {stage.failed.map((item) => <p key={item.name} className="inline-error">{`${item.name}: ${item.error}`}</p>)}
    <div className="review-actions">
      <button className="import-button" disabled={ready === 0} onClick={onImport}>{`Import ${plural(ready, "file", "files")}`}</button>
      <button className="secondary-button" onClick={onCancel}>Cancel</button>
    </div>
  </div>;
}
