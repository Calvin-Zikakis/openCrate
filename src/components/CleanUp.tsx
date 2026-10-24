import { useCallback, useEffect, useState } from "react";
import { proposeFixes, type ProposedFix } from "../library/cleanup";
import type { CrateComparison, CrateInfo, EditBatch, Genres, LibraryAdmin, MissingArt, Sort } from "../library/types";
import { Icon } from "./Icon";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
/** Rows drawn; ticking all still covers every one. */
const DRAWN = 300;

interface Props {
  admin: LibraryAdmin;
  /** Files changed: the library should be read again. */
  onChanged: () => void;
}

/** Fixes across the whole library. Nothing changes until it is chosen; changes go into the files'
 * tags, and the next sync with rekordbox offers them to rekordbox. */
export function CleanUp({ admin, onChanged }: Props) {
  return <div className="sync-screen">
    <div className="feature-empty sync-hero">
      <div className="empty-icon"><Icon name="clean" size={36} /></div>
      <h1>Clean up</h1>
      <p>Fix what is untidy across your whole library. Nothing changes until you choose; changes go into the files' tags, and the next Sync with rekordbox offers them to rekordbox.</p>
    </div>
    <HistoryCard admin={admin} onChanged={onChanged} />
    <GenresCard admin={admin} onChanged={onChanged} />
    <TitlesCard admin={admin} onChanged={onChanged} />
    <ArtCard admin={admin} onChanged={onChanged} />
    <CratesCard admin={admin} onChanged={onChanged} />
  </div>;
}

