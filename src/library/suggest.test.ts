import { describe, expect, test } from "vitest";
import { alikeKey, rank } from "./suggest";

describe("alikeKey", () => {
  test("keeps letters and digits only, in lower case", () => {
    expect(alikeKey("Deep-House ")).toBe("deephouse");
    expect(alikeKey("Night Drive")).toBe(alikeKey("night  drive"));
  });
});

describe("rank", () => {
  test("orders prefix, then word start, then anywhere; an exact match is first and not a near-miss", () => {
    const rows = rank("Drive", ["Overdrive", "Driver", "Night Drive", "Drive"]);
    expect(rows.map((row) => row.value)).toEqual(["Drive", "Driver", "Night Drive", "Overdrive"]);
    expect(rows[0]).toEqual({ value: "Drive", nearMiss: false });
  });

  test("a spelling that differs only in case or separators comes first, marked as a near-miss", () => {
    expect(rank("night drive", ["Night Shift", "Night Drive"])).toEqual([{ value: "Night Drive", nearMiss: true }]);
    // A separator difference is a near-miss; a longer name that only shares the words is not
    // matched, since "deep-house" is not inside it.
    expect(rank("deep-house", ["Deep House", "Deep House Classics"])).toEqual([{ value: "Deep House", nearMiss: true }]);
  });

  test("matches special characters literally", () => {
    expect(rank("C++", ["C++", "Cafe"])).toEqual([{ value: "C++", nearMiss: false }]);
    // Punctuation is ignored by the alike rule, so "c+" is a near-miss of "C++" (and no error).
    expect(rank("c+", ["C++", "Cafe"])).toEqual([{ value: "C++", nearMiss: true }]);
    expect(rank("(original mix)", ["Title (Original Mix)", "Other"]).map((row) => row.value)).toEqual(["Title (Original Mix)"]);
  });

  test("a value listed twice is offered once", () => {
    expect(rank("set", ["Set", "Set", "Sunset"]).map((row) => row.value)).toEqual(["Set", "Sunset"]);
  });

  test("caps the result at the limit and gives nothing for a blank query", () => {
    const many = Array.from({ length: 60 }, (_, index) => `a${index}`);
    expect(rank("a", many)).toHaveLength(50);
    expect(rank("a", many, 3)).toHaveLength(3);
    expect(rank("  ", many)).toEqual([]);
  });
});
