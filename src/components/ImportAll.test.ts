import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { acceptedFrom, doubtsIn } from "./ImportAll";
import { DEFAULT_OPTIONS, loadOptions } from "./ImportChoices";
import type { PlanFile } from "../library/types";

const file = (overrides: Partial<PlanFile> & { warnings?: string[]; guessed?: string[] } = {}): PlanFile => ({
  source: "R/a.flac", destination: "A/R/01 - A - T.flac", shortened: false, problem: null, becomes: null,
  tags: {
    fields: [
      { name: "Title", value: "Night Drive", guessed: false, tidiedFrom: null },
      ...(overrides.guessed ?? []).map((name) => ({ name, value: "1", guessed: true, tidiedFrom: null })),
    ],
    audio: "FLAC 16/44.1", length: "3:00", readError: null, warnings: overrides.warnings ?? [],
  },
  ...overrides,
});

describe("files whose tags deserve a look before importing everything", () => {
  it("lists warnings and guessed values, and leaves out clean and blocked files", () => {
    const doubts = doubtsIn("Between Spaces", [
      file(),
      file({ warnings: ["The year looks wrong.", "The artist or title is not in the tags; it came from the file name."], guessed: ["Track"] }),
      file({ problem: "A file is already there.", warnings: ["The year looks wrong."] }),
    ]);
    expect(doubts).toEqual([{
      release: "Between Spaces", file: "Night Drive",
      reasons: ["The year looks wrong.", "No track in the tags; taken from the file name."],
    }]);
  });
});

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

describe("values accepted from a lookup", () => {
  it("become one entry per ticked file, with numbers as numbers", () => {
    const lookup = {
      found: { source: "MusicBrainz", genre: null, title: "Between Spaces", artist: "Studio 001", year: 2019, label: null, tracks: 2, url: "https://musicbrainz.org/release/x" },
      note: null,
      files: [
        { source: "R/a.flac", matchedBy: "title", changes: [{ field: "Track", from: null, to: "2" }, { field: "Album artist", from: null, to: "Studio 001" }, { field: "Year", from: "1850", to: "2019" }] },
        { source: "R/b.flac", matchedBy: "title", changes: [{ field: "Title", from: "x", to: "Night Drive" }] },
        { source: "R/c.flac", matchedBy: "title", changes: [] },
        { source: "R/d.flac", matchedBy: null, changes: [] },
      ],
    };
    expect(acceptedFrom(lookup, new Set(["R/b.flac"]))).toEqual([{ source: "R/a.flac", track: 2, albumArtist: "Studio 001", year: 2019 }]);
    expect(acceptedFrom(lookup, new Set())).toHaveLength(2);
  });
});
