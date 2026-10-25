import { useState } from "react";
import type { LibraryAdmin, MergeApplied, MergeChange, MergeChoice, MergeField, MergeReview } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
const LABELS: Record<MergeField, string> = { title: "Title", artist: "Artist", album: "Album", year: "Year", label: "Label" };
const shown = (value: string) => (value === "" ? "(empty)" : value);
const keyOf = (change: MergeChange) => `${change.rbKey}:${change.field}`;
/** Rows drawn per group; ticking a whole group still covers every change in it. */
const DRAWN = 300;

type Side = "openCrate" | "rekordbox";

interface Props {
  admin: LibraryAdmin;
  /** Files or rekordbox changed: the library should be read again. */
  onChanged: () => void;
}

/** The rekordbox screen: what differs between openCrate and rekordbox since they last agreed,
 * reviewed and applied like a Git merge. Nothing is applied that is not ticked. */
export function RekordboxMerge({ admin, onChanged }: Props) {
  const [review, setReview] = useState<MergeReview | null>(null);
  /** Each change's chosen side; a change with none is not applied. */
  const [kept, setKept] = useState<Map<string, Side>>(new Map());
  const [applied, setApplied] = useState<MergeApplied | null>(null);
  const [busy, setBusy] = useState<"reviewing" | "syncing" | null>(null);
  /** Rows drawn per group; Show more adds the next lot. */
  const [drawn, setDrawn] = useState<Record<string, number>>({});
  const [error, setError] = useState("");

  const load = async () => {
    const next = await admin.reviewRekordboxMerge();
    setReview(next);
    // One-sided changes are ticked; a change only rekordbox's writer can make waits for writing
    // to be on; a change made on both sides waits for the owner to pick a side.
    setKept(new Map(next.changes.flatMap((change): [string, Side][] => {
      if (change.direction === "toOpenCrate") return [[keyOf(change), "rekordbox"]];
      if (change.direction === "toRekordbox" && next.canWrite) return [[keyOf(change), "openCrate"]];
      return [];
    })));
  };
  const act = async (work: () => Promise<void>, doing: "reviewing" | "syncing") => {
    setBusy(doing);
    setError("");
    try {
      await work();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };
  const choose = (change: MergeChange, side: Side | null) => setKept((current) => {
    const next = new Map(current);
    if (side) next.set(keyOf(change), side);
    else next.delete(keyOf(change));
    return next;
  });
  const sync = () => act(async () => {
    if (!review) return;
    const choices: MergeChoice[] = review.changes.flatMap((change) => {
      const side = kept.get(keyOf(change));
      return side ? [{ rbKey: change.rbKey, field: change.field, keep: side, ours: change.ours, theirs: change.theirs }] : [];
    });
    setApplied(await admin.applyRekordboxMerge(choices));
    onChanged();
    await load();
  }, "syncing");

  const changes = review?.changes ?? [];
  const group = (direction: MergeChange["direction"]) => changes.filter((change) => change.direction === direction);
  const ticked = changes.filter((change) => kept.has(keyOf(change))).length;

  const row = (change: MergeChange) => {
    const side = kept.get(keyOf(change));
    const conflict = change.direction === "conflict";
    const locked = change.direction === "toRekordbox" && !review?.canWrite;
    const name = `${change.title}, ${change.field}`;
    return <li key={keyOf(change)} className="merge-row">
      <label className="send-row">
        <input type="checkbox" aria-label={name} checked={side !== undefined} disabled={busy !== null || locked || (conflict && side === undefined)} title={conflict && side === undefined ? "Pick which value to keep first" : locked ? "Needs writing to rekordbox on in Settings" : undefined}
          onChange={(event) => choose(change, event.target.checked ? (change.direction === "toRekordbox" ? "openCrate" : "rekordbox") : null)} />
        <span className="folder-text">
          <strong>{change.title}</strong>
          <small>{change.direction === "toOpenCrate"
            ? `${LABELS[change.field]}: ${shown(change.ours)} -> ${shown(change.theirs)}`
            : change.direction === "toRekordbox"
              ? `${LABELS[change.field]}: rekordbox has ${shown(change.theirs)} -> ${shown(change.ours)}`
              : `${LABELS[change.field]}${change.base === null ? "" : `, was ${shown(change.base)}`}`}</small>
        </span>
      </label>
      {conflict && <span className="merge-sides" role="radiogroup" aria-label={`Which ${LABELS[change.field].toLowerCase()} to keep for ${change.title}`}>
        <label><input type="radio" name={keyOf(change)} checked={side === "openCrate"} disabled={busy !== null || !review?.canWrite} onChange={() => choose(change, "openCrate")} />{`Keep openCrate's: ${shown(change.ours)}`}</label>
        <label><input type="radio" name={keyOf(change)} checked={side === "rekordbox"} disabled={busy !== null} onChange={() => choose(change, "rekordbox")} />{`Keep rekordbox's: ${shown(change.theirs)}`}</label>
      </span>}
    </li>;
  };
  const section = (direction: MergeChange["direction"], title: string, note: string) => {
    const items = group(direction);
    if (items.length === 0) return null;
    // Ticking a whole group: not for a direction that cannot be written, nor where sides must be picked.
    const pickable = direction === "conflict" || (direction === "toRekordbox" && !review?.canWrite) ? [] : items;
    return <fieldset className="merge-group">
      <legend>{`${title} (${items.length})`}</legend>
      <p className="settings-copy">{note}</p>
      {pickable.length > 0 && <div className="merge-actions">
        <button type="button" className="link-button" disabled={busy !== null} onClick={() => pickable.forEach((change) => choose(change, direction === "toRekordbox" ? "openCrate" : "rekordbox"))}>Tick all</button>
        <button type="button" className="link-button" disabled={busy !== null} onClick={() => pickable.forEach((change) => choose(change, null))}>Untick all</button>
      </div>}
      {direction === "conflict" && <div className="merge-actions">
        <button type="button" className="link-button" disabled={busy !== null} onClick={() => items.forEach((change) => choose(change, "rekordbox"))}>Keep rekordbox's for all</button>
        <button type="button" className="link-button" disabled={busy !== null || !review?.canWrite} title={review?.canWrite ? undefined : "Needs writing to rekordbox on in Settings"} onClick={() => items.forEach((change) => choose(change, "openCrate"))}>Keep openCrate's for all</button>
        <button type="button" className="link-button" disabled={busy !== null} onClick={() => items.forEach((change) => choose(change, null))}>Decide later</button>
      </div>}
      <ul className="folder-list">{items.slice(0, drawn[direction] ?? DRAWN).map(row)}</ul>
      {items.length > (drawn[direction] ?? DRAWN) && <p className="settings-copy"><button type="button" className="link-button" onClick={() => setDrawn((before) => ({ ...before, [direction]: (before[direction] ?? DRAWN) + DRAWN }))}>{`Show ${Math.min(DRAWN, items.length - (drawn[direction] ?? DRAWN))} more`}</button>{` ${items.length - (drawn[direction] ?? DRAWN)} of ${items.length} not shown yet; Tick all covers them too.`}</p>}
    </fieldset>;
  };

  return <section className="sync-card" role="region" aria-label="Sync with rekordbox">
    <h2>Sync with rekordbox</h2>
    <p className="settings-copy">Compares each track's title, artist, album, year and label in openCrate with rekordbox's, since the two last agreed. A change made on one side goes to the other; where both changed, you choose. Nothing happens until you tick it and sync.</p>
    <button className="secondary-button" disabled={busy !== null} onClick={() => { setApplied(null); void act(load, "reviewing"); }}>{busy === "reviewing" ? "Reading rekordbox..." : review ? "Review again" : "Review changes"}</button>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {applied && <div role="status" className="send-steps">
      <p className="settings-copy">{`${plural(applied.toOpencrate, "change", "changes")} came from rekordbox into your files' tags; ${plural(applied.toRekordbox, "change", "changes")} went to rekordbox.`}</p>
      {applied.skipped.length > 0 && <ul className="skipped-list">{applied.skipped.map((why) => <li key={why}>{why}</li>)}</ul>}
    </div>}
    {review && changes.length === 0 && <p className="settings-copy">openCrate and rekordbox agree on every track rekordbox has from your folders.</p>}
    {review && changes.length > 0 && <>
      {section("toOpenCrate", "rekordbox to openCrate", "Changed in rekordbox. Ticked values are written into the file's tags.")}
      {section("toRekordbox", "openCrate to rekordbox", review.canWrite
        ? "Changed in openCrate. Ticked values are written into rekordbox's library, after a backup."
        : "Changed in openCrate. Turn on writing to rekordbox in Settings to send these.")}
      {section("conflict", "Changed on both sides", review.canWrite
        ? "Both sides changed since they last agreed. Pick which value to keep; a change with no side picked is left for later."
        : "Both sides changed since they last agreed. Pick which value to keep. Keeping openCrate's needs writing to rekordbox on in Settings.")}
      <button className="import-button" disabled={busy !== null || ticked === 0} onClick={() => void sync()}>{busy === "syncing" ? "Syncing..." : `Sync ${plural(ticked, "change", "changes")}`}</button>
    </>}
  </section>;
}
