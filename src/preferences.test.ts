import { describe, expect, it } from "vitest";
import { DEFAULT_PREFERENCES, loadPreferences, savePreferences } from "./preferences";

describe("appearance persistence", () => {
  it("returns defaults only when preferences are absent", () => {
    expect(loadPreferences({ getItem: () => null })).toEqual(DEFAULT_PREFERENCES);
  });
  it("validates and round-trips appearance", () => {
    let stored = "";
    const preferences = { compact: true, reducedMotion: false, theme: "light" as const, keyNotation: "camelot" as const, headerFolded: true, navFolded: true };
    savePreferences({ setItem: (_, value) => { stored = value; } }, preferences);
    expect(loadPreferences({ getItem: () => stored })).toEqual(preferences);
    for (const invalid of ["{", "null", "{}", '{"compact":"yes","reducedMotion":false}', '{"compact":true,"reducedMotion":false,"theme":"blue"}', '{"compact":true,"reducedMotion":false,"theme":"dark","keyNotation":"solfege"}']) {
      expect(() => loadPreferences({ getItem: () => invalid })).toThrow();
    }
  });
  it("reads settings saved before themes existed as the dark theme", () => {
    expect(loadPreferences({ getItem: () => '{"compact":true,"reducedMotion":true}' }))
      .toEqual({ compact: true, reducedMotion: true, theme: "dark", keyNotation: "rekordbox", headerFolded: false, navFolded: false });
    expect(DEFAULT_PREFERENCES.theme).toBe("dark");
  });
  it("propagates storage read and write failures for UI notification", () => {
    expect(() => loadPreferences({ getItem: () => { throw new Error("blocked"); } })).toThrow("blocked");
    expect(() => savePreferences({ setItem: () => { throw new Error("full"); } }, DEFAULT_PREFERENCES)).toThrow("full");
  });
});
