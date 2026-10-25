import { describe, expect, it } from "vitest";
import { fixCasing, onlyCasing, proposeFixes, removeJunk, removeNumberPrefix, tidySeparators } from "./cleanup";

describe("cleanup rules for track details", () => {
  it("removes a track number left at the front of a title", () => {
    expect(removeNumberPrefix("01 - Night Drive")).toBe("Night Drive");
    expect(removeNumberPrefix("1. Night Drive")).toBe("Night Drive");
    expect(removeNumberPrefix("07) Night Drive")).toBe("Night Drive");
    expect(removeNumberPrefix("A1 - Night Drive")).toBe("Night Drive");
    // A side letter alone is a name as often as a side: "A - Z", "E - Motion".
    expect(removeNumberPrefix("B - Firebird")).toBe("B - Firebird");
    // A number that is part of the name stays.
    expect(removeNumberPrefix("808 State")).toBe("808 State");
    expect(removeNumberPrefix("99 Problems")).toBe("99 Problems");
    expect(removeNumberPrefix("A Love Supreme")).toBe("A Love Supreme");
    // A name that only starts with a letter or digit and a mark is a name.
    for (const name of ["D.A.N.C.E.", "E.T.", "A-Trak Edit", "G-Funk Intro", "C-Section", "2-Step", "4-Ever", "B. Traits Theme",
      "A - Z", "H - Bomb", "C - Jam Blues", "E - Motion", "99 - Luftballons", "808 - State Of Mind", "3 - 2 - 1"]) {
      expect(removeNumberPrefix(name)).toBe(name);
    }
  });

  it("fixes casing only where a whole value is in one case", () => {
    expect(fixCasing("NIGHT DRIVE (ORIGINAL MIX)")).toBe("Night Drive (Original Mix)");
    expect(fixCasing("night drive")).toBe("Night Drive");
    expect(fixCasing("DJ SHADOW - VIP EDIT")).toBe("DJ Shadow - VIP Edit");
    // Mixed case is someone's choice and stays.
    // A word with digits in it is a name and keeps its case.
    expect(fixCasing("deadmau5")).toBe("deadmau5");
    expect(fixCasing("EH2EH")).toBe("EH2EH");
    expect(fixCasing("ST NICK B2B")).toBe("St Nick B2B");
    expect(fixCasing("iPhone Song")).toBe("iPhone Song");
    expect(fixCasing("Night Drive")).toBe("Night Drive");
    // The last word is never made small; abbreviations stay.
    expect(fixCasing("MALCOLM X")).toBe("Malcolm X");
    expect(fixCasing("PROJECT X")).toBe("Project X");
    expect(fixCasing("A X B")).toBe("A x B");
    expect(fixCasing("LCD SOUNDSYSTEM")).toBe("LCD Soundsystem");
    expect(fixCasing("4 AM")).toBe("4 AM");
    expect(fixCasing("MDMA")).toBe("MDMA");
  });

  it("tells a change of capitals alone from one that changes the words", () => {
    expect(onlyCasing({ field: "artist", from: "FSHR", to: "Fshr" })).toBe(true);
    expect(onlyCasing({ field: "title", from: "B2. Paradigma", to: "Paradigma" })).toBe(false);
    expect(onlyCasing({ field: "title", from: "Same", to: "Same" })).toBe(false);
  });

  it("removes links and promo text", () => {
    expect(removeJunk("Night Drive (www.example.com)")).toBe("Night Drive");
    expect(removeJunk("Night Drive https://example.com/track")).toBe("Night Drive");
    expect(removeJunk("Night Drive [FREE DOWNLOAD]")).toBe("Night Drive");
    expect(removeJunk("Night Drive (Out Now)")).toBe("Night Drive");
    expect(removeJunk("Night  Drive _")).toBe("Night Drive");
    expect(removeJunk("Night Drive (Original Mix)")).toBe("Night Drive (Original Mix)");
  });

  it("tidies stray separators as the import does", () => {
    expect(tidySeparators("Studio 001;")).toBe("Studio 001");
    expect(tidySeparators("Studio 001;Guest")).toBe("Studio 001, Guest");
    expect(tidySeparators(" Studio 001 ; Guest ")).toBe("Studio 001, Guest");
    expect(tidySeparators("Studio 001, Guest")).toBe("Studio 001, Guest");
  });

  it("proposes each track's fixes, splitting an artist out of the title when none is set", () => {
    expect(proposeFixes({ trackId: 1, title: "01 - NIGHT DRIVE", artist: "Studio 001;" })).toEqual([
      { field: "title", from: "01 - NIGHT DRIVE", to: "Night Drive" },
      { field: "artist", from: "Studio 001;", to: "Studio 001" },
    ]);
    expect(proposeFixes({ trackId: 2, title: "Studio 001 - Soft Focus", artist: "Unknown Artist" })).toEqual([
      { field: "title", from: "Studio 001 - Soft Focus", to: "Soft Focus" },
      { field: "artist", from: "Unknown Artist", to: "Studio 001" },
    ]);
    // A title with a dash and a real artist is left alone.
    expect(proposeFixes({ trackId: 3, title: "Night Drive - Dub", artist: "Studio 001" })).toEqual([]);
    // A version after the dash is not an artist before it.
    for (const title of ["Night Drive - Original Mix", "Song - Remastered 2011", "Track - Extended", "Tune - Radio Edit", "3 - 2 - 1"]) {
      expect(proposeFixes({ trackId: 4, title, artist: "Unknown Artist" }).filter((fix) => fix.field === "artist")).toEqual([]);
    }
  });
});
