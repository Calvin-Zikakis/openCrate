import { describe, expect, it } from "vitest";
import { fixCasing, removeJunk, removeNumberPrefix, tidySeparators } from "./cleanup";

describe("cleanup rules for track details", () => {
  it("removes a track number left at the front of a title", () => {
    expect(removeNumberPrefix("01 - Night Drive")).toBe("Night Drive");
    expect(removeNumberPrefix("1. Night Drive")).toBe("Night Drive");
    expect(removeNumberPrefix("07) Night Drive")).toBe("Night Drive");
    expect(removeNumberPrefix("A1 - Night Drive")).toBe("Night Drive");
    expect(removeNumberPrefix("B - Firebird")).toBe("Firebird");
    // A number that is part of the name stays.
    expect(removeNumberPrefix("808 State")).toBe("808 State");
    expect(removeNumberPrefix("99 Problems")).toBe("99 Problems");
    expect(removeNumberPrefix("A Love Supreme")).toBe("A Love Supreme");
  });

  it("fixes casing only where a whole value is in one case", () => {
    expect(fixCasing("NIGHT DRIVE (ORIGINAL MIX)")).toBe("Night Drive (Original Mix)");
    expect(fixCasing("night drive")).toBe("Night Drive");
    expect(fixCasing("DJ SHADOW - VIP EDIT")).toBe("DJ Shadow - VIP Edit");
    // Mixed case is someone's choice and stays.
    expect(fixCasing("deadmau5")).toBe("Deadmau5");
    expect(fixCasing("iPhone Song")).toBe("iPhone Song");
    expect(fixCasing("Night Drive")).toBe("Night Drive");
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
});
