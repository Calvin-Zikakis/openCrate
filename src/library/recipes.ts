import { fixCasing, removeJunk, removeNumberPrefix, tidySeparators } from "./cleanup";
import type { Accepted, EditField, PlanFile } from "./types";

/** Cleanup rules, each for the fields it suits. */
export const RULES: { id: string; label: string; fields: EditField[]; apply: (value: string) => string }[] = [
  { id: "numbers", label: "Remove number prefixes", fields: ["title"], apply: removeNumberPrefix },
  { id: "casing", label: "Fix casing", fields: ["title", "artist", "album"], apply: fixCasing },
  { id: "junk", label: "Remove links and junk", fields: ["title", "artist", "album"], apply: removeJunk },
  { id: "separators", label: "Tidy separators", fields: ["artist", "albumArtist"], apply: tidySeparators },
];

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

/** One file's values with a recipe applied: its typed values (never a track number, which
 * belongs to one track), then its rules in order on the fields it did not type. */
export function withRecipe(values: Partial<Record<EditField, string>>, recipe: Recipe): Partial<Record<EditField, string>> {
  const typed = Object.entries(recipe.values).filter(([field]) => field !== "track") as [EditField, string][];
  const next = { ...values, ...Object.fromEntries(typed) };
  for (const rule of recipe.rules.flatMap((id) => RULES.filter((found) => found.id === id))) {
    for (const field of rule.fields) {
      const value = next[field];
      if (value === undefined || typed.some(([name]) => name === field)) continue;
      next[field] = rule.apply(value);
    }
  }
  return next;
}

/** The import screen's field names for the fields a recipe can change. */
const PLAN_FIELDS: Record<string, EditField> = { Title: "title", Artist: "artist", Album: "album", "Album artist": "albumArtist", Genre: "genre", Label: "label", Year: "year" };

/** What a recipe changes in the files of an import plan, as values to send with the import. */
export function acceptedForRecipe(files: PlanFile[], recipe: Recipe): Accepted[] {
  return files.flatMap((file) => {
    if (file.problem !== null || !file.tags) return [];
    const before: Partial<Record<EditField, string>> = {};
    for (const field of file.tags.fields) {
      const key = PLAN_FIELDS[field.name];
      if (key && field.value !== null) before[key] = field.value;
    }
    const after = withRecipe(before, recipe);
    const accepted: Accepted = { source: file.source };
    let changed = false;
    for (const [field, value] of Object.entries(after) as [EditField, string][]) {
      if (value === before[field] || field === "track") continue;
      if (field === "year") {
        const year = Number(value);
        if (!/^\d{1,4}$/.test(value.trim()) || year < 1) continue;
        accepted.year = year;
      } else {
        accepted[field] = value;
      }
      changed = true;
    }
    return changed ? [accepted] : [];
  });
}
