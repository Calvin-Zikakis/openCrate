import { describe, expect, it } from "vitest";
import { leftOut } from "./CleanUp";

describe("files left as they were", () => {
  it("says nothing when every file changed", () => {
    expect(leftOut([])).toBe("");
  });

  it("groups files by why, by name only, and folds a long list", () => {
    const damaged = "its embedded picture is damaged";
    const left = ["a", "b", "c", "d"].map((name) => `/Music/Oak/${name}.aif: ${damaged}`);
    left.push("C:\\Music\\e.mp3: file could not be opened: denied");
    left.push(`/Music2/Oak/a.aif: ${damaged}`);
    left.push("/Music/Live: Part 1.mp3: file could not be opened: busy");
    expect(leftOut(left)).toBe(
      " 7 files were left as they were. a.aif (2 copies), b.aif, c.aif and 1 more: its embedded picture is damaged. e.mp3: file could not be opened: denied. Live: Part 1.mp3: file could not be opened: busy.",
    );
  });
});
