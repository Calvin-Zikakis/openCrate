import { MAX_WINDOW, type LibraryItem, type LibraryPage, type LibraryQuery, type Sort, type View } from "./types";

const TITLES = ["After Hours", "Soft Focus", "Night Drive", "Open Water", "Low Orbit", "Slow Motion", "Morning Light", "Parallel Lines"];
const RELEASES = ["Between Spaces", "Nightfall Studies", "Colour Theory", "The Long Way Home", "Echoes in Motion", "Quiet Signals"];
const ARTISTS = ["Studio", "Ensemble", "Project", "Collective"];
const FORMATS = ["AIFF", "MP3", "FLAC"] as const;
const KEYS = ["8A", "9A", "10A", "11A", "4A", "5A"];

export function createCatalog(count = 100_000) {
  if (!Number.isInteger(count) || count < 0 || count > 100_000) {
    throw new Error("Synthetic catalog size must be between 0 and 100,000.");
  }
  const tracks: LibraryItem[] = [];
  const releases = new Map<number, LibraryItem>();
  const artists = new Map<number, LibraryItem>();
  for (let id = 0; id < count; id += 1) {
    const releaseId = Math.floor(id / 10);
    const artistId = releaseId % 240;
    const artist = `${ARTISTS[artistId % ARTISTS.length]} ${String(artistId + 1).padStart(3, "0")}`;
    const release = `${RELEASES[releaseId % RELEASES.length]} ${String(releaseId + 1).padStart(4, "0")}`;
    const item: LibraryItem = {
      id, kind: "tracks",
      title: `${TITLES[id % TITLES.length]} ${String(id + 1).padStart(6, "0")}`,
      artist, artistId, release, releaseId,
      bpm: id % 17 === 0 ? null : 110 + id % 31,
      musicalKey: KEYS[id % KEYS.length] ?? "8A",
      duration: 210 + id % 300,
      format: FORMATS[id % FORMATS.length] ?? "AIFF",
      trackCount: 1, missing: false, analysed: id % 17 !== 0, rekordboxKey: true, color: releaseId % 8,
    };
    tracks.push(item);
    const album = releases.get(releaseId);
    if (album) album.trackCount += 1;
    else releases.set(releaseId, { ...item, id: releaseId, kind: "releases", title: release, trackCount: 1 });
    const performer = artists.get(artistId);
    if (performer) performer.trackCount += 1;
    else artists.set(artistId, { ...item, id: artistId, kind: "artists", title: artist, trackCount: 1 });
  }
  const sources: Record<View, LibraryItem[]> = {
    tracks, releases: [...releases.values()], artists: [...artists.values()],
  };
  const indexes = new Map<string, LibraryItem[]>();
  let lastQuery = "";
  let lastRows: LibraryItem[] = [];

  function ordered(view: View, sort: Sort): LibraryItem[] {
    const key = `${view}:${sort}`;
    const cached = indexes.get(key);
    if (cached) return cached;
    const rows = [...sources[view]].sort((a, b) => {
      if (sort === "recent") return b.id - a.id;
      if (sort === "bpm") return (a.bpm ?? Infinity) - (b.bpm ?? Infinity) || a.id - b.id;
      const valueA = sort === "artist" ? a.artist : a.title;
      const valueB = sort === "artist" ? b.artist : b.title;
      return (valueA < valueB ? -1 : valueA > valueB ? 1 : 0) || a.id - b.id;
    });
    indexes.set(key, rows);
    return rows;
  }

  return {
    read(query: LibraryQuery, start: number, window: number): LibraryPage {
      if (!Number.isInteger(start) || start < 0 || !Number.isInteger(window) || window < 1 || window > MAX_WINDOW) {
        throw new Error(`Request a non-negative start and 1-${MAX_WINDOW} rows.`);
      }
      const key = JSON.stringify(query);
      if (key !== lastQuery) {
        const terms = query.search.trim().toLowerCase().split(/\s+/).filter(Boolean);
        const matches = (item: LibraryItem) => {
          const text = `${item.title} ${item.artist} ${item.release}`.toLowerCase();
          return terms.every((term) => text.includes(term));
        };
        // A release or artist shows for a track of its that matches, as in the desktop library.
        const byTrack = query.view !== "tracks" && terms.length > 0
          ? new Set(ordered("tracks", "recent").filter(matches).map((track) => (query.view === "releases" ? track.releaseId : track.artistId)))
          : null;
        lastRows = ordered(query.view, query.sort).filter((item) => {
          if (query.artistId !== null && item.artistId !== query.artistId) return false;
          if (query.releaseId !== null && item.releaseId !== query.releaseId) return false;
          if (query.format !== "all" && item.format !== query.format) return false;
          if (terms.length === 0) return true;
          return matches(item) || (byTrack?.has(item.id) ?? false);
        });
        lastQuery = key;
      }
      return { items: lastRows.slice(start, start + window), total: lastRows.length, start };
    },
  };
}
