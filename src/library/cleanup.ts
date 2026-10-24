/** Cleanup rules the track editor offers. Each returns the value unchanged when it does not apply,
 * so the editor can show only what a rule would change. */

/** "01 - Title", "1. Title", "07) Title", "A1 - Title" (a vinyl side): the number goes. A number
 * that is part of the name, with no separator after it ("808 State"), stays. */
export function removeNumberPrefix(title: string): string {
  const stripped = title.replace(/^\s*(?:\d{1,3}|[A-H]\d{0,2})\s*(?:[-–.)_]|\s-\s)\s*/u, "");
  return stripped.trim() === "" ? title : stripped;
}

/** Words kept in capitals however the rest is cased. */
const UPPER = new Set(["DJ", "MC", "VIP", "EP", "LP", "UK", "USA", "NYC", "II", "III", "IV", "ID"]);
/** Small words lower case unless they start the value. */
const SMALL = new Set(["a", "an", "and", "at", "by", "for", "in", "of", "on", "or", "the", "to", "vs", "x"]);

/** ALL CAPS or all lower case becomes Title Case; a value that mixes cases is someone's choice
 * and stays as it is. A word holding digits ("EH2EH", "deadmau5") is a name and keeps its case. */
export function fixCasing(text: string): string {
  const letters = text.replace(/[^\p{L}]/gu, "");
  if (letters === "") return text;
  const allUpper = letters === letters.toUpperCase();
  const allLower = letters === letters.toLowerCase();
  if (!allUpper && !allLower) return text;
  let first = true;
  return text.replace(/[\p{L}\p{N}']+/gu, (word) => {
    if (/\p{N}/u.test(word)) { first = false; return word; }
    const upper = word.toUpperCase();
    const result = UPPER.has(upper) ? upper
      : !first && SMALL.has(word.toLowerCase()) ? word.toLowerCase()
        : word.charAt(0).toUpperCase() + word.slice(1).toLowerCase();
    first = false;
    return result;
  });
}

/** Links, store promos ("Free Download", "Out Now", "Buy"), stray underscores and doubled spaces. */
export function removeJunk(text: string): string {
  return text
    .replace(/[([]\s*(?:https?:\/\/|www\.)[^)\]]*[)\]]/giu, " ")
    .replace(/(?:https?:\/\/|www\.)\S+/giu, " ")
    .replace(/[([]\s*(?:free\s*(?:download|dl)|out\s*now|buy(?:\s*now)?|preview|snippet)\s*[)\]]/giu, " ")
    .replace(/_+/gu, " ")
    .replace(/\s{2,}/gu, " ")
    .replace(/[\s\-–,;]+$/u, "")
    .trim();
}

/** "Artist;" and "A;B" as the import tidies them: no stray separator, and "A, B". */
export function tidySeparators(text: string): string {
  return text
    .split(";")
    .map((part) => part.trim())
    .filter((part) => part !== "")
    .join(", ")
    .replace(/\s{2,}/gu, " ")
    .trim();
}

/** What the library calls a track with no artist in its tags. */
const NO_ARTIST = "Unknown Artist";

export interface ProposedFix { field: "title" | "artist"; from: string; to: string }

/** Only the capitals change: in DJ music "FSHR" or "mo wrights" is often how the artist writes it. */
export const onlyCasing = (fix: ProposedFix) => fix.from !== fix.to && fix.from.toLowerCase() === fix.to.toLowerCase();

/** Every rule at once, for a track found in a library-wide check. An "Artist - Title" title is
 * split only when the artist is not set, so a title that merely holds a dash stays. */
export function proposeFixes(track: { trackId: number; title: string; artist: string }): ProposedFix[] {
  let { title, artist } = track;
  const unset = artist.trim() === "" || artist === NO_ARTIST;
  const split = unset ? /^(.+?)\s+[-–]\s+(.+)$/u.exec(removeNumberPrefix(title)) : null;
  if (split) {
    artist = split[1]!;
    title = split[2]!;
  }
  title = fixCasing(removeJunk(removeNumberPrefix(title)));
  if (!unset || split) artist = fixCasing(removeJunk(tidySeparators(artist)));
  const fixes: ProposedFix[] = [];
  if (title !== track.title && title !== "") fixes.push({ field: "title", from: track.title, to: title });
  if (artist !== track.artist && artist !== "") fixes.push({ field: "artist", from: track.artist, to: artist });
  return fixes;
}
