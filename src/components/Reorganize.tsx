import { useCallback, useEffect, useRef, useState } from "react";
import type { Source } from "../library/importEdits";
import type { EditField, LibraryAdmin, RefilePlan, ReorganizeSitting, TrackTags } from "../library/types";
import { EditableCell } from "./ImportGrid";
import { Icon } from "./Icon";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

/** How the tool behaves; kept in this browser. */
export interface ReorganizeSettings {
  /** Offer files rekordbox does not know about too. Off: only files rekordbox uses are moved. */
  includeUnknown: boolean;
  /** Go straight to the moves, with the tags as they are. */
  skipTags: boolean;
}
const SETTINGS_KEY = "opencrate.reorganize";
const FOLDED_KEY = "opencrate.reorganizeFolded";
const DEFAULT_SETTINGS: ReorganizeSettings = { includeUnknown: false, skipTags: false };

export function loadReorganizeSettings(): ReorganizeSettings {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "null");
    if (typeof saved === "object" && saved !== null) {
      const record = saved as Record<string, unknown>;
      return {
        includeUnknown: typeof record.includeUnknown === "boolean" ? record.includeUnknown : DEFAULT_SETTINGS.includeUnknown,
        skipTags: typeof record.skipTags === "boolean" ? record.skipTags : DEFAULT_SETTINGS.skipTags,
      };
    }
  } catch { /* unreadable: the defaults */ }
  return DEFAULT_SETTINGS;
}
function saveSettings(settings: ReorganizeSettings) {
  try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings)); } catch { /* not remembered, still applied */ }
}
function loadFolded(): boolean {
  try { return localStorage.getItem(FOLDED_KEY) !== "false"; } catch { return true; }
}

/** The tag columns, each one an editable cell. The ids are tag fields and grid columns alike. */
const FIELDS = [
  { id: "track", label: "#", placeholder: "-" },
  { id: "title", label: "Title", placeholder: "Title" },
  { id: "artist", label: "Artist", placeholder: "Artist" },
  { id: "album", label: "Album", placeholder: "Album" },
  { id: "albumArtist", label: "Album artist", placeholder: "Album artist" },
  { id: "genre", label: "Genre", placeholder: "Genre" },
  { id: "label", label: "Label", placeholder: "Label" },
  { id: "year", label: "Year", placeholder: "Year" },
] as const;
type Field = (typeof FIELDS)[number]["id"];
/** The lookup names a change by the field's shown name. */
const LOOKUP_FIELD: Record<string, Field> = {
  "Track": "track", "Title": "title", "Artist": "artist", "Album": "album", "Album artist": "albumArtist", "Genre": "genre", "Label": "label", "Year": "year",
};

/** How many tracks the tag step takes on at once, newest first. */
const TRACKS_AT_ONCE = 500;
/** Tracks per lookup call: the host asks MusicBrainz once per release, a second apart. */
const LOOKUP_CHUNK = 8;
/** Files moved per call, so Stop takes effect between lots and one failure loses little. */
const MOVE_CHUNK = 50;
/** Rows drawn at first. */
const DRAWN = 200;
/** Sittings the host lists, newest first. */
const SITTINGS_SHOWN = 20;

interface Row {
  tags: TrackTags;
  typed: Partial<Record<Field, string>>;
  lookedUp: Partial<Record<Field, string>>;
  /** What the lookup found, or why it found nothing. */
  found: string | null;
  busy: boolean;
}

const tagText = (tags: TrackTags, field: Field): string => {
  const value = tags[field];
  return value === null || value === undefined ? "" : String(value);
};
const shown = (row: Row, field: Field) => row.typed[field] ?? row.lookedUp[field] ?? tagText(row.tags, field);
const sourceOf = (row: Row, field: Field): Source =>
  row.typed[field] !== undefined ? "edited" : row.lookedUp[field] !== undefined ? "lookedUp" : shown(row, field) ? "tags" : "missing";
