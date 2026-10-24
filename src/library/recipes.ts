import type { EditField } from "./types";

/** A saved set of typed values and cleanup rules the editor applies together. Kept in this
 * browser; openCrate's database is for the library, not for how one person likes to tag. */
export interface Recipe {
  name: string;
  values: Partial<Record<EditField, string>>;
  /** Cleanup rule ids, in the order to apply them. */
  rules: string[];
}

const KEY = "opencrate.editRecipes";
const FIELDS: readonly EditField[] = ["title", "artist", "album", "albumArtist", "genre", "label", "year", "track"];

function isRecipe(value: unknown): value is Recipe {
  if (typeof value !== "object" || value === null) return false;
  const { name, values, rules } = value as Record<string, unknown>;
  if (typeof name !== "string" || name.trim() === "") return false;
  if (typeof values !== "object" || values === null || Array.isArray(values)) return false;
  if (!Object.entries(values).every(([field, text]) => FIELDS.includes(field as EditField) && typeof text === "string")) return false;
  return Array.isArray(rules) && rules.every((rule) => typeof rule === "string");
}

/** The saved recipes; anything unreadable is left out. */
export function loadRecipes(storage: Pick<Storage, "getItem">): Recipe[] {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter(isRecipe) : [];
  } catch {
    return [];
  }
}

export function saveRecipes(storage: Pick<Storage, "setItem">, recipes: Recipe[]) {
  try { storage.setItem(KEY, JSON.stringify(recipes)); } catch { /* applied, just not remembered */ }
}
