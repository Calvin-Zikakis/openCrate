/** Musical keys as DJs write them. rekordbox writes a key the way its own setting says: Camelot
 * ("8A"), Open Key ("1m") or classic ("Am"); openCrate shows it the way the owner prefers. */

export type KeyNotation = "rekordbox" | "camelot" | "openKey" | "musical";

/** Camelot numbers 1 to 12, each with its minor and major key in classic spelling. */
const WHEEL: [string, string][] = [
  ["Abm", "B"], ["Ebm", "Gb"], ["Bbm", "Db"], ["Fm", "Ab"], ["Cm", "Eb"], ["Gm", "Bb"],
  ["Dm", "F"], ["Am", "C"], ["Em", "G"], ["Bm", "D"], ["F#m", "A"], ["Dbm", "E"],
];
/** Spellings that mean the same note as a wheel entry. */
const SAME: Record<string, string> = {
  "G#m": "Abm", "D#m": "Ebm", "A#m": "Bbm", "C#m": "Dbm", "Gbm": "F#m",
  "Cb": "B", "F#": "Gb", "C#": "Db", "G#": "Ab", "D#": "Eb", "A#": "Bb",
};

/** The Camelot number (1 to 12) and whether the key is minor, from any notation. */
function parse(key: string): { number: number; minor: boolean } | null {
  const text = key.trim();
  const camelot = /^(\d{1,2})\s*([ABab])$/u.exec(text);
  if (camelot) {
    const number = Number(camelot[1]);
    return number >= 1 && number <= 12 ? { number, minor: camelot[2]!.toUpperCase() === "A" } : null;
  }
  const open = /^(\d{1,2})\s*([mdMD])$/u.exec(text);
  if (open) {
    const number = Number(open[1]);
    // Open Key 1 is Camelot 8: the wheels start seven steps apart.
    return number >= 1 && number <= 12 ? { number: ((number + 6) % 12) + 1, minor: open[2]!.toLowerCase() === "m" } : null;
  }
  const classic = /^([A-Ga-g][#b]?)\s*(m|min|minor|maj|major)?$/u.exec(text);
  if (!classic) return null;
  const note = classic[1]!.charAt(0).toUpperCase() + classic[1]!.slice(1);
  const minor = classic[2] !== undefined && classic[2].toLowerCase().startsWith("m") && !classic[2].toLowerCase().startsWith("maj");
  const spelled = minor ? `${note}m` : note;
  const canonical = SAME[spelled] ?? spelled;
  const index = WHEEL.findIndex(([minorKey, majorKey]) => (minor ? minorKey : majorKey) === canonical);
  return index < 0 ? null : { number: index + 1, minor };
}

/** `key` as written by rekordbox, shown in `notation`. What cannot be read stays as written. */
export function showKey(key: string, notation: KeyNotation): string {
  if (notation === "rekordbox") return key;
  const parsed = parse(key);
  if (!parsed) return key;
  switch (notation) {
    case "camelot": return `${parsed.number}${parsed.minor ? "A" : "B"}`;
    case "openKey": return `${((parsed.number + 4) % 12) + 1}${parsed.minor ? "m" : "d"}`;
    case "musical": return WHEEL[parsed.number - 1]![parsed.minor ? 0 : 1];
  }
}
