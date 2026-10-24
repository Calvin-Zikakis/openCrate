import type { KeyNotation } from "./library/keys";

export type Theme = "system" | "dark" | "light";
export interface Preferences {
  compact: boolean;
  reducedMotion: boolean;
  /** "system" follows the computer's light or dark setting. */
  theme: Theme;
  /** How keys are shown; "rekordbox" shows them as rekordbox wrote them. */
  keyNotation: KeyNotation;
  /** The page header (title, notes, cover) folded down to one line, for more rows. */
  headerFolded: boolean;
}
export const DEFAULT_PREFERENCES: Preferences = { compact: false, reducedMotion: false, theme: "dark", keyNotation: "rekordbox", headerFolded: false };
const NOTATIONS: readonly KeyNotation[] = ["rekordbox", "camelot", "openKey", "musical"];
const KEY = "opencrate.appearance.v1";
const THEMES: readonly Theme[] = ["system", "dark", "light"];

export function loadPreferences(storage: Pick<Storage, "getItem">): Preferences {
  const stored = storage.getItem(KEY);
  if (!stored) return DEFAULT_PREFERENCES;
  const value: unknown = JSON.parse(stored);
  if (typeof value !== "object" || value === null
    || !("compact" in value) || typeof value.compact !== "boolean"
    || !("reducedMotion" in value) || typeof value.reducedMotion !== "boolean") {
    throw new Error("Saved appearance settings are invalid. Choose your preferences again.");
  }
  // Settings saved before themes existed have none: they were made with the dark theme.
  const theme = "theme" in value ? value.theme : "dark";
  if (!THEMES.includes(theme as Theme)) throw new Error("Saved appearance settings are invalid. Choose your preferences again.");
  // Settings saved before key notation existed show keys as rekordbox wrote them.
  const keyNotation = "keyNotation" in value ? value.keyNotation : "rekordbox";
  if (!NOTATIONS.includes(keyNotation as KeyNotation)) throw new Error("Saved appearance settings are invalid. Choose your preferences again.");
  const headerFolded = "headerFolded" in value && value.headerFolded === true;
  return { compact: value.compact, reducedMotion: value.reducedMotion, theme: theme as Theme, keyNotation: keyNotation as KeyNotation, headerFolded };
}

export function savePreferences(storage: Pick<Storage, "setItem">, value: Preferences) {
  storage.setItem(KEY, JSON.stringify(value));
}
