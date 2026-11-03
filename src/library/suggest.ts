/** Which library values to offer for what has been typed, and in what order. Pure. */

export interface Suggestion {
  value: string;
  /** The same name as the typed text once case, spaces and punctuation are ignored, spelled
   * differently: the library's spelling, worth pointing out. */
  nearMiss: boolean;
}

/** Letters and digits only, in lower case: the rule the Clean up screen groups genres by, so
 * "Deep House", "deep-house" and "DeepHouse" are one name. */
export function alikeKey(text: string): string {
  return Array.from(text.toLowerCase()).filter((char) => /\p{L}|\p{N}/u.test(char)).join("");
}

/** Options that contain the query (case-insensitive), ordered: the query itself first, then a
 * near-miss spelling, then options starting with the query, then those with a word starting
 * with it, then the rest; each tier in the options' own order. A blank query offers nothing. */
export function rank(query: string, options: readonly string[], limit = 50): Suggestion[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [];
  const key = alikeKey(query);
  const tiers: Suggestion[][] = [[], [], [], [], []];
  const seen = new Set<string>();
  for (const value of options) {
    if (seen.has(value)) continue;
    seen.add(value);
    const lowered = value.toLowerCase();
    const at = lowered.indexOf(needle);
    const nearMiss = key !== "" && value !== query && alikeKey(value) === key;
    const tier = value === query ? 0 : nearMiss ? 1 : at === 0 ? 2 : at > 0 && lowered[at - 1] === " " ? 3 : at > 0 ? 4 : -1;
    if (tier >= 0) tiers[tier]!.push({ value, nearMiss });
  }
  return tiers.flat().slice(0, limit);
}
