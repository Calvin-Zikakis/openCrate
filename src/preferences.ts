export type Theme = "system" | "dark" | "light";
export interface Preferences {
  compact: boolean;
  reducedMotion: boolean;
  /** "system" follows the computer's light or dark setting. */
  theme: Theme;
}
export const DEFAULT_PREFERENCES: Preferences = { compact: false, reducedMotion: false, theme: "dark" };
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
  return { compact: value.compact, reducedMotion: value.reducedMotion, theme: theme as Theme };
}

export function savePreferences(storage: Pick<Storage, "setItem">, value: Preferences) {
  storage.setItem(KEY, JSON.stringify(value));
}
