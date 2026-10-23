import { describe, expect, it } from "vitest";
import { fold } from "./Waveform";
import { clock } from "./PlayerBar";

describe("the player's waveform and clock", () => {
  it("keeps the loudest of each slice and fills every bar", () => {
    expect(fold([0, 255, 10, 20], 2)).toEqual([255, 20]);
    expect(fold([7], 3)).toEqual([7, 7, 7]);
    expect(fold([], 5)).toEqual([]);
    expect(fold(Array.from({ length: 800 }, (_, i) => (i === 400 ? 200 : 1)), 160)[80]).toBe(200);
  });
  it("shows minutes and seconds", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(65_999)).toBe("1:05");
    expect(clock(-5)).toBe("0:00");
  });
});
