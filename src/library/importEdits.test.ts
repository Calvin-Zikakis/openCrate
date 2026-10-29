import { describe, expect, it } from "vitest";
import { acceptedFor, acceptedValue, editCount, sharedValue, sourceOf, withEdit } from "./importEdits";
import type { PlanFile, ReleaseLookup, TagField } from "./types";

const lookup: ReleaseLookup = {
  found: { source: "MusicBrainz", title: "Between Spaces", artist: "Studio 001", year: 2019, label: "Test Label", genre: null, tracks: 2, url: "https://musicbrainz.org/release/0b1d" },
  note: null,
  files: [
    { source: "R/01.flac", matchedBy: "title", changes: [{ field: "Title", from: "x", to: "Night Drive" }, { field: "Year", from: null, to: "2019" }, { field: "Track", from: null, to: "nope" }] },
    { source: "R/02.flac", matchedBy: null, changes: [] },
  ],
};

describe("what an import sends", () => {
  it("takes the lookup's suggestions and puts typed values on top, by file", () => {
    expect(acceptedFor(lookup, new Map())).toEqual([{ source: "R/01.flac", title: "Night Drive", year: 2019 }]);
    const edits = withEdit(withEdit(new Map(), ["R/02.flac", "R/01.flac"], "genre", " Techno "), ["R/01.flac"], "year", "2020");
    expect(acceptedFor(lookup, edits)).toEqual([
      { source: "R/01.flac", title: "Night Drive", year: 2020, genre: "Techno" },
      { source: "R/02.flac", genre: "Techno" },
    ]);
    // Blank text takes a typed value back; a number that is not whole says nothing.
    const cleared = withEdit(edits, ["R/01.flac", "R/02.flac"], "genre", "");
    expect(acceptedFor(null, cleared)).toEqual([{ source: "R/01.flac", year: 2020 }]);
    expect(editCount(cleared)).toBe(1);
    expect(acceptedValue("track", "3.5")).toBeUndefined();
    expect(acceptedValue("year", "120248202450")).toBeUndefined();
    expect(withEdit(new Map(), ["R/01.flac"], "year", "20x").size).toBe(0);
    expect(acceptedValue("track", "07")).toBe(7);
    expect(acceptedValue("title", "  ")).toBeUndefined();
  });

  it("names where a value came from, and what a release's files share", () => {
    const field = (extra: Partial<TagField>): TagField => ({ name: "Album", value: "Set", guessed: false, tidiedFrom: null, lookedUp: false, ...extra });
    expect(sourceOf(field({}), false)).toBe("tags");
    expect(sourceOf(field({ guessed: true }), false)).toBe("guessed");
    expect(sourceOf(field({ tidiedFrom: "Set;" }), false)).toBe("tidied");
    expect(sourceOf(field({ lookedUp: true, guessed: true }), false)).toBe("lookedUp");
    expect(sourceOf(field({ value: null }), false)).toBe("missing");
    expect(sourceOf(undefined, true)).toBe("edited");
    const file = (album: string | null): PlanFile => ({ source: "a", destination: null, shortened: false, problem: null, becomes: null, tags: { fields: [field({ value: album })], audio: null, length: null, readError: null, warnings: [] } });
    expect(sharedValue([file("Set"), file("Set")], "album")).toEqual({ value: "Set", mixed: false });
    expect(sharedValue([file("Set"), file("Other")], "album")).toEqual({ value: "", mixed: true });
    expect(sharedValue([file(null), file(null)], "album")).toEqual({ value: "", mixed: false });
  });
});
