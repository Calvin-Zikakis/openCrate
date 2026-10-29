import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_OPTIONS, loadOptions } from "./ImportChoices";

describe("import choices remembered in this browser", () => {
  beforeEach(() => {
    const kept = new Map<string, string>();
    (globalThis as { localStorage?: unknown }).localStorage = {
      getItem: (key: string) => kept.get(key) ?? null,
      setItem: (key: string, value: string) => { kept.set(key, value); },
    };
  });
  afterEach(() => { delete (globalThis as { localStorage?: unknown }).localStorage; });
  it("come back as saved, and anything unreadable falls back to keeping files as they are", () => {
    expect(loadOptions()).toEqual(DEFAULT_OPTIONS);
    const chosen = { convert: { lossless: "aiff", lossy: "mp3" }, tidy: false };
    localStorage.setItem("opencrate.importOptions", JSON.stringify(chosen));
    expect(loadOptions()).toEqual(chosen);
    localStorage.setItem("opencrate.importOptions", JSON.stringify({ convert: { lossless: "ogg", lossy: null }, tidy: true }));
    expect(loadOptions()).toEqual(DEFAULT_OPTIONS);
    localStorage.setItem("opencrate.importOptions", "{nope");
    expect(loadOptions()).toEqual(DEFAULT_OPTIONS);
  });
});
