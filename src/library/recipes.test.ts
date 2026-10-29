import { describe, expect, it } from "vitest";
import { acceptedForRecipe, loadRecipes, saveRecipes, withRecipe, type Recipe } from "./recipes";
import type { PlanFile } from "./types";

const memory = () => {
  const store = new Map<string, string>();
  return { getItem: (key: string) => store.get(key) ?? null, setItem: (key: string, value: string) => { store.set(key, value); } };
};

describe("edit recipes", () => {
  it("round-trips recipes and ignores what it cannot read", () => {
    const storage = memory();
    expect(loadRecipes(storage)).toEqual([]);
    const recipes: Recipe[] = [
      { name: "Techno import", values: { genre: "Techno", label: "" }, rules: ["numbers", "casing"] },
      { name: "Just tidy", values: {}, rules: ["separators"] },
    ];
    saveRecipes(storage, recipes);
    expect(loadRecipes(storage)).toEqual(recipes);
    storage.setItem("opencrate.editRecipes", JSON.stringify([{ name: "ok", values: { genre: "House" }, rules: [] }, { name: 3 }, { name: "bad values", values: "x", rules: [] }, { name: "bad field", values: { bpm: "1" }, rules: [] }]));
    expect(loadRecipes(storage)).toEqual([{ name: "ok", values: { genre: "House" }, rules: [] }]);
    storage.setItem("opencrate.editRecipes", "{");
    expect(loadRecipes(storage)).toEqual([]);
  });
  it("applies a recipe's values, then its rules on the fields it did not type", () => {
    const recipe: Recipe = { name: "Techno", values: { genre: "Techno", track: "5" }, rules: ["numbers", "casing"] };
    expect(withRecipe({ title: "02 - NIGHT DRIVE", artist: "STUDIO", genre: "House" }, recipe))
      .toEqual({ title: "Night Drive", artist: "Studio", genre: "Techno" });
    // A typed title is the recipe's; the rules leave it.
    expect(withRecipe({ title: "02 - x" }, { ...recipe, values: { title: "01 - KEEP" } })).toEqual({ title: "01 - KEEP" });
  });
  it("turns a recipe into values to send with an import, for files that change and can go", () => {
    const field = (name: string, value: string | null) => ({ name, value, guessed: false, tidiedFrom: null, lookedUp: false });
    const file = (source: string, title: string, problem: string | null = null): PlanFile => ({
      source, destination: problem ? null : `A/${source}`, shortened: false, problem, becomes: null,
      tags: { fields: [field("Title", title), field("Artist", "Studio"), field("Year", "2019"), field("Track", "1")], audio: null, length: null, readError: null, warnings: [] },
    });
    const recipe: Recipe = { name: "Clean", values: { year: "2020" }, rules: ["numbers"] };
    expect(acceptedForRecipe([file("a.aif", "01 - Night Drive"), file("b.aif", "Soft Focus"), file("c.aif", "03 - Gone", "taken")], recipe)).toEqual([
      { source: "a.aif", title: "Night Drive", year: 2020 },
      { source: "b.aif", year: 2020 },
    ]);
    expect(acceptedForRecipe([file("b.aif", "Soft Focus")], { name: "Bad year", values: { year: "soon" }, rules: [] })).toEqual([]);
  });
});
