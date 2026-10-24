import { describe, expect, it } from "vitest";
import { showKey } from "./keys";

describe("key notation", () => {
  it("reads a key in any notation and shows it in the chosen one", () => {
    // rekordbox writes Camelot ("8A"), Open Key ("1m") or classic ("Am") depending on its setting.
    for (const written of ["8A", "1m", "Am", "A min", "A minor"]) {
      expect(showKey(written, "camelot")).toBe("8A");
      expect(showKey(written, "openKey")).toBe("1m");
      expect(showKey(written, "musical")).toBe("Am");
    }
    for (const written of ["8B", "1d", "C", "C maj", "Cmaj"]) {
      expect(showKey(written, "camelot")).toBe("8B");
      expect(showKey(written, "openKey")).toBe("1d");
      expect(showKey(written, "musical")).toBe("C");
    }
    expect(showKey("F#m", "camelot")).toBe("11A");
    expect(showKey("Gbm", "camelot")).toBe("11A");
    expect(showKey("Db", "camelot")).toBe("3B");
    expect(showKey("12A", "musical")).toBe("Dbm");
    expect(showKey("12B", "musical")).toBe("E");
    expect(showKey("6d", "camelot")).toBe("1B");
  });

  it("leaves what it cannot read, and the rekordbox setting, as written", () => {
    expect(showKey("8A", "rekordbox")).toBe("8A");
    expect(showKey("", "camelot")).toBe("");
    expect(showKey("??", "camelot")).toBe("??");
    expect(showKey("13A", "musical")).toBe("13A");
  });
});
