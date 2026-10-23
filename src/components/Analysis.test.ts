import { describe, expect, it } from "vitest";
import { clock, cueLabel } from "./Analysis";

describe("showing rekordbox's cue data", () => {
  it("formats positions like a player and keeps the fraction", () => {
    expect(clock(0)).toBe("0:00.000");
    expect(clock(12.345678)).toBe("0:12.345");
    expect(clock(60.25)).toBe("1:00.250");
    expect(clock(3599.999)).toBe("59:59.999");
    expect(clock(-3)).toBe("0:00.000");
    expect(clock(0.0255)).toBe("0:00.025");
  });

  it("names cues the way rekordbox numbers them", () => {
    const cue = { kind: 0, number: -1, startSeconds: 1, endSeconds: null, name: "" };
    expect(cueLabel(cue)).toBe("Memory cue");
    expect(cueLabel({ ...cue, number: 0 })).toBe("Hot cue A");
    expect(cueLabel({ ...cue, number: 7 })).toBe("Hot cue H");
    expect(cueLabel({ ...cue, number: 9 })).toBe("Hot cue 10");
    expect(cueLabel({ ...cue, kind: 4, number: -1 })).toBe("Loop");
    expect(cueLabel({ ...cue, kind: 4, number: 1 })).toBe("Hot loop B");
    expect(cueLabel({ ...cue, kind: 1 })).toBe("Fade in");
    expect(cueLabel({ ...cue, kind: 2 })).toBe("Fade out");
    expect(cueLabel({ ...cue, kind: 3 })).toBe("Load");
  });
});
