import { describe, expect, it } from "vitest";
import { applyEdit, checkValue, fieldForProblem, mergeAccepted, toAccepted, type ReleaseEdits } from "./importEdits";

describe("values typed in the import preview", () => {
  it("apply to the chosen files, and an empty value takes a field back to the file's own", () => {
    let edits: ReleaseEdits = {};
    edits = applyEdit(edits, ["a.aif", "b.aif"], "album", "  Between Spaces ");
    edits = applyEdit(edits, ["a.aif"], "title", "Night Drive");
    expect(edits).toEqual({ "a.aif": { album: "Between Spaces", title: "Night Drive" }, "b.aif": { album: "Between Spaces" } });
    edits = applyEdit(edits, ["b.aif"], "album", " ");
    expect(edits).toEqual({ "a.aif": { album: "Between Spaces", title: "Night Drive" } });
  });

  it("become what an import sends, numbers as numbers", () => {
    const edits: ReleaseEdits = { "a.aif": { album: "Between Spaces", year: "2026", track: "3" }, "b.aif": {} };
    expect(toAccepted(edits)).toEqual([{ source: "a.aif", album: "Between Spaces", year: 2026, track: 3 }]);
  });

  it("are checked: a year is four digits and a track a whole number from 1", () => {
    expect(checkValue("year", "2026")).toBeNull();
    expect(checkValue("year", "26")).toBe("A year is four digits, such as 2026.");
    expect(checkValue("track", "0")).toBe("A track number is a whole number from 1.");
    expect(checkValue("track", "4")).toBeNull();
    expect(checkValue("album", "")).toBeNull();
    expect(checkValue("title", "x".repeat(501))).toBe("That is too long.");
  });

  it("win over what a lookup suggests for the same file", () => {
    const typed = [{ source: "a.aif", album: "Mine" }];
    const looked = [{ source: "a.aif", album: "Theirs", year: 2019 }, { source: "b.aif", title: "Other" }];
    expect(mergeAccepted(typed, looked)).toEqual([
      { source: "a.aif", album: "Mine", year: 2019 },
      { source: "b.aif", title: "Other" },
    ]);
  });

  it("name the field a problem asks for", () => {
    expect(fieldForProblem("No release in the tags or file name.", true)).toBe("album");
    expect(fieldForProblem("No artist or title in the tags or file name.", false)).toBe("title");
    expect(fieldForProblem("No artist or title in the tags or file name.", true)).toBe("artist");
    expect(fieldForProblem("No year in the tags or file name.", true)).toBe("year");
    expect(fieldForProblem("A file is already there.", true)).toBeNull();
  });
});