function GenresCard({ admin, onChanged }: Props) {
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
  useEffect(() => { void load(); }, [load]);

  const rename = async (from: string[], to: string) => {
    if (!to.trim()) return;
    setBusy(true);
    setError("");
    try {
      const done = await admin.renameGenres(from, to.trim());
      setNote(`${plural(done.files, "file now has", "files now have")} the genre ${to.trim()}.${done.left.length > 0 ? ` ${plural(done.left.length, "file was", "files were")} left as they were.` : ""}`);
      setNames({});
      await load();
      onChanged();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };

  return <section className="sync-card" role="region" aria-label="Genres">
    <h2>Genres</h2>
    <p className="settings-copy">Genres that differ only in case, spaces or punctuation are grouped, with the commonest spelling suggested. Merge a group into one spelling, or rename any genre.</p>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {genres && genres.groups.length === 0 && <p className="settings-copy">{genres.genres.length === 0 ? "No track has a genre yet." : "No genres are spelled two ways."}</p>}
    {genres && genres.groups.length > 0 && <ul className="folder-list" aria-label="Genres spelled alike">{genres.groups.map((group) => {
      const key = group.members.join("|");
      const to = names[key] ?? group.suggested;
      return <li key={key} className="folder-row genre-row">
        <div className="folder-text">
          <strong>{group.members.join(", ")}</strong>
          <small>{plural(group.tracks, "track", "tracks")}</small>
        </div>
        <input aria-label={`New name for ${group.members.join(", ")}`} value={to} onChange={(event) => setNames({ ...names, [key]: event.target.value })} />
        <button className="secondary-button" disabled={busy || !to.trim()} onClick={() => void rename(group.members.filter((member) => member !== to.trim()), to)}>Merge</button>
      </li>;
    })}</ul>}
    {genres && genres.genres.length > 0 && <button className="link-button" aria-expanded={showAll} onClick={() => setShowAll(!showAll)}>{showAll ? "Hide the list of genres" : `Show all ${plural(genres.genres.length, "genre", "genres")}`}</button>}
    {showAll && genres && <ul className="folder-list" aria-label="All genres">{genres.genres.map((genre) => {
      const to = names[genre.name] ?? genre.name;
      return <li key={genre.name} className="folder-row genre-row">
        <div className="folder-text"><strong>{genre.name}</strong><small>{plural(genre.tracks, "track", "tracks")}</small></div>
        <input aria-label={`New name for ${genre.name}`} value={to} onChange={(event) => setNames({ ...names, [genre.name]: event.target.value })} />
        <button className="secondary-button" disabled={busy || !to.trim() || to.trim() === genre.name} onClick={() => void rename([genre.name], to)}>Rename</button>
      </li>;
    })}</ul>}
  </section>;
}

type Found = { trackId: number; fixes: ProposedFix[] };

function TitlesCard({ admin, onChanged }: Props) {
  const [found, setFound] = useState<Found[] | null>(null);
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
      setFound(names.map((track) => ({ trackId: track.trackId, fixes: proposeFixes(track) })).filter((track) => track.fixes.length > 0));
      setUnticked(new Set());
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const apply = async () => {
    if (!found) return;
    const chosen = found.filter((track) => !unticked.has(track.trackId));
    setBusy(true);
    setError("");
    try {
      const done = await admin.editEach(chosen.map((track) => ({ trackId: track.trackId, changes: track.fixes.map((fix) => ({ field: fix.field, value: fix.to })) })));
      setNote(`${plural(chosen.length, "track was", "tracks were")} cleaned up.${done.left.length > 0 ? ` ${plural(done.left.length, "file was", "files were")} left as they were.` : ""}`);
      setFound(null);
      onChanged();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const toggle = (trackId: number) => setUnticked((before) => {
    const next = new Set(before);
    if (!next.delete(trackId)) next.add(trackId);
    return next;
  });
  const ticked = found ? found.length - unticked.size : 0;

  return <section className="sync-card" role="region" aria-label="Titles and artists">
    <h2>Titles and artists</h2>
    <p className="settings-copy">Finds titles with a track number in front ("01 - Title"), links or promo text, titles and artists in all capitals or all lower case, stray separators in artists, and "Artist - Title" titles on tracks with no artist.</p>
    <button className="secondary-button" disabled={busy} onClick={() => void check()}>{found ? "Check again" : "Check titles and artists"}</button>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {found && found.length === 0 && <p className="settings-copy">Every title and artist looks tidy.</p>}
    {found && found.length > 0 && <>
      <div className="merge-actions">
        <button className="link-button" disabled={busy} onClick={() => setUnticked(new Set())}>Tick all</button>
        <button className="link-button" disabled={busy} onClick={() => setUnticked(new Set(found.map((track) => track.trackId)))}>Untick all</button>
      </div>
      <ul className="folder-list" aria-label="Proposed fixes">{found.slice(0, DRAWN).map((track) => <li key={track.trackId} className="merge-row">
        <label className="send-row">
          <input type="checkbox" checked={!unticked.has(track.trackId)} onChange={() => toggle(track.trackId)} aria-label={track.fixes.map((fix) => fix.from).join(", ")} />
          <span className="folder-text">{track.fixes.map((fix) => <small key={fix.field}>{`${fix.field === "title" ? "Title" : "Artist"}: ${fix.from} -> ${fix.to}`}</small>)}</span>
        </label>
      </li>)}</ul>
      {found.length > DRAWN && <p className="settings-copy">{`And ${found.length - DRAWN} more, included while ticked.`}</p>}
      <button className="import-button" disabled={busy || ticked === 0} onClick={() => void apply()}>{busy ? "Cleaning up..." : `Clean up ${plural(ticked, "track", "tracks")}`}</button>
    </>}
  </section>;
}

function ArtCard({ admin, onChanged }: Props) {
  const [missing, setMissing] = useState<MissingArt[] | null>(null);
  const [busy, setBusy] = useState<number | "all" | null>(null);
  const [notes, setNotes] = useState<Record<number, string>>({});
  const [error, setError] = useState("");

  const check = async () => {
    setBusy("all");
    setError("");
    try {
      setMissing(await admin.releasesWithoutArt());
      setNotes({});
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };
  const fill = async (release: MissingArt, source: "folder" | "online" | "choose") => {
    setBusy(release.releaseId);
    setError("");
    try {
      const done = await admin.setReleaseArt(release.releaseId, source);
      if (done) {
        setNotes({ ...notes, [release.releaseId]: `${plural(done.files, "file", "files")} now ${done.files === 1 ? "has" : "have"} the cover.${done.left.length > 0 ? ` ${plural(done.left.length, "file was", "files were")} left as they were.` : ""}` });
        setMissing((before) => before?.filter((found) => found.releaseId !== release.releaseId) ?? null);
        onChanged();
      }
    } catch (reason) {
      setNotes({ ...notes, [release.releaseId]: message(reason) });
    } finally {
      setBusy(null);
    }
  };
  const fillAllFromFolders = async () => {
    if (!missing) return;
    for (const release of missing.filter((found) => found.folderImage)) await fill(release, "folder");
  };
  const withFolder = missing?.filter((found) => found.folderImage).length ?? 0;

  return <section className="sync-card" role="region" aria-label="Album art">
    <h2>Album art</h2>
    <p className="settings-copy">Finds releases whose files carry no picture. A cover can come from an image beside the files (cover.jpg and the like), from the Cover Art Archive (needs no key; one request a second), or from an image you choose. It goes into every file of the release.</p>
    <button className="secondary-button" disabled={busy !== null} onClick={() => void check()}>{missing ? "Check again" : "Check album art"}</button>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {missing && missing.length === 0 && <p className="settings-copy">Every release has a cover.</p>}
    {missing && missing.length > 0 && <>
      <p className="settings-copy">{`${plural(missing.length, "release has", "releases have")} no cover${withFolder > 0 ? `; ${withFolder} ${withFolder === 1 ? "has" : "have"} an image beside the files` : ""}.`}</p>
      {withFolder > 1 && <button className="import-button" disabled={busy !== null} onClick={() => void fillAllFromFolders()}>{`Use the folder image for all ${withFolder}`}</button>}
      <ul className="folder-list" aria-label="Releases without art">{missing.slice(0, DRAWN).map((release) => <li key={release.releaseId} className="folder-row">
        <div className="folder-text">
          <strong>{release.title}</strong>
          <small>{`${release.artist} - ${plural(release.files, "file", "files")}`}{notes[release.releaseId] ? ` - ${notes[release.releaseId]}` : ""}</small>
        </div>
        {release.folderImage && <button className="secondary-button" disabled={busy !== null} aria-label={`Use the folder image for ${release.title}`} onClick={() => void fill(release, "folder")}>Use folder image</button>}
        <button className="secondary-button" disabled={busy !== null} aria-label={`Find a cover online for ${release.title}`} onClick={() => void fill(release, "online")}>Find online</button>
        <button className="secondary-button" disabled={busy !== null} aria-label={`Choose an image for ${release.title}`} onClick={() => void fill(release, "choose")}>Choose image...</button>
      </li>)}</ul>
      {missing.length > DRAWN && <p className="settings-copy">{`And ${missing.length - DRAWN} more.`}</p>}
    </>}
  </section>;
}

const FIELD_NAMES: Record<string, string> = { title: "title", artist: "artist", album: "album", albumArtist: "album artist", genre: "genre", label: "label", year: "year", track: "track number" };
const when = (ms: number) => new Date(ms).toLocaleString();

function HistoryCard({ admin, onChanged }: Props) {
  const [history, setHistory] = useState<EditBatch[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      setHistory(await admin.editHistory());
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);
  useEffect(() => { void load(); }, [load]);

  const undo = async () => {
    setBusy(true);
    setError("");
    try {
      const done = await admin.undoLastEdit();
      setNote(`${plural(done.files, "file is", "files are")} back as before.${done.left.length > 0 ? ` ${plural(done.left.length, "file was", "files were")} left as they were.` : ""}`);
      await load();
      onChanged();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const describe = (batch: EditBatch) => `${batch.fields.map((field) => FIELD_NAMES[field] ?? field).join(", ")} of ${plural(batch.tracks, "track", "tracks")}, ${when(batch.savedMs)}`;

  return <section className="sync-card" role="region" aria-label="Recent changes">
    <h2>Recent changes</h2>
    <p className="settings-copy">Every save from the editor and this screen is remembered with what the files held before. Undo puts the newest one back.</p>
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {history && history.length === 0 && <p className="settings-copy">No changes yet.</p>}
    {history && history.length > 0 && <>
      <ol className="history-list" aria-label="Saves">{history.slice(0, 10).map((batch) => <li key={batch.id}>{describe(batch)}</li>)}</ol>
      <button className="secondary-button" disabled={busy} onClick={() => void undo()}>{`Undo: ${describe(history[0]!)}`}</button>
    </>}
  </section>;
}

const ORDERS: { sort: Sort; label: string }[] = [
  { sort: "title", label: "Title" }, { sort: "artist", label: "Artist" }, { sort: "bpm", label: "BPM" }, { sort: "recent", label: "Recently added" },
];

function CratesCard({ admin, onChanged }: Props) {
  const [crates, setCrates] = useState<CrateInfo[]>([]);
  const [first, setFirst] = useState<number | null>(null);
  const [second, setSecond] = useState<number | null>(null);
  const [remove, setRemove] = useState(true);
  const [order, setOrder] = useState<Sort>("title");
  const [comparison, setComparison] = useState<CrateComparison | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      const found = await admin.crates();
      setCrates(found);
      setFirst((current) => current ?? found[0]?.id ?? null);
      setSecond((current) => current ?? found[1]?.id ?? null);
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);
  useEffect(() => { void load(); }, [load]);

  const act = async (work: () => Promise<string>) => {
    setBusy(true);
    setError("");
    setNote("");
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

  if (crates.length === 0) return null;
  return <section className="sync-card" role="region" aria-label="Crates">
    <h2>Crates</h2>
    <p className="settings-copy">Merge one crate into another, sort a crate for good, or see what two crates share. Crates are openCrate's own; rekordbox is not changed.</p>
    <div className="crate-tools">
      <div className="crate-tool">
        <h3>Sort</h3>
        {picker("Crate", first, setFirst)}
        <label className="crate-pick">Order<select value={order} onChange={(event) => setOrder(event.target.value as Sort)}>{ORDERS.map((item) => <option key={item.sort} value={item.sort}>{item.label}</option>)}</select></label>
        <button className="secondary-button" disabled={busy || first === null} onClick={() => void act(async () => { await admin.sortCrate(first!, order); return `${name(first)} is now in ${ORDERS.find((item) => item.sort === order)?.label.toLowerCase()} order.`; })}>Sort crate</button>
      </div>
      {crates.length > 1 && <div className="crate-tool">
        <h3>Merge</h3>
        {picker("From", second, setSecond)}
        {picker("Into", first, setFirst)}
        <label className="crate-pick"><input type="checkbox" checked={remove} onChange={(event) => setRemove(event.target.checked)} />Remove the merged crate</label>
        <button className="secondary-button" disabled={busy || first === null || second === null || first === second} onClick={() => void act(async () => {
          const added = await admin.mergeCrates(first!, [second!], remove);
          return `${plural(added, "track", "tracks")} added to ${name(first)}${remove ? `; ${name(second)} removed` : ""}.`;
        })}>Merge</button>
      </div>}
      {crates.length > 1 && <div className="crate-tool">
        <h3>Compare</h3>
        {picker("First", first, setFirst)}
        {picker("Second", second, setSecond)}
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
