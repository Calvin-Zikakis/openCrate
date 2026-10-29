import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { onlyCasing, proposeFixes, type ProposedFix } from "../library/cleanup";
import type { CrateComparison, CrateInfo, EditBatch, FolderSearch, Genres, LibraryAdmin, MissingArt, MovedFiles, RefilePlan, Relinked, Sort } from "../library/types";
import { Icon } from "./Icon";
import { PLAIN_TEXT } from "./plainText";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
/** Files a save left alone, grouped by why: "a.aif, b.aif: its embedded picture is damaged...".
 * Each reason comes as "path: why"; the folder is dropped to keep the note short. */
export function leftOut(left: string[]): string {
  if (left.length === 0) return "";
  const byReason = new Map<string, string[]>();
  for (const item of left) {
    // "path: why": the path ends at its audio file's extension, as a name may hold ": " too.
    const named = /^.*?\.(?:aiff?|wav|mp3|flac|m4a|aac|ogg|opus|alac): /i.exec(item);
    const at = named ? named[0].length - 2 : item.indexOf(": ");
    const path = at < 0 ? "" : item.slice(0, at);
    const why = at < 0 ? item : item.slice(at + 2);
    const name = path.split(/[\\/]/).pop() ?? path;
    byReason.set(why, [...(byReason.get(why) ?? []), name]);
  }
  const groups = Array.from(byReason, ([why, all]) => {
    // Copies of one track in two music folders share a name; list it once.
    const names = Array.from(new Set(all), (name) => {
      const copies = all.filter((other) => other === name).length;
      return copies > 1 ? `${name} (${copies} copies)` : name;
    });
    const shown = names.slice(0, 3).join(", ");
    return `${shown}${names.length > 3 ? ` and ${names.length - 3} more` : ""}: ${why}.`;
  });
  return ` ${plural(left.length, "file was", "files were")} left as they were. ${groups.join(" ")}`;
}

/** Rows drawn at first; Show more draws the next lot, so every row can be seen and unticked. */
const DRAWN = 300;

/** The first `shown` rows of a long list, with a link for the next lot. */
function useDrawn(total: number) {
  const [shown, setShown] = useState(DRAWN);
  const more = total > shown
    ? <p className="settings-copy"><button className="link-button" onClick={() => setShown((count) => count + DRAWN)}>{`Show ${Math.min(DRAWN, total - shown)} more`}</button>{` ${total - shown} of ${total} not shown yet; ticked ones count all the same.`}</p>
    : null;
  return { shown, more };
}

interface Props {
  admin: LibraryAdmin;
  /** Files changed: the library should be read again. */
  onChanged: () => void;
}

/** Fixes across the whole library. Nothing changes until it is chosen; changes go into the files'
 * tags, and the next sync with rekordbox offers them to rekordbox. */
export function CleanUp({ admin, onChanged }: Props) {
  // Each save here is a new entry in Recent changes, so that list is read again after one.
  const [saves, setSaves] = useState(0);
  const changed = useCallback(() => { setSaves((count) => count + 1); onChanged(); }, [onChanged]);
  return <div className="sync-screen">
    <div className="feature-empty sync-hero">
      <div className="empty-icon"><Icon name="clean" size={36} /></div>
      <h1>Clean up</h1>
      <p>Fix what is untidy across your whole library. Nothing changes until you choose; changes go into the files' tags, and the next Sync with rekordbox offers them to rekordbox.</p>
    </div>
    <HistoryCard admin={admin} onChanged={changed} saves={saves} />
    <MovedCard admin={admin} onChanged={onChanged} />
    <GenresCard admin={admin} onChanged={changed} saves={saves} />
    <TitlesCard admin={admin} onChanged={changed} />
    <ArtCard admin={admin} onChanged={changed} />
    <CratesCard admin={admin} onChanged={changed} />
    <RefileCard admin={admin} onChanged={onChanged} />
  </div>;
}

/** What a relink did, in a sentence. */
function relinkedNote(done: Relinked): string {
  const parts = [done.relinked === 0 ? "Nothing was relinked." : `${plural(done.relinked, "track was", "tracks were")} relinked${done.relocated > 0 ? `; rekordbox was told the new place of ${plural(done.relocated, "file", "files")}` : ""}.`];
  if (done.left.length > 0) parts.push(`Left as ${done.left.length === 1 ? "it was" : "they were"}: ${done.left.join(" ")}`);
  return parts.join(" ");
}

