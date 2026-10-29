import type { Accepted, PlanFile, ReleaseLookup, TagField, TagView } from "./types";

/** A tag the owner can type into the import table. Key and BPM are rekordbox's to analyse. */
export type GridField = "title" | "artist" | "album" | "albumArtist" | "label" | "genre" | "year" | "track";

/** What the owner typed for one file, by field. Empty text means "leave it as it is". */
export type Edit = Partial<Record<GridField, string>>;

/** The tag names the host uses for each field. */
export const TAG_NAMES: Record<GridField, string> = {
  title: "Title", artist: "Artist", album: "Album", albumArtist: "Album artist", label: "Label", genre: "Genre", year: "Year", track: "Track",
};
const FIELD_OF_TAG: Record<string, GridField> = Object.fromEntries(Object.entries(TAG_NAMES).map(([field, name]) => [name, field as GridField]));
const NUMBERS: ReadonlySet<GridField> = new Set(["year", "track"]);

/** Fields that belong to the whole release rather than one track. */
export const RELEASE_FIELDS: readonly GridField[] = ["album", "albumArtist", "label", "genre", "year"];

/** The values a lookup suggests, as the host takes them, for every file it matched. */
export function acceptedFromLookup(lookup: ReleaseLookup): Accepted[] {
  return lookup.files.filter((file) => file.changes.length > 0).map((file) => {
    const accepted: Accepted = { source: file.source };
    for (const change of file.changes) {
      const key = FIELD_OF_TAG[change.field];
      if (!key) continue;
      const value = NUMBERS.has(key) ? Number(change.to) : change.to;
      if (typeof value === "number" && !Number.isFinite(value)) continue;
      (accepted as unknown as Record<string, string | number>)[key] = value;
    }
    return accepted;
  });
}

/** The largest year or track number taken: four digits, as tags and file names have them. */
const MAX_NUMBER = 9999;

/** One typed value as the host takes it: numbers for year and track, trimmed text otherwise;
 * undefined when it says nothing (blank, or not a whole number up to four digits). */
export function acceptedValue(field: GridField, text: string): string | number | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return undefined;
  if (!NUMBERS.has(field)) return trimmed;
  const number = Number(trimmed);
  return Number.isInteger(number) && number >= 0 && number <= MAX_NUMBER ? number : undefined;
}

/** What to send with a release: the lookup's suggestions, with what the owner typed on top.
 * Sorted by file, so the same choices always make the same list. */
export function acceptedFor(lookup: ReleaseLookup | null, edits: ReadonlyMap<string, Edit>): Accepted[] {
  const bySource = new Map<string, Accepted>();
  if (lookup) for (const accepted of acceptedFromLookup(lookup)) bySource.set(accepted.source, accepted);
  for (const [source, edit] of edits) {
    const accepted = bySource.get(source) ?? { source };
    for (const [field, text] of Object.entries(edit) as [GridField, string][]) {
      const value = acceptedValue(field, text);
      if (value !== undefined) (accepted as unknown as Record<string, string | number>)[field] = value;
    }
    if (Object.keys(accepted).length > 1) bySource.set(source, accepted);
  }
  return [...bySource.values()].sort((a, b) => (a.source < b.source ? -1 : a.source > b.source ? 1 : 0));
}

/** Where a shown value came from, in the order the table marks them. */
export type Source = "edited" | "lookedUp" | "guessed" | "tidied" | "tags" | "missing";

export function sourceOf(field: TagField | undefined, edited: boolean): Source {
  if (edited) return "edited";
  if (!field || field.value === null) return "missing";
  if (field.lookedUp) return "lookedUp";
  if (field.guessed) return "guessed";
  if (field.tidiedFrom !== null) return "tidied";
  return "tags";
}

export function tagOf(view: TagView | null, field: GridField): TagField | undefined {
  return view?.fields.find((candidate) => candidate.name === TAG_NAMES[field]);
}

/** The value every file of a release shares for a field, "" when they differ or none has one. */
export function sharedValue(files: PlanFile[], field: GridField): { value: string; mixed: boolean } {
  const values = new Set(files.map((file) => tagOf(file.tags, field)?.value ?? ""));
  if (values.size === 1) return { value: [...values][0] ?? "", mixed: false };
  return { value: "", mixed: true };
}

/** How many typed values a set of edits holds. */
export function editCount(edits: ReadonlyMap<string, Edit>): number {
  let count = 0;
  for (const edit of edits.values()) count += Object.values(edit).filter((text) => text !== undefined).length;
  return count;
}

/** `edits` with one more value typed for each of `sources`; blank text takes the value back, and
 * so does a year or track that is not a whole number of up to four digits. */
export function withEdit(edits: ReadonlyMap<string, Edit>, sources: string[], field: GridField, text: string): Map<string, Edit> {
  const next = new Map(edits);
  for (const source of sources) {
    const edit: Edit = { ...next.get(source) };
    if (acceptedValue(field, text) === undefined) delete edit[field]; else edit[field] = text;
    if (Object.keys(edit).length === 0) next.delete(source); else next.set(source, edit);
  }
  return next;
}
