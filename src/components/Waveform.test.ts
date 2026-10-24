import { describe, expect, it } from "vitest";
import { byLevel, columns, fold, foldBands, snapped } from "./Waveform";
import { clock } from "./PlayerBar";

describe("the player's waveform and clock", () => {
  it("keeps the loudest of each slice and fills every bar", () => {
    expect(fold([0, 255, 10, 20], 2)).toEqual([255, 20]);
    expect(fold([7], 3)).toEqual([7, 7, 7]);
    expect(fold([], 5)).toEqual([]);
    expect(fold(Array.from({ length: 800 }, (_, i) => (i === 400 ? 200 : 1)), 160)[80]).toBe(200);
  });
  it("folds each band on its own", () => {
    expect(foldBands({ low: [1, 9, 3, 4], mid: [0, 0, 5, 0], high: [2, 0, 0, 8] }, 2))
      .toEqual({ low: [9, 4], mid: [0, 5], high: [2, 8] });
  });
  it("lays the close-up out around the playhead", () => {
    const detail = { perSecond: 10, low: [10, 20, 30, 40, 50], mid: [1, 2, 3, 4, 5], high: [0, 0, 0, 0, 9] };
    // Five pixels showing one second around the playhead at 0.2 s, each sampled at its centre:
    // -0.2 s, 0 s, 0.2 s, 0.4 s and 0.6 s (past the end).
    const shown = columns(detail, 200, 5, 1);
    expect(shown.map((column) => column?.low ?? null)).toEqual([null, 10, 30, 50, null]);
    expect(shown[2]).toEqual({ low: 30, mid: 3, high: 0 });
    expect(columns(detail, 0, 0, 1)).toEqual([]);
  });
  it("keeps a short peak on screen at every step of the scroll, so nothing blinks", () => {
    // Two slices to a pixel, and one loud slice: as the playhead moves by less than a slice,
    // the peak must stay in some pixel at full height.
    const low = Array.from({ length: 200 }, (_, i) => (i === 100 ? 255 : 10));
    const detail = { perSecond: 100, low, mid: low.map(() => 0), high: low.map(() => 0) };
    for (let at = 900; at < 920; at += 1) {
      const peak = Math.max(...columns(detail, at, 50, 1).map((column) => column?.low ?? 0));
      expect(peak, `at ${at} ms`).toBe(255);
    }
  });
  it("moves the close-up in whole pixels, so each column keeps its height as it scrolls", () => {
    // 150 slices a second under 1600 pixels for 10 s: a column covers one slice or two depending
    // on where it starts, so drawing from in-between positions makes heights flicker.
    const low = Array.from({ length: 3000 }, (_, i) => (i * 37) % 251);
    const detail = { perSecond: 150, low, mid: low.map(() => 0), high: low.map(() => 0) };
    const width = 1600;
    const msPerPixel = 10_000 / width;
    const first = columns(detail, snapped(5000, width, 10), width, 10);
    for (let step = 1; step < 40; step += 1) {
      const at = 5000 + step * 2.7;
      const shift = Math.round(snapped(at, width, 10) / msPerPixel) - Math.round(snapped(5000, width, 10) / msPerPixel);
      const now = columns(detail, snapped(at, width, 10), width, 10);
      for (let x = 100; x < 200; x += 1) expect(now[x]?.low, `step ${step}, column ${x}`).toBe(first[x + shift]?.low);
    }
    expect(snapped(1000.2, 1000, 10)).toBe(1000);
  });
  it("draws the loudest band first so none is hidden", () => {
    expect(byLevel({ low: 40, mid: 200, high: 90 })).toEqual([["mid", 200], ["high", 90], ["low", 40]]);
  });
  it("shows minutes and seconds", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(65_999)).toBe("1:05");
    expect(clock(-5)).toBe("0:00");
  });
});
