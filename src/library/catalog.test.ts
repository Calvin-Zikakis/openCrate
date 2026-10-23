import { describe, expect, it } from "vitest";
import { createCatalog } from "./catalog";
import { DEFAULT_QUERY } from "./types";

describe("synthetic catalog windows", () => {
  const catalog = createCatalog();
  it("returns an exact total and bounded first and deep windows", () => {
    const first = catalog.read(DEFAULT_QUERY, 0, 100);
    expect(first.total).toBe(100_000);
    expect(first.items).toHaveLength(100);
    expect(first.items[0]?.id).toBe(99_999);
    const last = catalog.read(DEFAULT_QUERY, 99_990, 100);
    expect(last.items).toHaveLength(10);
    expect(last.items.at(-1)?.id).toBe(0);
    expect(catalog.read(DEFAULT_QUERY, 100_000, 20).items).toEqual([]);
  });
  it("filters without changing source data or totals on repeated windows", () => {
    const query = { ...DEFAULT_QUERY, search: "after studio", sort: "title" as const };
    const first = catalog.read(query, 0, 100);
    expect(first.total).toBeGreaterThan(0);
    expect(first.items.every((item) => item.title.includes("After") && item.artist.includes("Studio"))).toBe(true);
    expect(catalog.read(query, 100, 100).total).toBe(first.total);
    expect(catalog.read(DEFAULT_QUERY, 0, 100).total).toBe(100_000);
  });
  it("sorts null BPM last with stable ID tie-breaks", () => {
    const query = { ...DEFAULT_QUERY, sort: "bpm" as const };
    const first = catalog.read(query, 0, 200);
    expect(first.items.every((item) => item.bpm === 110)).toBe(true);
    expect(first.items.map((item) => item.id)).toEqual(first.items.map((item) => item.id).sort((a, b) => a - b));
    expect(catalog.read(query, 99_999, 1).items[0]?.bpm).toBeNull();
  });
  it("returns correct release and artist membership, including partial releases", () => {
    const small = createCatalog(23);
    const releases = small.read({ ...DEFAULT_QUERY, view: "releases" }, 0, 200);
    expect(releases.total).toBe(3);
    expect(releases.items[0]?.trackCount).toBe(3);
    expect(small.read({ ...DEFAULT_QUERY, releaseId: 2 }, 0, 200).total).toBe(3);
    expect(small.read({ ...DEFAULT_QUERY, artistId: 0 }, 0, 200).total).toBe(10);
    expect(catalog.read({ ...DEFAULT_QUERY, view: "artists" }, 0, 200).total).toBe(240);
  });
  it("filters formats and preserves deterministic synthetic data", () => {
    const result = catalog.read({ ...DEFAULT_QUERY, format: "MP3" }, 0, 200);
    expect(result.total).toBe(33_333);
    expect(result.items.every((item) => item.format === "MP3")).toBe(true);
    expect(createCatalog(1).read(DEFAULT_QUERY, 0, 1)).toEqual(createCatalog(1).read(DEFAULT_QUERY, 0, 1));
  });
  it("rejects invalid bounds, never silently truncates an oversized request", () => {
    for (const [start, count] of [[-1, 1], [0, 201], [0, 0], [0.5, 1], [0, 1.5], [NaN, 1]]) {
      expect(() => catalog.read(DEFAULT_QUERY, start ?? 0, count ?? 0)).toThrow();
    }
    expect(() => createCatalog(100_001)).toThrow();
    expect(() => createCatalog(-1)).toThrow();
    expect(createCatalog(0).read(DEFAULT_QUERY, 0, 20).total).toBe(0);
  });
});