/** The changes a row carries: typed values first, then the lookup's, where they differ from the tags. */
function changesOf(row: Row): { field: EditField; value: string }[] {
  const changes: { field: EditField; value: string }[] = [];
  for (const { id } of FIELDS) {
    const value = row.typed[id] ?? row.lookedUp[id];
    if (value !== undefined && value !== tagText(row.tags, id)) changes.push({ field: id, value });
  }
  return changes;
}

interface Props {
  admin: LibraryAdmin;
  /** Tags or files changed: the library should be read again. */
  onChanged: () => void;
}

/** Reorganize library: an experimental tool that tags, then files, music already in the library by
 * the folder layout, in sittings that can be put back. Moves go through the Folder layout card's
 * host command with a sitting id, so each one is written down. */
export function Reorganize({ admin, onChanged }: Props) {
  const [folded, setFolded] = useState(loadFolded);
  const [settings, setSettings] = useState(loadReorganizeSettings);
  const [showSettings, setShowSettings] = useState(false);
  const [rows, setRows] = useState<Row[] | null>(null);
  const [rowsShown, setRowsShown] = useState(DRAWN);
  const [plan, setPlan] = useState<RefilePlan | null>(null);
  const [unticked, setUnticked] = useState<Set<number>>(new Set());
  const [sittings, setSittings] = useState<ReorganizeSitting[] | null>(null);
  /** The sitting whose Put back is asking first. */
  const [confirming, setConfirming] = useState<number | null>(null);
  const [busy, setBusy] = useState<"tags" | "looking" | "applying" | "checking" | "moving" | "undoing" | null>(null);
  const [progress, setProgress] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  /** Which step the note, error and progress belong under, so they show where the eye is. */
  const [where, setWhere] = useState<"tags" | "moves" | "undo">("tags");
  const stop = useRef(false);

  const loadSitting = useCallback(async () => {
    try {
      setSittings(await admin.reorganizeSittings());
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);
  useEffect(() => { if (!folded) void loadSitting(); }, [folded, loadSitting]);

  const fold = (next: boolean) => {
    setFolded(next);
    try { localStorage.setItem(FOLDED_KEY, String(next)); } catch { /* not remembered */ }
  };
  const update = (next: ReorganizeSettings) => { setSettings(next); saveSettings(next); };
  const stepOf = (what: typeof busy): typeof where => (what === "checking" || what === "moving" ? "moves" : what === "undoing" ? "undo" : "tags");
  const begin = (what: typeof busy) => { setBusy(what); setWhere(stepOf(what)); setError(""); setNote(""); setProgress(""); stop.current = false; };
  const said = (step: typeof where) => (where !== step ? null : <>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {progress && <p className="settings-copy reorganize-progress" aria-live="polite">{progress} <button className="link-button" onClick={() => { stop.current = true; }}>Stop</button></p>}
  </>);

  const updateRow = (trackId: number, change: (row: Row) => Row) =>
    setRows((before) => before && before.map((row) => (row.tags.trackId === trackId ? change(row) : row)));

  const readTags = async () => {
    begin("tags");
    try {
      const ids = await admin.thinTracks(TRACKS_AT_ONCE);
      const tags = await admin.trackTags(ids);
      setRows(tags.map((item) => ({ tags: item, typed: {}, lookedUp: {}, found: null, busy: false })));
      setRowsShown(DRAWN);
      if (tags.length === 0) setNote("Every track has a title, artist, album, genre, label and year. Nothing to fill in.");
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };

  /** Ask the host about these tracks and keep what it suggests, under anything typed. */
  const lookUp = async (trackIds: number[]) => {
    for (const id of trackIds) updateRow(id, (row) => ({ ...row, busy: true }));
    try {
      const results = await admin.lookupTracks(trackIds);
      for (const result of results) {
        updateRow(result.trackId, (row) => {
          const lookedUp: Partial<Record<Field, string>> = {};
          for (const change of result.changes) {
            const field = LOOKUP_FIELD[change.field];
            if (field) lookedUp[field] = change.to;
          }
          const found = result.found
            ? `${result.found.source}: ${result.found.title} by ${result.found.artist}${result.matchedBy ? `, matched by ${result.matchedBy}` : ", not matched to a track"}`
            : result.note ?? "Nothing found.";
          return { ...row, lookedUp, found, busy: false };
        });
      }
    } finally {
      for (const id of trackIds) updateRow(id, (row) => ({ ...row, busy: false }));
    }
  };
  const lookUpOne = async (trackId: number) => {
    setError("");
    setWhere("tags");
    try {
      await lookUp([trackId]);
    } catch (reason) {
      setError(message(reason));
    }
  };
  const lookUpAll = async () => {
    if (!rows) return;
    begin("looking");
    const ids = rows.map((row) => row.tags.trackId);
    let done = 0;
    try {
      for (let at = 0; at < ids.length && !stop.current; at += LOOKUP_CHUNK) {
        const chunk = ids.slice(at, at + LOOKUP_CHUNK);
        setProgress(`Looking up ${plural(done + chunk.length, "track", "tracks")} of ${ids.length}...`);
        await lookUp(chunk);
        done += chunk.length;
      }
      setNote(`${plural(done, "track was", "tracks were")} looked up${stop.current ? "; stopped" : ""}. Suggestions are marked; type over any you do not want, then apply.`);
    } catch (reason) {
      setError(`${message(reason)} ${plural(done, "track was", "tracks were")} looked up before that.`);
    } finally {
      setProgress("");
      setBusy(null);
    }
  };
  const pending = rows ? rows.filter((row) => changesOf(row).length > 0) : [];
  const apply = async () => {
    if (pending.length === 0) return;
    begin("applying");
    try {
      const edits = pending.map((row) => ({ trackId: row.tags.trackId, changes: changesOf(row) }));
      const done = await admin.editEach(edits);
      const tags = await admin.trackTags(edits.map((edit) => edit.trackId));
      const fresh = new Map(tags.map((item) => [item.trackId, item]));
      setRows((before) => before && before.map((row) => {
        const now = fresh.get(row.tags.trackId);
        return now ? { ...row, tags: now, typed: {}, lookedUp: {} } : row;
      }));
      setNote(`${plural(done.files, "file was", "files were")} written.${done.left.length > 0 ? ` Left as they were: ${done.left.join(" ")}` : ""}`);
      onChanged();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };

  const check = async () => {
    begin("checking");
    try {
      setPlan(await admin.refilePlan());
      setUnticked(new Set());
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };
  const offered = plan ? plan.moves.filter((item) => settings.includeUnknown || item.inRekordbox) : [];
  const leftOutUnknown = plan ? plan.moves.length - offered.length : 0;
  const ticked = offered.filter((item) => !unticked.has(item.fileId));
  const toggle = (fileId: number) => setUnticked((before) => {
    const next = new Set(before);
    if (!next.delete(fileId)) next.add(fileId);
    return next;
  });
  const move = async () => {
    if (ticked.length === 0) return;
    begin("moving");
    const ids = ticked.map((item) => item.fileId);
    let moved = 0;
    let relocated = 0;
    const left: string[] = [];
    try {
      const session = await admin.reorganizeBegin();
      for (let at = 0; at < ids.length && !stop.current; at += MOVE_CHUNK) {
        const chunk = ids.slice(at, at + MOVE_CHUNK);
        setProgress(`Moving ${plural(moved + chunk.length, "file", "files")} of ${ids.length}...`);
        const done = await admin.refile(chunk, session);
        moved += done.moved;
        relocated += done.relocated;
        left.push(...done.left);
      }
      setNote(`${plural(moved, "file was", "files were")} moved${relocated > 0 ? `; rekordbox was told where ${plural(relocated, "file", "files")} went` : ""}${stop.current ? "; stopped" : ""}.${left.length > 0 ? ` Left where they are: ${left.join(" ")}` : ""}`);
    } catch (reason) {
      setError(`${message(reason)} ${plural(moved, "file was", "files were")} moved before that; the sitting can be put back below.`);
    } finally {
      setProgress("");
      setBusy(null);
      onChanged();
      await loadSitting();
      try {
        setPlan(await admin.refilePlan());
        setUnticked(new Set());
      } catch (reason) {
        setError(message(reason));
      }
    }
  };
  const putBack = async (sitting: ReorganizeSitting) => {
    setConfirming(null);
    begin("undoing");
    try {
      const done = await admin.reorganizeUndo(sitting.id);
      setNote(`${done.putBack === 0 ? "No file was put back" : `${plural(done.putBack, "file was", "files were")} put back`}${done.relocated > 0 ? `; rekordbox was told where ${plural(done.relocated, "file", "files")} went` : ""}.${done.left.length > 0 ? ` ${done.left.join(" ")}` : ""}`);
      onChanged();
      await loadSitting();
      if (plan) setPlan(await admin.refilePlan());
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };

  const when = (ms: number) => new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

  return <section className="sync-card reorganize" role="region" aria-label="Reorganize library (experimental)">
    <div className="reorganize-head">
      <h2>Reorganize library <span className="experimental-badge">experimental, use at your own risk</span></h2>
      <button className="link-button" aria-expanded={!folded} onClick={() => fold(!folded)}>{folded ? "Show the tool" : "Hide the tool"}</button>
    </div>
    <p className="settings-copy">For a library that grew without a plan: fill in the tags, then move the files into the folder layout from Settings, and tell rekordbox where they went. It moves your files. Read the warnings before you start.</p>
    {!folded && <>
      <ul className="check-list reorganize-warnings" aria-label="Before you start">
        <li>Back up your music and your rekordbox library first. The last {SITTINGS_SHOWN} sittings are listed below and each can be put back; a backup covers everything else.</li>
        <li>Close rekordbox and turn on reading and writing its library in Settings. Files rekordbox uses are moved only when it can be told.</li>
        <li>Other software keeps its own paths. Serato, Traktor, playlists saved as M3U files and anything else that remembers where a file is will not follow the moves.</li>
        <li>Everything happens on this computer's view of the folders. A music folder on a network drive should be mounted the way rekordbox sees it.</li>
      </ul>
      <div className="merge-actions reorganize-settings-row">
        <button className="icon-button" aria-label="Reorganize settings" aria-expanded={showSettings} title="Choose which files are offered and whether to skip the tags" onClick={() => setShowSettings(!showSettings)}><Icon name="settings" size={14} /></button>
        <small className="settings-copy">{settings.includeUnknown ? "Files rekordbox does not know are offered too." : "Only files rekordbox uses are offered."}{settings.skipTags ? " The tag step is skipped." : ""}</small>
      </div>
      {showSettings && <div className="reorganize-settings" role="group" aria-label="Reorganize settings">
        <label className="send-row"><input type="checkbox" checked={settings.includeUnknown} onChange={(event) => update({ ...settings, includeUnknown: event.target.checked })} /><span>Offer files rekordbox does not know about too</span></label>
        <small className="settings-copy">Off, only files in rekordbox's library are moved, so nothing rekordbox cannot be told about changes place. On, every file the folder layout would file differently is offered.</small>
        <label className="send-row"><input type="checkbox" checked={settings.skipTags} onChange={(event) => update({ ...settings, skipTags: event.target.checked })} /><span>Skip the tag step and move files with the tags they have</span></label>
        <small className="settings-copy">The layout files each track by its tags. A thin tag means a thin folder name, so filling them in first is the sane order.</small>
      </div>}
      {where === "tags" && settings.skipTags && said("tags")}

      {!settings.skipTags && <div className="reorganize-step">
        <h3>1. Tags</h3>
        <p className="settings-copy">Tracks whose tags lack a title, artist, album, genre, label or year, newest first, {TRACKS_AT_ONCE} at a time. Look a track up to have MusicBrainz suggest the rest, or type it. Nothing is written until you apply.</p>
        <div className="merge-actions">
          <button className="secondary-button" disabled={busy !== null} onClick={() => void readTags()}>{busy === "tags" ? "Reading tags..." : rows ? "Read tags again" : "Read tags"}</button>
          {rows && rows.length > 0 && <button className="secondary-button" disabled={busy !== null} onClick={() => void lookUpAll()}>{busy === "looking" ? "Looking up..." : `Look up all ${rows.length}`}</button>}
          {rows && rows.length > 0 && <button className="import-button" disabled={busy !== null || pending.length === 0} onClick={() => void apply()}>{busy === "applying" ? "Writing..." : `Apply changes to ${plural(pending.length, "track", "tracks")}`}</button>}
        </div>
        {said("tags")}
        {rows && rows.length > 0 && <div className="plan-table-wrap">
          <table className="analysis-table plan-table import-grid reorganize-grid" aria-label="Tracks to tag">
            <colgroup><col className="is-state" /><col className="is-file" />{FIELDS.map((field) => <col key={field.id} className={`is-${field.id}`} />)}<col className="is-found" /></colgroup>
            <thead><tr><th aria-label="Actions" className="reorganize-action-head" /><th>Track</th>{FIELDS.map((field) => <th key={field.id}>{field.label}</th>)}<th>Found</th></tr></thead>
            <tbody>
              {rows.slice(0, rowsShown).map((row) => <tr key={row.tags.trackId} className="plan-row">
                <td className="plan-state">
                  <button className="icon-button grid-action" disabled={row.busy || busy !== null} aria-label={`Look up track ${row.tags.trackId}`} title={row.busy ? "Looking up..." : "Look this track up on MusicBrainz"} onClick={() => void lookUpOne(row.tags.trackId)}>{row.busy ? <span aria-hidden="true">...</span> : <Icon name="search" size={13} />}</button>
                </td>
                <td className="plan-cell"><span className="grid-static" title={`Track ${row.tags.trackId}`}>{row.tags.title || row.tags.artist ? `${row.tags.artist ?? "?"} - ${row.tags.title ?? "?"}` : `Track ${row.tags.trackId}`}</span></td>
                {FIELDS.map((field) => <td key={field.id} className="plan-cell">
                  <EditableCell
                    column={field.id}
                    label={`${field.label} of track ${row.tags.trackId}`}
                    value={shown(row, field.id)}
                    source={sourceOf(row, field.id)}
                    placeholder={field.placeholder}
                    disabled={busy === "applying"}
                    onCommit={(text) => updateRow(row.tags.trackId, (current) => {
                      const typed = { ...current.typed };
                      if (text === tagText(current.tags, field.id) && current.lookedUp[field.id] === undefined) delete typed[field.id]; else typed[field.id] = text;
                      return { ...current, typed };
                    })}
                  />
                </td>)}
                <td className="plan-cell plan-destination"><span className="grid-static" title={row.found ?? ""}>{row.found ?? ""}</span></td>
              </tr>)}
            </tbody>
          </table>
        </div>}
        {rows && rows.length > rowsShown && <p className="settings-copy"><button className="link-button" onClick={() => setRowsShown((count) => count + DRAWN)}>{`Show ${Math.min(DRAWN, rows.length - rowsShown)} more`}</button>{` ${rows.length - rowsShown} of ${rows.length} not shown yet.`}</p>}
      </div>}

      <div className="reorganize-step">
        <h3>{settings.skipTags ? "1. Moves" : "2. Moves"}</h3>
        <p className="settings-copy">Checks every file against the folder layout and lists the ones filed another way. Tick what should move. Files go in lots of {MOVE_CHUNK}; Stop ends the sitting after the current lot. A file is never moved over another.</p>
        <div className="merge-actions">
          <button className="secondary-button" disabled={busy !== null} onClick={() => void check()}>{busy === "checking" ? "Reading tags..." : plan ? "Check again" : "Check the folder layout"}</button>
        </div>
        {said("moves")}
        {plan && <p className="settings-copy">{`Layout: ${plan.layout}. ${plural(plan.inPlace, "file is", "files are")} already in place; ${plural(offered.length, "file is", "files are")} offered${leftOutUnknown > 0 ? `; ${plural(leftOutUnknown, "file", "files")} rekordbox does not know ${leftOutUnknown === 1 ? "is" : "are"} left out (see the settings above)` : ""}${plan.skipped.length > 0 ? `; ${plural(plan.skipped.length, "file", "files")} cannot be placed from ${plan.skipped.length === 1 ? "its" : "their"} tags` : ""}.`}</p>}
        {plan && offered.length > 0 && <>
          <div className="merge-actions">
            <button className="link-button" disabled={busy !== null} onClick={() => setUnticked(new Set())}>Tick all</button>
            <button className="link-button" disabled={busy !== null} onClick={() => setUnticked(new Set(offered.map((item) => item.fileId)))}>Untick all</button>
          </div>
          <ul className="folder-list" aria-label="Files to reorganize">{offered.slice(0, DRAWN).map((item) => <li key={item.fileId} className="merge-row">
            <label className="send-row">
              <input type="checkbox" checked={!unticked.has(item.fileId)} onChange={() => toggle(item.fileId)} aria-label={item.from} />
              <span className="folder-text">
                <small>{`From: ${item.rootLabel}: ${item.from}`}</small>
                <small>{`To: ${item.to}${item.inRekordbox ? " (rekordbox uses it)" : ""}`}</small>
              </span>
            </label>
          </li>)}</ul>
          {offered.length > DRAWN && <p className="settings-copy">{`${offered.length - DRAWN} more not shown; ticked ones count all the same.`}</p>}
          <button className="import-button" disabled={busy !== null || ticked.length === 0} onClick={() => void move()}>{busy === "moving" ? "Moving..." : `Move ${plural(ticked.length, "file", "files")}`}</button>
        </>}
        {plan && plan.skipped.length > 0 && <ul className="check-list" aria-label="Files that cannot be placed">{plan.skipped.slice(0, DRAWN).map((item) => <li key={item.path}><strong>{item.path}</strong> <small>{item.why}</small></li>)}</ul>}
      </div>

      <div className="reorganize-step">
        <h3>{settings.skipTags ? "2. Put back" : "3. Put back"}</h3>
        {said("undo")}
        {sittings === null || sittings.length === 0
          ? <p className="settings-copy">No sitting yet. Each press of Move is a sitting, and its moves can be put back here.</p>
          : <>
            <p className="settings-copy">Putting a sitting back moves each of its files to where it was, newest move first, and tells rekordbox. Any sitting can go back, in any order. A file a later sitting moved again stays until that sitting is put back, so going newest first undoes everything; a sitting left partly put back can be finished later. A file moved since by hand stays.</p>
            <ul className="folder-list" aria-label="Sittings">{sittings.map((sitting) => {
              const left = sitting.moves - sitting.putBack;
              const state = sitting.moves === 0 ? "moved nothing"
                : sitting.undone ? `moved ${plural(sitting.moves, "file", "files")}, all put back`
                  : sitting.putBack > 0 ? `moved ${plural(sitting.moves, "file", "files")}, ${sitting.putBack} put back`
                    : `moved ${plural(sitting.moves, "file", "files")}`;
              const more = sitting.moves - sitting.names.length;
              const files = sitting.names.length === 0 ? "" : `${sitting.names.join(", ")}${more > 0 ? ` and ${more} more` : ""}`;
              return <li key={sitting.id} className="merge-row sitting-row" aria-label={`Sitting of ${when(sitting.startedMs)}`}>
                <span className="folder-text"><strong>{when(sitting.startedMs)}</strong><small>{state}</small>{files && <small title={files}>{files}</small>}</span>
                {!sitting.undone && left > 0 && (confirming === sitting.id
                  ? <div className="crate-confirm" role="alertdialog" aria-label="Put this sitting back?">
                    <span>{`Put ${plural(left, "file", "files")} back where ${left === 1 ? "it was" : "they were"}?`}</span>
                    <button className="link-button" onClick={() => void putBack(sitting)}>Put them back</button>
                    <button className="link-button" autoFocus onClick={() => setConfirming(null)}>Cancel</button>
                  </div>
                  : <button className="secondary-button" disabled={busy !== null} onClick={() => setConfirming(sitting.id)}>{sitting.putBack > 0 ? `Put back the other ${plural(left, "file", "files")}` : `Put back ${plural(left, "file", "files")}`}</button>)}
              </li>;
            })}</ul>
          </>}
      </div>
    </>}
  </section>;
}