function MovedCard({ admin, onChanged }: Props) {
  const [found, setFound] = useState<MovedFiles | null>(null);
  const [folded, setFolded] = useState(false);
  /** Pairs left out of Relink; a guess by name and size starts left out. */
  const [unticked, setUnticked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const look = async () => {
    const next = await admin.movedFiles();
    setFound(next);
    setUnticked(new Set(next.moved.filter((item) => !item.sameFile).map((item) => item.missing.fileId)));
  };

  const check = async () => {
    setBusy(true);
    setError("");
    setNote("");
    try {
      await look();
      setFolded(false);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  /** Relink, then look again: what is left is what still needs a hand. */
  const act = async (run: () => Promise<Relinked | null>) => {
    setBusy(true);
    setError("");
    setNote("");
    try {
      const done = await run();
      if (!done) return;
      setNote(relinkedNote(done));
      if (done.relinked > 0) onChanged();
      await look();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  /** The last look in a folder the owner picked, and the finds left out of the copy. */
  const [search, setSearch] = useState<FolderSearch | null>(null);
  const [searchUnticked, setSearchUnticked] = useState<Set<number>>(new Set());
  const searchFolder = async () => {
    setBusy(true);
    setError("");
    setNote("");
    try {
      const next = await admin.findMissingInFolder();
      if (!next) return;
      setSearch(next);
      setSearchUnticked(new Set(next.found.filter((item) => !item.sameSize).map((item) => item.missing.fileId)));
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const restore = async () => {
    if (!search) return;
    const chosen = search.found.filter((item) => !searchUnticked.has(item.missing.fileId));
    setBusy(true);
    setError("");
    setNote("");
    try {
      const done = await admin.restoreMissing(chosen.map((item) => [item.missing.fileId, item.path]));
      setNote(`${plural(done.restored, "file was", "files were")} copied back into place.${done.left.length > 0 ? ` Left as ${done.left.length === 1 ? "it was" : "they were"}: ${done.left.join(" ")}` : ""}`);
      if (done.restored > 0) onChanged();
      setSearch(null);
      await look();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const total = found ? found.moved.length + found.unmatched.length : 0;
  const movedDrawn = useDrawn(found?.moved.length ?? 0);
  const unmatchedDrawn = useDrawn(found?.unmatched.length ?? 0);
  const searchDrawn = useDrawn(search?.found.length ?? 0);

  return <section className="sync-card" role="region" aria-label="Moved files">
    <h2>Moved files</h2>
    <p className="settings-copy">Finds tracks whose file is missing because it was moved or renamed outside openCrate, and the file it most likely is now: the same file on disk, or one of the same name and size that appeared since. Relinking keeps the track's crates, your fields and its date added. If rekordbox uses the file, it is told the new place too, which needs writing to rekordbox on.</p>
    <div className="merge-actions">
      <button className="secondary-button" disabled={busy} onClick={() => void check()}>{busy ? "Looking..." : found ? "Check again" : "Find moved files"}</button>
      {found && total > 0 && <button className="link-button" aria-expanded={!folded} onClick={() => setFolded(!folded)}>{folded ? `Show the list (${total})` : "Hide the list"}</button>}
    </div>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {found && total === 0 && <p className="settings-copy">No track is missing its file.</p>}
    {found && total > 0 && !folded && <>
      {found.moved.length > 0 && <>
        <p className="settings-copy">{`${plural(found.moved.length, "missing file was", "missing files were")} found somewhere else in your music folders.`}</p>
        {found.moved.some((item) => !item.sameFile) && <p className="settings-copy">A match by name and size alone starts unticked: check it before relinking.</p>}
        {found.moved.length > 1 && <div className="merge-actions">
          <button className="link-button" disabled={busy} onClick={() => setUnticked(new Set())}>Tick all</button>
          <button className="link-button" disabled={busy} onClick={() => setUnticked(new Set(found.moved.map((item) => item.missing.fileId)))}>Untick all</button>
        </div>}
        <ul className="folder-list" aria-label="Found elsewhere">{found.moved.slice(0, movedDrawn.shown).map((item) => <li key={item.missing.fileId} className="folder-row">
          <label className="send-row">
            <input type="checkbox" checked={!unticked.has(item.missing.fileId)} aria-label={`Relink ${item.missing.title}`} onChange={() => setUnticked((before) => {
              const next = new Set(before);
              if (!next.delete(item.missing.fileId)) next.add(item.missing.fileId);
              return next;
            })} />
            <span className="folder-text">
              <strong>{`${item.missing.title || "Untitled"} - ${item.missing.artist}`}</strong>
              <small>{`Was: ${item.missing.path}`}</small>
              <small>{`Now: ${item.found.path} (${item.sameFile ? "the same file, moved or renamed" : "same name and size"})`}</small>
            </span>
          </label>
        </li>)}</ul>
        {movedDrawn.more}
        {(() => {
          const chosen = found.moved.filter((item) => !unticked.has(item.missing.fileId));
          return <button className="import-button" disabled={busy || chosen.length === 0} onClick={() => void act(() => admin.relink(chosen.map((item) => [item.missing.fileId, item.found.fileId])))}>{busy ? "Relinking..." : `Relink ${plural(chosen.length, "track", "tracks")}`}</button>;
        })()}
      </>}
      {found.unmatched.length > 0 && <>
        <p className="settings-copy">{`${plural(found.unmatched.length, "missing file was", "missing files were")} not found in your music folders.`} Point at a folder that may hold them (a backup drive, an old library) and copies are put back where the library expects them, so the tracks keep everything. Or choose one file at a time; a chosen file has to be in one of your music folders.</p>
        <div className="merge-actions">
          <button className="secondary-button" disabled={busy} onClick={() => void searchFolder()}>{busy ? "Looking..." : "Look in a folder..."}</button>
        </div>
        {search && <div className="import-review" role="region" aria-label="Found in the folder">
          <p className="settings-copy">{search.found.length === 0
            ? `Nothing of theirs in ${search.folder} (${plural(search.searched, "audio file", "audio files")} looked at).`
            : `${plural(search.found.length, "missing file has", "missing files have")} a copy in ${search.folder} (${plural(search.searched, "audio file", "audio files")} looked at${search.notFound > 0 ? `; ${search.notFound} not there` : ""}).`}
            {search.found.some((item) => !item.sameSize) ? " A copy of the same name but a different size starts unticked: check it first." : ""}</p>
          {search.found.length > 0 && <>
            <ul className="folder-list" aria-label="Copies found">{search.found.slice(0, searchDrawn.shown).map((item) => <li key={item.missing.fileId} className="folder-row">
              <label className="send-row">
                <input type="checkbox" checked={!searchUnticked.has(item.missing.fileId)} onChange={() => setSearchUnticked((before) => { const next = new Set(before); if (!next.delete(item.missing.fileId)) next.add(item.missing.fileId); return next; })} />
                <span className="folder-text">
                  <strong>{`${item.missing.title || "Untitled"} - ${item.missing.artist}`}</strong>
                  <small>{`Found: ${item.path} (${item.sameSize ? "same name and size" : "same name, different size"}${item.others > 0 ? `; ${plural(item.others, "other file", "other files")} of that name left out` : ""})`}</small>
                  <small>{`Goes back to: ${item.missing.path}`}</small>
                </span>
              </label>
            </li>)}</ul>
            {searchDrawn.more}
            <div className="review-actions">
              <button className="import-button" disabled={busy || search.found.every((item) => searchUnticked.has(item.missing.fileId))} onClick={() => void restore()}>{busy ? "Copying..." : `Copy ${plural(search.found.filter((item) => !searchUnticked.has(item.missing.fileId)).length, "file", "files")} back into place`}</button>
              <button className="secondary-button" disabled={busy} onClick={() => setSearch(null)}>Cancel</button>
            </div>
          </>}
        </div>}
        <ul className="folder-list" aria-label="Not found">{found.unmatched.slice(0, unmatchedDrawn.shown).map((file) => <li key={file.fileId} className="folder-row">
          <div className="folder-text">
            <strong>{`${file.title || "Untitled"} - ${file.artist}`}</strong>
            <small>{`Was: ${file.path}`}</small>
          </div>
          <button className="secondary-button" disabled={busy} aria-label={`Choose the file for ${file.title}`} onClick={() => void act(() => admin.relinkChosen(file.fileId))}>Choose file...</button>
        </li>)}</ul>
        {unmatchedDrawn.more}
      </>}
    </>}
  </section>;
}

function RefileCard({ admin, onChanged }: Props) {
  const [plan, setPlan] = useState<RefilePlan | null>(null);
  const movesDrawn = useDrawn(plan?.moves.length ?? 0);
  const skippedDrawn = useDrawn(plan?.skipped.length ?? 0);
  const [unticked, setUnticked] = useState<Set<number>>(new Set());
  const [folded, setFolded] = useState(false);
  const [busy, setBusy] = useState<"checking" | "moving" | null>(null);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const check = async () => {
    setBusy("checking");
    setError("");
    setNote("");
    try {
      setPlan(await admin.refilePlan());
      setUnticked(new Set());
      setFolded(false);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };
  const move = async () => {
    if (!plan) return;
    const chosen = plan.moves.filter((item) => !unticked.has(item.fileId)).map((item) => item.fileId);
    setBusy("moving");
    setError("");
    setNote("");
    try {
      const done = await admin.refile(chosen);
      setNote(`${plural(done.moved, "file was", "files were")} moved${done.relocated > 0 ? `; rekordbox was told where ${plural(done.relocated, "file", "files")} went` : ""}.${done.left.length > 0 ? ` Left where they are: ${done.left.join(" ")}` : ""}`);
      onChanged();
      setPlan(await admin.refilePlan());
      setUnticked(new Set());
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };
  const ticked = plan ? plan.moves.length - unticked.size : 0;
  const toggle = (fileId: number) => setUnticked((before) => {
    const next = new Set(before);
    if (!next.delete(fileId)) next.add(fileId);
    return next;
  });

  return <section className="sync-card" role="region" aria-label="Folder layout">
    <h2>Folder layout</h2>
    <p className="settings-copy">The folder layout in Settings decides where imports go. This finds music already in your folders that is filed some other way, works out each file's place from its own tags, and moves the files you tick. A file is never moved over another. If rekordbox uses a file, it is told where it went, which needs reading and writing rekordbox on. Checking reads every file's tags, so it can take a while.</p>
    <div className="merge-actions">
      <button className="secondary-button" disabled={busy !== null} onClick={() => void check()}>{busy === "checking" ? "Reading tags..." : plan ? "Check again" : "Check against the layout"}</button>
      {plan && plan.moves.length + plan.skipped.length > 0 && <button className="link-button" aria-expanded={!folded} onClick={() => setFolded(!folded)}>{folded ? `Show the list (${plan.moves.length + plan.skipped.length})` : "Hide the list"}</button>}
    </div>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {plan && <p className="settings-copy">{`Layout: ${plan.layout}. ${plural(plan.inPlace, "file is", "files are")} already in place${plan.moves.length > 0 ? `; ${plural(plan.moves.length, "file is", "files are")} filed another way` : ""}.`}</p>}
    {plan && !folded && plan.moves.length > 0 && <>
      <div className="merge-actions">
        <button className="link-button" disabled={busy !== null} onClick={() => setUnticked(new Set())}>Tick all</button>
        <button className="link-button" disabled={busy !== null} onClick={() => setUnticked(new Set(plan.moves.map((item) => item.fileId)))}>Untick all</button>
      </div>
      <ul className="folder-list" aria-label="Files to move">{plan.moves.slice(0, movesDrawn.shown).map((item) => <li key={item.fileId} className="merge-row">
        <label className="send-row">
          <input type="checkbox" checked={!unticked.has(item.fileId)} onChange={() => toggle(item.fileId)} aria-label={item.from} />
          <span className="folder-text">
            <small>{`From: ${item.rootLabel}: ${item.from}`}</small>
            <small>{`To: ${item.to}${item.inRekordbox ? " (rekordbox uses it)" : ""}`}</small>
          </span>
        </label>
      </li>)}</ul>
      {movesDrawn.more}
      <button className="import-button" disabled={busy !== null || ticked === 0} onClick={() => void move()}>{busy === "moving" ? "Moving..." : `Move ${plural(ticked, "file", "files")}`}</button>
    </>}
    {plan && !folded && plan.skipped.length > 0 && <>
      <p className="settings-copy">{`${plural(plan.skipped.length, "file stays", "files stay")} where ${plan.skipped.length === 1 ? "it is" : "they are"}:`}</p>
      <ul className="check-list" aria-label="Files left where they are">{plan.skipped.slice(0, skippedDrawn.shown).map((item) => <li key={item.path}><strong>{item.path}</strong> <small>{item.why}</small></li>)}</ul>
    </>}
  </section>;
}

function GenresCard({ admin, onChanged, saves }: Props & { saves: number }) {
  const [genres, setGenres] = useState<Genres | null>(null);
  const [names, setNames] = useState<Record<string, string>>({});
  const [showAll, setShowAll] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      setGenres(await admin.genres());
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);
  // Read again after any save here, an Undo included, so the spellings shown are the files' own.
  useEffect(() => { void load(); }, [load, saves]);

  const rename = async (from: string[], to: string) => {
    if (!to.trim()) return;
    setBusy(true);
    setError("");
    try {
      setNote("");
      const done = await admin.renameGenres(from, to.trim());
      setNote(`${done.files === 0 ? "No file changed." : `${plural(done.files, "file now has", "files now have")} the genre ${to.trim()}.`}${leftOut(done.left)}`);
      setNames({});
      await load();
      onChanged();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  /** Merge every group into the spelling its box shows, one group after another. Each merge is
   * its own entry in Recent changes, so any one can be undone. */
  const [progress, setProgress] = useState("");
  const stopAsked = useRef(false);
  const mergeAll = async () => {
    if (!genres) return;
    setBusy(true);
    setError("");
    setNote("");
    stopAsked.current = false;
    let files = 0;
    let merged = 0;
    let tried = 0;
    const left: string[] = [];
    try {
      for (const [index, group] of genres.groups.entries()) {
        if (stopAsked.current) break;
        tried += 1;
        const to = (names[group.members.join("|")] ?? group.suggested).trim();
        if (!to) continue;
        setProgress(`Merging group ${index + 1} of ${genres.groups.length}: ${to}...`);
        const done = await admin.renameGenres(group.members.filter((member) => member !== to), to);
        files += done.files;
        merged += 1;
        left.push(...done.left);
      }
      setNote(`${plural(merged, "group", "groups")} merged; ${plural(files, "file has", "files have")} a new genre.${tried < genres.groups.length ? ` Stopped with ${plural(genres.groups.length - tried, "group", "groups")} not merged.` : ""}${leftOut(left)}`);
      setNames({});
      await load();
      onChanged();
    } catch (reason) {
      setError(`${merged > 0 ? `${plural(merged, "group was", "groups were")} merged before this. ` : ""}${message(reason)}`);
      await load();
    } finally {
      setProgress("");
      setBusy(false);
    }
  };

  return <section className="sync-card" role="region" aria-label="Genres">
    <h2>Genres</h2>
    <p className="settings-copy">Genres that differ only in case, spaces or punctuation are grouped, with the commonest spelling suggested. Merge a group into one spelling, or rename any genre.</p>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {busy && <p role="status" className="settings-copy">{progress || "Writing the new genre into the files..."}{progress && <> <button className="link-button" onClick={() => { stopAsked.current = true; }}>Stop after this one</button></>}</p>}
    {genres && genres.groups.length === 0 && <p className="settings-copy">{genres.genres.length === 0 ? "No track has a genre yet." : "No genres are spelled two ways."}</p>}
    {genres && genres.groups.length > 0 && <ul className="folder-list" aria-label="Genres spelled alike">{genres.groups.map((group) => {
      const key = group.members.join("|");
      const to = names[key] ?? group.suggested;
      return <li key={key} className="folder-row genre-row">
        <div className="folder-text">
          <strong>{group.members.join(", ")}</strong>
          <small>{plural(group.tracks, "track", "tracks")}</small>
        </div>
        <input {...PLAIN_TEXT} aria-label={`New name for ${group.members.join(", ")}`} value={to} onChange={(event) => setNames({ ...names, [key]: event.target.value })} />
        <button className="secondary-button" disabled={busy || !to.trim()} onClick={() => void rename(group.members.filter((member) => member !== to.trim()), to)}>Merge</button>
      </li>;
    })}</ul>}
    {genres && genres.groups.length > 1 && <button className="import-button" disabled={busy} onClick={() => void mergeAll()}>{busy ? "Merging..." : `Merge all ${genres.groups.length} groups`}</button>}
    {genres && genres.genres.length > 0 && <button className="link-button" aria-expanded={showAll} onClick={() => setShowAll(!showAll)}>{showAll ? "Hide the list of genres" : `Show all ${plural(genres.genres.length, "genre", "genres")}`}</button>}
    {showAll && genres && <ul className="folder-list" aria-label="All genres">{genres.genres.map((genre) => {
      const to = names[genre.name] ?? genre.name;
      return <li key={genre.name} className="folder-row genre-row">
        <div className="folder-text"><strong>{genre.name}</strong><small>{plural(genre.tracks, "track", "tracks")}</small></div>
        <input {...PLAIN_TEXT} aria-label={`New name for ${genre.name}`} value={to} onChange={(event) => setNames({ ...names, [genre.name]: event.target.value })} />
        <button className="secondary-button" disabled={busy || !to.trim() || to.trim() === genre.name} onClick={() => void rename([genre.name], to)}>Rename</button>
      </li>;
    })}</ul>}
  </section>;
}

/** One row's worth of fixes for a track: a change of capitals alone is a row of its own, so it
 * can wait for a tick while the track's other fixes go ahead. `id` tells the rows apart. */
type Found = { id: number; trackId: number; fixes: ProposedFix[] };

function TitlesCard({ admin, onChanged }: Props) {
  const [found, setFound] = useState<Found[] | null>(null);
  /** The list folded away, its ticks kept. */
  const [folded, setFolded] = useState(false);
  const [unticked, setUnticked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const check = async () => {
    setBusy(true);
    setError("");
    setNote("");
    try {
      const names = await admin.trackNames();
      const next: Found[] = [];
      for (const track of names) {
        const fixes = proposeFixes(track);
        const casing = fixes.filter(onlyCasing);
        const others = fixes.filter((fix) => !onlyCasing(fix));
        if (others.length > 0) next.push({ id: next.length, trackId: track.trackId, fixes: others });
        if (casing.length > 0) next.push({ id: next.length, trackId: track.trackId, fixes: casing });
      }
      setFound(next);
      setFolded(false);
      // A change of capitals alone waits for a tick: those are often how an artist writes a name.
      setUnticked(new Set(next.filter((entry) => entry.fixes.every(onlyCasing)).map((entry) => entry.id)));
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const apply = async () => {
    if (!found) return;
    // A track's ticked rows go together, as one save of that track.
    const byTrack = new Map<number, ProposedFix[]>();
    for (const entry of found.filter((entry) => !unticked.has(entry.id))) {
      byTrack.set(entry.trackId, [...(byTrack.get(entry.trackId) ?? []), ...entry.fixes]);
    }
    setBusy(true);
    setError("");
    try {
      const done = await admin.editEach([...byTrack].map(([trackId, fixes]) => ({ trackId, changes: fixes.map((fix) => ({ field: fix.field, value: fix.to })) })));
      setNote(`${plural(byTrack.size, "track was", "tracks were")} cleaned up.${leftOut(done.left)}`);
      setFound(null);
      onChanged();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  /** Tracks with at least one ticked row: what the button counts. */
  const ticked = found ? new Set(found.filter((entry) => !unticked.has(entry.id)).map((entry) => entry.trackId)).size : 0;
  /** The same fix on several tracks (copies in two music folders, say) is one row. */
  const groups = useMemo(() => {
    const byFix = new Map<string, { trackIds: number[]; fixes: ProposedFix[] }>();
    // `trackIds` here are row ids (Found.id), one per track's row.
    for (const entry of found ?? []) {
      const key = JSON.stringify(entry.fixes);
      const group = byFix.get(key);
      if (group) group.trackIds.push(entry.id);
      else byFix.set(key, { trackIds: [entry.id], fixes: entry.fixes });
    }
    return [...byFix.values()];
  }, [found]);
  const toggleGroup = (trackIds: number[]) => setUnticked((before) => {
    const next = new Set(before);
    const on = trackIds.every((id) => !before.has(id));
    for (const id of trackIds) if (on) next.add(id); else next.delete(id);
    return next;
  });
  const casingOnly = groups.filter((group) => group.fixes.every(onlyCasing)).length;
  const groupsDrawn = useDrawn(groups.length);

  return <section className="sync-card" role="region" aria-label="Titles and artists">
    <h2>Titles and artists</h2>
    <p className="settings-copy">Finds titles with a track number in front ("01 - Title"), links or promo text, titles and artists in all capitals or all lower case, stray separators in artists, and "Artist - Title" titles on tracks with no artist.</p>
    <div className="merge-actions">
      <button className="secondary-button" disabled={busy} onClick={() => void check()}>{found ? "Check again" : "Check titles and artists"}</button>
      {found && found.length > 0 && <button className="link-button" aria-expanded={!folded} onClick={() => setFolded(!folded)}>{folded ? `Show the list (${groups.length})` : "Hide the list"}</button>}
    </div>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {found && found.length === 0 && <p className="settings-copy">Every title and artist looks tidy.</p>}
    {found && found.length > 0 && !folded && <>
      <div className="merge-actions">
        <button className="link-button" disabled={busy} onClick={() => setUnticked(new Set())}>Tick all</button>
        <button className="link-button" disabled={busy} onClick={() => setUnticked(new Set(found.map((entry) => entry.id)))}>Untick all</button>
      </div>
      {casingOnly > 0 && <p className="settings-copy">{`${plural(casingOnly, "suggestion only changes", "suggestions only change")} capitals and ${casingOnly === 1 ? "starts" : "start"} unticked: all capitals or all lower case is often how an artist writes a name.`}</p>}
      <ul className="folder-list" aria-label="Proposed fixes">{groups.slice(0, groupsDrawn.shown).map((group) => <li key={group.trackIds.join(",")} className="merge-row">
        <label className="send-row">
          <input type="checkbox" checked={group.trackIds.every((id) => !unticked.has(id))} onChange={() => toggleGroup(group.trackIds)} aria-label={group.fixes.map((fix) => fix.from).join(", ")} />
          <span className="folder-text">{group.fixes.map((fix) => <small key={fix.field}>{`${fix.field === "title" ? "Title" : "Artist"}: ${fix.from} -> ${fix.to}`}</small>)}{group.trackIds.length > 1 && <small>{`${group.trackIds.length} tracks`}</small>}</span>
        </label>
      </li>)}</ul>
      {groupsDrawn.more}
      <button className="import-button" disabled={busy || ticked === 0} onClick={() => void apply()}>{busy ? "Cleaning up..." : `Clean up ${plural(ticked, "track", "tracks")}`}</button>
    </>}
  </section>;
}

function ArtCard({ admin, onChanged }: Props) {
  const [missing, setMissing] = useState<MissingArt[] | null>(null);
  const [folded, setFolded] = useState(false);
  const [busy, setBusy] = useState<number | "all" | null>(null);
  const [notes, setNotes] = useState<Record<number, string>>({});
  /** What the last cover did, kept on the card: the release's row goes once it has a cover. */
  const [note, setNote] = useState("");
  const [progress, setProgress] = useState("");
  const [error, setError] = useState("");

  const check = async () => {
    setBusy("all");
    setError("");
    setNote("");
    try {
      setMissing(await admin.releasesWithoutArt());
      setFolded(false);
      setNotes({});
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };
  /** Give one release a cover; true when it got one. A failure stays on its row. */
  const fill = async (release: MissingArt, source: "folder" | "online" | "choose", alone = true): Promise<boolean> => {
    setBusy(alone ? release.releaseId : "all");
    setError("");
    if (alone) setNote("");
    try {
      const done = await admin.setReleaseArt(release.releaseId, source);
      if (!done) return false;
      if (alone) setNote(`${release.title}: ${plural(done.files, "file", "files")} now ${done.files === 1 ? "has" : "have"} the cover.${leftOut(done.left)}`);
      setMissing((before) => before?.filter((found) => found.releaseId !== release.releaseId) ?? null);
      onChanged();
      return true;
    } catch (reason) {
      setNotes((before) => ({ ...before, [release.releaseId]: message(reason) }));
      return false;
    } finally {
      if (alone) setBusy(null);
    }
  };
  /** Set while a run over every release should end after the release in hand. */
  const stopAsked = useRef(false);
  /** Give every listed release a cover, one after another. "folder" takes only releases with an
   * image beside their files; "any" takes the folder image where there is one and looks online
   * for the rest, which is one request a second, so it can be stopped. */
  const fillAll = async (how: "folder" | "any") => {
    if (!missing) return;
    const todo = how === "folder" ? missing.filter((found) => found.folderImage) : missing;
    setNote("");
    setNotes({});
    stopAsked.current = false;
    let given = 0;
    let tried = 0;
    for (const [index, release] of todo.entries()) {
      if (stopAsked.current) break;
      tried += 1;
      setProgress(`${how === "folder" ? "Using the folder images" : "Finding covers"}: ${index + 1} of ${todo.length}, ${release.title}...`);
      if (await fill(release, release.folderImage ? "folder" : "online", false)) given += 1;
    }
    setProgress("");
    setBusy(null);
    const failed = tried - given;
    setNote([
      `${plural(given, "release now has", "releases now have")} a cover${how === "folder" ? " from its folder" : ""}.`,
      failed > 0 ? `${failed} could not; each says why.` : "",
      tried < todo.length ? `Stopped with ${todo.length - tried} not tried.` : "",
    ].filter(Boolean).join(" "));
  };
  const withFolder = missing?.filter((found) => found.folderImage).length ?? 0;
  const missingDrawn = useDrawn(missing?.length ?? 0);

  return <section className="sync-card" role="region" aria-label="Album art">
    <h2>Album art</h2>
    <p className="settings-copy">Finds releases whose files carry no picture. A cover can come from an image beside the files (cover.jpg and the like), from the Cover Art Archive (needs no key; one request a second), or from an image you choose. It goes into every file of the release.</p>
    <div className="merge-actions">
      <button className="secondary-button" disabled={busy !== null} onClick={() => void check()}>{busy === "all" && !progress ? "Checking..." : missing ? "Check again" : "Check album art"}</button>
      {missing && missing.length > 0 && <button className="link-button" aria-expanded={!folded} onClick={() => setFolded(!folded)}>{folded ? `Show the list (${missing.length})` : "Hide the list"}</button>}
    </div>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {(progress || note) && <p role="status" className="settings-copy">{progress || note}{progress && <> <button className="link-button" disabled={stopAsked.current} onClick={() => { stopAsked.current = true; setProgress((current) => current && `${current.replace(/\.\.\.$/, "")}; stopping after this one...`); }}>Stop</button></>}</p>}
    {missing && missing.length === 0 && <p className="settings-copy">Every release has a cover.</p>}
    {missing && missing.length > 0 && !folded && <>
      <p className="settings-copy">{`${plural(missing.length, "release has", "releases have")} no cover${withFolder > 0 ? `; ${withFolder} ${withFolder === 1 ? "has" : "have"} an image beside the files` : ""}.`}</p>
      {missing.length > 1 && <div className="merge-actions">
        <button className="import-button" disabled={busy !== null} onClick={() => void fillAll("any")}>{`Find a cover for all ${missing.length}`}</button>
        {withFolder > 1 && withFolder < missing.length && <button className="secondary-button" disabled={busy !== null} onClick={() => void fillAll("folder")}>{`Use the folder image for the ${withFolder} that have one`}</button>}
      </div>}
      {missing.length > 1 && <p className="settings-copy">Find a cover for all takes the image beside the files where there is one and asks the Cover Art Archive for the rest, one release a second. You can stop it; what was given stays.</p>}
      <ul className="folder-list" aria-label="Releases without art">{missing.slice(0, missingDrawn.shown).map((release) => <li key={release.releaseId} className="folder-row">
        <div className="folder-text">
          <strong>{release.title}</strong>
          <small>{`${release.artist} - ${plural(release.files, "file", "files")}`}{notes[release.releaseId] ? ` - ${notes[release.releaseId]}` : ""}</small>
          {busy === release.releaseId && <small role="status">Looking for a cover...</small>}
        </div>
        {release.folderImage && <button className="secondary-button" disabled={busy !== null} aria-label={`Use the folder image for ${release.title}`} onClick={() => void fill(release, "folder")}>Use folder image</button>}
        <button className="secondary-button" disabled={busy !== null} aria-label={`Find a cover online for ${release.title}`} onClick={() => void fill(release, "online")}>Find cover online</button>
        <button className="secondary-button" disabled={busy !== null} aria-label={`Choose an image for ${release.title}`} onClick={() => void fill(release, "choose")}>Choose image...</button>
      </li>)}</ul>
      {missingDrawn.more}
    </>}
  </section>;
}

const FIELD_NAMES: Record<string, string> = { title: "title", artist: "artist", album: "album", albumArtist: "album artist", genre: "genre", label: "label", year: "year", track: "track number" };
const when = (ms: number) => new Date(ms).toLocaleString();

function HistoryCard({ admin, onChanged, saves }: Props & { saves: number }) {
  const [history, setHistory] = useState<EditBatch[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  /** The save the last undo could not finish, which the owner may give up on. */
  const [stuck, setStuck] = useState<number | null>(null);

  // Only the newest read is shown: two reads close together may answer out of order.
  const reads = useRef(0);
  const load = useCallback(async () => {
    const read = ++reads.current;
    try {
      const found = await admin.editHistory();
      if (read === reads.current) setHistory(found);
      return found;
    } catch (reason) {
      setError(message(reason));
      return null;
    }
  }, [admin]);
  useEffect(() => { void load(); }, [load, saves]);

  const undo = async (id: number) => {
    setBusy(true);
    setError("");
    setStuck(null);
    try {
      const done = await admin.undoLastEdit(id);
      setNote(`${plural(done.files, "file is", "files are")} back as before.${leftOut(done.left)}`);
      // The button stays off until the list says what the next undo would be.
      const found = await load();
      if (found?.[0]?.id === id) setStuck(id);
      onChanged();
    } catch (reason) {
      setError(message(reason));
      await load();
    } finally {
      setBusy(false);
    }
  };
  const forget = async (id: number) => {
    setBusy(true);
    setError("");
    try {
      await admin.forgetEdit(id);
      setStuck(null);
      setNote("That change is no longer in the list; the files keep what they hold now.");
      await load();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const describe = (batch: EditBatch) => `${batch.fields.map((field) => FIELD_NAMES[field] ?? field).join(", ")} of ${plural(batch.tracks, "track", "tracks")}, ${when(batch.savedMs)}`;
  /** Each value a save gave, with what it replaced: what Undo puts back. */
  const values = (batch: EditBatch) => batch.changes.map((change) => `${FIELD_NAMES[change.field] ?? change.field}: ${change.before === "" ? "empty" : `"${change.before}"`} became ${change.after === "" ? "empty" : `"${change.after}"`}${change.tracks > 1 ? ` on ${change.tracks} tracks` : ""}`);
  const SHOWN_VALUES = 4;

  return <section className="sync-card" role="region" aria-label="Recent changes">
    <h2>Recent changes</h2>
    <p className="settings-copy">Every tag change saved from the editor and this screen is remembered with what the files held before. Undo puts the newest one back. New album art is not undone this way.</p>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {history && history.length === 0 && <p className="settings-copy">No changes yet.</p>}
    {history && history.length > 0 && <>
      <ol className="history-list" aria-label="Saves">{history.slice(0, 10).map((batch) => {
        const shown = values(batch);
        return <li key={batch.id}>{describe(batch)}{shown.length > 0 && <ul className="history-values">{shown.slice(0, SHOWN_VALUES).map((line) => <li key={line}>{line}</li>)}{shown.length > SHOWN_VALUES && <li>{`and ${shown.length - SHOWN_VALUES} more`}</li>}</ul>}</li>;
      })}</ol>
      <div className="warning-actions">
        <button className="secondary-button" disabled={busy} title={values(history[0]!).length > 0 ? `Puts back: ${values(history[0]!).join("; ")}` : undefined} onClick={() => void undo(history[0]!.id)}>{`Undo: ${describe(history[0]!)}`}</button>
        {stuck === history[0]!.id && <button className="link-button" disabled={busy} onClick={() => void forget(history[0]!.id)}>Forget this change</button>}
      </div>
      {stuck === history[0]!.id && <p className="settings-copy">Part of this change could not be put back. Undo tries that part again; Forget this change drops it from the list so earlier changes can be undone.</p>}
    </>}
  </section>;
}

const ORDERS: { sort: Sort; label: string }[] = [
  { sort: "title", label: "Title" }, { sort: "artist", label: "Artist" }, { sort: "bpm", label: "BPM" }, { sort: "recent", label: "Recently added" },
];

/** "title order", "BPM order": an order's label inside a sentence. */
function orderWords(order: Sort): string {
  const label = ORDERS.find((item) => item.sort === order)?.label ?? order;
  return label === label.toUpperCase() ? label : label.toLowerCase();
}

function CratesCard({ admin, onChanged }: Props) {
  const [crates, setCrates] = useState<CrateInfo[]>([]);
  // Each tool has its own choice, so comparing two crates never sets up a merge that removes one.
  const [sorted, setSorted] = useState<number | null>(null);
  const [into, setInto] = useState<number | null>(null);
  const [from, setFrom] = useState<number | null>(null);
  const [first, setFirst] = useState<number | null>(null);
  const [second, setSecond] = useState<number | null>(null);
  const [remove, setRemove] = useState(false);
  /** A sort or merge waiting for a yes: both change crates for good. */
  const [asking, setAsking] = useState<"sort" | "merge" | null>(null);
  const [order, setOrder] = useState<Sort>("title");
  const [comparison, setComparison] = useState<CrateComparison | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      const found = await admin.crates();
      setCrates(found);
      // A choice whose crate has gone (merged away, deleted) falls back to a crate still there.
      const keep = (fallback: number) => (current: number | null) =>
        current !== null && found.some((crate) => crate.id === current) ? current : found[fallback]?.id ?? null;
      setSorted(keep(0));
      setInto(keep(0));
      setFrom(keep(1));
      setFirst(keep(0));
      setSecond(keep(1));
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);
  useEffect(() => { void load(); }, [load]);

  const act = async (work: () => Promise<string>) => {
    setAsking(null);
    setBusy(true);
    setError("");
    setNote("");
    // A comparison is of the crates as they were; any change (or a new comparison) replaces it.
    setComparison(null);
    try {
      setNote(await work());
      await load();
      onChanged();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const name = (id: number | null) => crates.find((found) => found.id === id)?.name ?? "";
  const picker = (label: string, value: number | null, set: (id: number) => void) => <label className="crate-pick">{label}
    <select value={value ?? ""} onChange={(event) => set(Number(event.target.value))}>{crates.map((found) => <option key={found.id} value={found.id}>{found.name}</option>)}</select>
  </label>;

  if (crates.length === 0) return <section className="sync-card" role="region" aria-label="Crates">
    <h2>Crates</h2>
    <p className="settings-copy">Sort, merge and compare crates here once you have some. Make a crate with the + beside CRATES in the sidebar.</p>
  </section>;
  return <section className="sync-card" role="region" aria-label="Crates">
    <h2>Crates</h2>
    <p className="settings-copy">Merge one crate into another, sort a crate for good, or see what two crates share. Crates are openCrate's own; rekordbox is not changed.</p>
    <div className="crate-tools">
      <div className="crate-tool">
        <h3>Sort</h3>
        {picker("Crate", sorted, setSorted)}
        <label className="crate-pick">Order<select value={order} onChange={(event) => setOrder(event.target.value as Sort)}>{ORDERS.map((item) => <option key={item.sort} value={item.sort}>{item.label}</option>)}</select></label>
        {asking === "sort"
          ? <div className="crate-confirm" role="alertdialog" aria-label={`Sort ${name(sorted)}?`} onKeyDown={(event) => { if (event.key === "Escape") setAsking(null); }}>
            <span>{`${name(sorted)} takes ${orderWords(order)} order for good; the order it has now is not kept.`}</span>
            <button className="link-button" disabled={busy} onClick={() => void act(async () => { await admin.sortCrate(sorted!, order); return `${name(sorted)} is now in ${orderWords(order)} order.`; })}>Sort crate</button>
            <button className="link-button" autoFocus onClick={() => setAsking(null)}>Cancel</button>
          </div>
          : <button className="secondary-button" disabled={busy || sorted === null} onClick={() => setAsking("sort")}>Sort crate...</button>}
      </div>
      {crates.length > 1 && <div className="crate-tool">
        <h3>Merge</h3>
        {picker("From", from, setFrom)}
        {picker("Into", into, setInto)}
        <label className="send-row"><input type="checkbox" checked={remove} onChange={(event) => setRemove(event.target.checked)} />Remove the merged crate</label>
        {asking === "merge"
          ? <div className="crate-confirm" role="alertdialog" aria-label={`Merge ${name(from)} into ${name(into)}?`} onKeyDown={(event) => { if (event.key === "Escape") setAsking(null); }}>
            <span>{`${name(from)}'s tracks go into ${name(into)}${remove ? `, and ${name(from)} is removed` : ""}.`}</span>
            <button className="link-button" disabled={busy} onClick={() => void act(async () => {
              const added = await admin.mergeCrates(into!, [from!], remove);
              return `${plural(added, "track", "tracks")} added to ${name(into)}${remove ? `; ${name(from)} removed` : ""}.`;
            })}>Merge crates</button>
            <button className="link-button" autoFocus onClick={() => setAsking(null)}>Cancel</button>
          </div>
          : <button className="secondary-button" disabled={busy || into === null || from === null || into === from} onClick={() => setAsking("merge")}>Merge...</button>}
      </div>}
      {crates.length > 1 && <div className="crate-tool">
        <h3>Compare</h3>
        {picker("First", first, (id) => { setFirst(id); setComparison(null); })}
        {picker("Second", second, (id) => { setSecond(id); setComparison(null); })}
        <button className="secondary-button" disabled={busy || first === null || second === null || first === second} onClick={() => void act(async () => { setComparison(await admin.compareCrates(first!, second!)); return ""; })}>Compare</button>
      </div>}
    </div>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {comparison && <div className="comparison" role="region" aria-label="Comparison">
      {([["In both", comparison.both], [`Only in ${name(first)}`, comparison.onlyFirst], [`Only in ${name(second)}`, comparison.onlySecond]] as const).map(([title, tracks]) => <div key={title}>
        <h4>{`${title} (${tracks.length})`}</h4>
        <ul className="check-list">{tracks.slice(0, 50).map((track) => <li key={track.trackId}><strong>{track.title}</strong> <small>{track.artist}</small></li>)}</ul>
        {tracks.length > 50 && <p className="settings-copy">{`And ${tracks.length - 50} more.`}</p>}
      </div>)}
    </div>}
  </section>;
}
