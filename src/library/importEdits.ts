import type { Accepted, EditField } from "./types";

/** The tags that can be typed in the import preview. They go into the imported copy only. */
export type ImportEditField = Extract<EditField, "title" | "artist" | "album" | "albumArtist" | "year" | "genre" | "track">;

/** What was typed for one release: per file (its path in the watch folder), per field, as typed. */
export type ReleaseEdits = Record<string, Partial<Record<ImportEditField, string>>>;

/** Fields that are usually the same on every file of a release, so a change offers them all. */
export const RELEASE_WIDE: ReadonlySet<ImportEditField> = new Set(["album", "albumArtist", "year", "genre"]);

/** The preview's field names (as the tags are shown) and the field each one edits. */
export const FIELD_OF_NAME: Readonly<Record<string, ImportEditField>> = {
  Title: "title", Artist: "artist", Album: "album", "Album artist": "albumArtist", Year: "year", Genre: "genre", Track: "track",
};

export const FIELD_LABEL: Readonly<Record<ImportEditField, string>> = {
  title: "title", artist: "artist", album: "album", albumArtist: "album artist", year: "year", genre: "genre", track: "track number",
};

const MAX_CHARS = 500;

/** Why a typed value cannot be used, or null when it can. Empty is fine: it means "as the file has it". */
export function checkValue(field: ImportEditField, value: string): string | null {
  const text = value.trim();
  if (text === "") return null;
  if (text.length > MAX_CHARS) return "That is too long.";
  if (field === "year" && !/^\d{4}$/.test(text)) return "A year is four digits, such as 2026.";
  if (field === "track" && (!/^\d{1,3}$/.test(text) || Number(text) < 1)) return "A track number is a whole number from 1.";
  return null;
}

/** Set `field` to `value` on each of `sources`; an empty value takes the field back to the file's own. */
export function applyEdit(edits: ReleaseEdits, sources: readonly string[], field: ImportEditField, value: string): ReleaseEdits {
  const text = value.trim();
  const next: ReleaseEdits = { ...edits };
  for (const source of sources) {
    const fields = { ...next[source] };
    if (text === "") delete fields[field]; else fields[field] = text;
    if (Object.keys(fields).length === 0) delete next[source]; else next[source] = fields;
  }
  return next;
}

/** What an import sends for the typed values. */
export function toAccepted(edits: ReleaseEdits): Accepted[] {
  return Object.entries(edits).flatMap(([source, fields]) => {
    const entries = Object.entries(fields) as [ImportEditField, string][];
    if (entries.length === 0) return [];
    const accepted: Accepted = { source };
    for (const [field, value] of entries) {
      if (field === "year" || field === "track") accepted[field] = Number(value);
      else accepted[field] = value;
    }
    return [accepted];
  });
}

/** Typed values and a lookup's suggestions for the same files, the typed ones winning. */
export function mergeAccepted(typed: readonly Accepted[], looked: readonly Accepted[]): Accepted[] {
  const mine = new Map(typed.map((item) => [item.source, item]));
  const merged = looked.map((item) => ({ ...item, ...mine.get(item.source) }));
  const seen = new Set(looked.map((item) => item.source));
  return [...merged, ...typed.filter((item) => !seen.has(item.source))];
}

/** The field a file's problem asks for, when typing it would help. `hasTitle`: the file has a title. */
export function fieldForProblem(problem: string, hasTitle: boolean): ImportEditField | null {
  const missing = /^No (.+) in the tags or file name\.$/.exec(problem)?.[1];
  switch (missing) {
    case "release": return "album";
    case "artist or title": return hasTitle ? "artist" : "title";
    case "year": return "year";
    case "track": return "track";
    case "genre": return "genre";
    case "album artist": return "albumArtist";
    default: return null;
  }
}
