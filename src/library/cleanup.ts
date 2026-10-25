/** Cleanup rules the track editor offers. Each returns the value unchanged when it does not apply,
 * so the editor can show only what a rule would change. */

/** "01 - Title", "1. Title", "07) Title", "A1 - Title" (a vinyl side): the number goes. Only what
 * reads as a track number goes: a leading zero, or 1 to 40, then a mark and a space. A number
 * that is part of the name stays ("808 State", "99 - Luftballons", "808 - State Of Mind"), and so
 * does a name that starts with a letter or digit and a mark ("D.A.N.C.E.", "A-Trak Edit",
 * "2-Step", "A - Z", "E - Motion"), and a countdown ("3 - 2 - 1"). */
export function removeNumberPrefix(title: string): string {
  const found = /^\s*(?:([A-H])(\d{1,2})|(\d{1,3}))\s*(?:[.)_]|[-–])\s+/u.exec(title);
  if (!found) return title;
  const number = found[3];
  if (number !== undefined && !number.startsWith("0") && (Number(number) < 1 || Number(number) > 40)) return title;
  const stripped = title.slice(found[0].length);
  // What is left starting with a number and a dash too is a countdown or a name, not a track number.
  if (/^\d+\s*[-–]\s/u.test(stripped)) return title;
  return stripped.trim() === "" ? title : stripped;
}

/** Words kept in capitals however the rest is cased. */
const UPPER = new Set(["DJ", "MC", "VIP", "EP", "LP", "UK", "USA", "NYC", "II", "III", "IV", "ID", "AM", "PM", "MDMA", "EDM", "DMT", "TV", "OK", "LA", "NY", "DC", "BPM"]);
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
  const words = [...text.matchAll(/[\p{L}\p{N}']+/gu)];
  const lastAt = words.at(-1)?.index ?? -1;
  return text.replace(/[\p{L}\p{N}']+/gu, (word, at: number) => {
    if (/\p{N}/u.test(word)) { first = false; return word; }
    const upper = word.toUpperCase();
    // A short all-capitals word with no vowel is an abbreviation ("LCD", "NRG"), kept as it is.
    // Two letters are too often a word cut short ("ST", "MR"); those go in UPPER by name.
    const abbreviation = allUpper && word.length >= 3 && word.length <= 4 && !/[AEIOUY]/u.test(upper);
    const result = UPPER.has(upper) || abbreviation ? upper
      // Small words go lower case, but never the first or the last ("Malcolm X", "Project X").
      : !first && at !== lastAt && SMALL.has(word.toLowerCase()) ? word.toLowerCase()
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

/** What follows the dash in "Title - Original Mix": a version of the track, not its title. */
const VERSION = /^(?:(?:original|extended|radio|club|dub|vocal|instrumental|acapella|a cappella|short|long|full|vip|main|clean|dirty)\b.*|.*\b(?:mix|edit|remix|rmx|version|dub|remaster(?:ed)?|live|instrumental|bootleg|rework|re-?edit|cut)\b.*|\d{4}.*)$/iu;

/** Every rule at once, for a track found in a library-wide check. An "Artist - Title" title is
 * split only when the artist is not set, so a title that merely holds a dash stays; and not when
 * the part after the dash names a version ("Night Drive - Original Mix", "Song - Remastered 2011"). */
export function proposeFixes(track: { trackId: number; title: string; artist: string }): ProposedFix[] {
  let { title, artist } = track;
  const unset = artist.trim() === "" || artist === NO_ARTIST;
  const parts = unset ? /^(.+?)\s+[-–]\s+(.+)$/u.exec(removeNumberPrefix(title)) : null;
  const split = parts && !VERSION.test(parts[2]!.trim()) && !/^\d+$/u.test(parts[1]!.trim()) ? parts : null;
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
