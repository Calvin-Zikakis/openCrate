import { describe, expect, it } from "vitest";
import { loadRecipes, saveRecipes, type Recipe } from "./recipes";

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
});
