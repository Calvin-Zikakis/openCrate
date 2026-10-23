export interface Preferences {
  compact: boolean;
  reducedMotion: boolean;
}
export const DEFAULT_PREFERENCES: Preferences = { compact: false, reducedMotion: false };
const KEY = "opencrate.appearance.v1";

export function loadPreferences(storage: Pick<Storage, "getItem">): Preferences {
  const stored = storage.getItem(KEY);
  if (!stored) return DEFAULT_PREFERENCES;
  const value: unknown = JSON.parse(stored);
  if (typeof value !== "object" || value === null
    || !("compact" in value) || typeof value.compact !== "boolean"
    || !("reducedMotion" in value) || typeof value.reducedMotion !== "boolean") {
    throw new Error("Saved appearance settings are invalid. Choose your preferences again.");
  }
  return { compact: value.compact, reducedMotion: value.reducedMotion };
}

export function savePreferences(storage: Pick<Storage, "setItem">, value: Preferences) {
  storage.setItem(KEY, JSON.stringify(value));
}
