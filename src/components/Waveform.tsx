import { useEffect, useRef, type PointerEvent } from "react";
import { cueLabel } from "./Analysis";
import type { TrackCue, WaveBands, WaveDetail } from "../library/types";

/** Bars drawn across the overview; the stored 800 slices are folded down to this many. */
const BARS = 320;
/** Seconds the close-up shows, half each side of the playhead. */
const WINDOW_SECONDS = 10;
const BANDS = ["low", "mid", "high"] as const;

/** The loudest of each slice, so short peaks survive the folding. */
export function fold(peaks: readonly number[], bars: number): number[] {
  if (peaks.length === 0) return [];
  return Array.from({ length: bars }, (_, bar) => {
    const from = Math.floor((bar * peaks.length) / bars);
    const to = Math.max(from + 1, Math.floor(((bar + 1) * peaks.length) / bars));
    return Math.max(...peaks.slice(from, to));
  });
}

export function foldBands(bands: WaveBands, bars: number): WaveBands {
  return { low: fold(bands.low, bars), mid: fold(bands.mid, bars), high: fold(bands.high, bars) };
}

export interface Column { low: number; mid: number; high: number }

/** The bands of a slice, loudest first: the drawing order that keeps every band visible. */
export function byLevel(column: Column): ["low" | "mid" | "high", number][] {
  return BANDS.map((band) => [band, column[band]] as ["low" | "mid" | "high", number]).sort((a, b) => b[1] - a[1]);
}

/** What each of `width` pixel columns shows, with the playhead at `positionMs` in the middle and
 * `seconds` across: the loudest of the slices the column covers, so a short peak never falls
 * between two samples and blinks as the picture scrolls. Null where the column's centre is before
 * the start or past the end. */
export function columns(detail: WaveDetail, positionMs: number, width: number, seconds: number): (Column | null)[] {
  const msPerPixel = (seconds * 1000) / Math.max(1, width);
  const slice = (ms: number) => (ms * detail.perSecond) / 1000;
  const count = detail.low.length;
  return Array.from({ length: Math.max(0, width) }, (_, x) => {
    const start = positionMs + (x - width / 2) * msPerPixel;
    const centre = start + msPerPixel / 2;
    if (centre < 0 || Math.floor(slice(centre) + 1e-9) >= count) return null;
    const from = Math.max(0, Math.floor(slice(start) + 1e-9));
    const to = Math.min(count, Math.max(from + 1, Math.ceil(slice(start + msPerPixel) - 1e-9)));
    const column = { low: 0, mid: 0, high: 0 };
    for (let index = from; index < to; index += 1) {
      column.low = Math.max(column.low, detail.low[index] ?? 0);
      column.mid = Math.max(column.mid, detail.mid[index] ?? 0);
      column.high = Math.max(column.high, detail.high[index] ?? 0);
    }
    return column;
  });
}

interface Props {
  bands: WaveBands | null;
  cues: TrackCue[];
  positionMs: number;
  durationMs: number;
  /** Where the pointer is, as a share of the track; `done` when it is let go. */
  onSeek: (fraction: number, done: boolean) => void;
  /** The native slider, for the keyboard and screen readers; the pointer is handled here, so a
   * click lands exactly where it points, even on the slider's own thumb. */
  children: React.ReactNode;
}

/** The whole track by sound (lows at the back, highs in front), the played part bright and
 * rekordbox's cues marked. */
export function Waveform({ bands, cues, positionMs, durationMs, onSeek, children }: Props) {
  const bars = bands ? foldBands(bands, BARS) : null;
  const loudest = bars ? Math.max(1, ...bars.low, ...bars.mid, ...bars.high) : 1;
  const played = durationMs > 0 ? Math.min(1, positionMs / durationMs) : 0;
  const marks = durationMs > 0
    ? cues.filter((cue) => cue.kind !== 3 && cue.startSeconds * 1000 <= durationMs)
    : [];
  const scrubbing = useRef(false);
  const pick = (event: PointerEvent<HTMLDivElement>, done: boolean) => {
    const box = event.currentTarget.getBoundingClientRect();
    onSeek(Math.min(1, Math.max(0, (event.clientX - box.left) / Math.max(1, box.width))), done);
  };
  return <div className="waveform" data-testid="waveform"
    onPointerDown={(event) => {
      if (event.button !== 0) return;
      scrubbing.current = true;
      event.currentTarget.setPointerCapture(event.pointerId);
      pick(event, false);
    }}
    onPointerMove={(event) => { if (scrubbing.current) pick(event, false); }}
    onPointerUp={(event) => { if (scrubbing.current) { scrubbing.current = false; pick(event, true); } }}
    onPointerCancel={() => { scrubbing.current = false; }}>
    <svg viewBox={`0 0 ${BARS} 24`} preserveAspectRatio="none" aria-hidden="true">
      {!bars || bars.low.length === 0
        ? <rect x="0" y="11.5" width={BARS} height="1" className="wave-empty" />
        : bars.low.map((_, bar) => {
          const state = (bar + 0.5) / BARS <= played ? "wave-played" : "wave-rest";
          // Largest first, so a quieter band is never hidden behind a louder one.
          return byLevel({ low: bars.low[bar] ?? 0, mid: bars.mid[bar] ?? 0, high: bars.high[bar] ?? 0 }).map(([band, level]) => {
            const height = Math.max(band === "low" ? 1 : 0, (level / loudest) * 22);
            return <rect key={`${bar}-${band}`} x={bar + 0.1} y={12 - height / 2} width="0.75" height={height} className={`band-${band} ${state}`} />;
          });
        })}
    </svg>
    {marks.map((cue, index) => <span key={`${cue.kind}-${cue.number}-${index}`}
      className={`cue-mark${cue.number !== null && cue.number >= 0 ? " hot" : ""}`}
      style={{ left: `${(cue.startSeconds * 100_000) / durationMs}%` }}
      title={`${cueLabel(cue)}${cue.name ? `: ${cue.name}` : ""}`} />)}
    {children}
  </div>;
}

interface ScopeProps {
  detail: WaveDetail | null;
  cues: TrackCue[];
  positionMs: number;
  playing: boolean;
}

/** Above this gap between where the picture is and where the player says it is, the picture jumps
 * (a seek); below it, the picture eases over, so the uneven timing of the reports never shows. */
const SNAP_MS = 300;
/** The share of the gap made up each frame. */
const EASE = 0.1;

/** The close-up that scrolls past a fixed playhead, coloured by sound like `MiniMeters`'
 * multi-band waveform. Between the player's position reports it moves on by the clock. */
export function Scope({ detail, cues, positionMs, playing }: ScopeProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const reported = useRef({ positionMs, at: performance.now(), playing });
  useEffect(() => { reported.current = { positionMs, at: performance.now(), playing }; }, [positionMs, playing]);

  useEffect(() => {
    const element = canvas.current;
    const context = element?.getContext("2d");
    if (!element || !context || !detail) return;
    let frame = 0;
    let frames = 0;
    let fills = { low: "", mid: "", high: "" };
    // Where the picture is, kept apart from the reports so it moves on smoothly.
    let shown: number | null = null;
    let last = performance.now();
    let drawn = "";
    const draw = () => {
      frame = requestAnimationFrame(draw);
      const now = performance.now();
      const report = reported.current;
      const target = report.positionMs + (report.playing ? now - report.at : 0);
      const moved = shown === null ? target : shown + (report.playing ? now - last : 0);
      const gap = target - moved;
      shown = !report.playing || Math.abs(gap) > SNAP_MS ? target : moved + gap * EASE;
      last = now;

      const ratio = window.devicePixelRatio || 1;
      const width = Math.max(1, Math.round(element.clientWidth * ratio));
      const height = Math.max(1, Math.round(element.clientHeight * ratio));
      // Paused and unchanged: nothing to draw.
      const state = `${shown}:${width}:${height}`;
      if (state === drawn && !report.playing) return;
      drawn = state;
      if (element.width !== width || element.height !== height) {
        element.width = width;
        element.height = height;
      }
      // The theme's colours, read again now and then rather than every frame.
      if (frames % 60 === 0) {
        const style = getComputedStyle(element);
        const colour = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
        fills = { low: colour("--wave-low", "#3d7cff"), mid: colour("--wave-mid", "#ff7a3d"), high: colour("--scope-high", "#f4f4f6") };
      }
      frames += 1;
      const at = shown;
      context.clearRect(0, 0, width, height);
      const middle = height / 2;
      columns(detail, at, width, WINDOW_SECONDS).forEach((column, x) => {
        if (!column) return;
        for (const [band, level] of byLevel(column)) {
          const half = Math.max(band === "low" ? 0.5 : 0, (level / 255) * middle * 0.96);
          if (half <= 0) continue;
          context.fillStyle = fills[band];
          context.fillRect(x, middle - half, 1, half * 2);
        }
      });
      // The part already played is dimmed, as on a deck.
      context.fillStyle = "rgba(13, 13, 16, 0.35)";
      context.fillRect(0, 0, Math.floor(width / 2), height);
      const msPerPixel = (WINDOW_SECONDS * 1000) / width;
      for (const cue of cues) {
        if (cue.kind === 3) continue;
        const x = Math.round(width / 2 + (cue.startSeconds * 1000 - at) / msPerPixel);
        if (x < 0 || x >= width) continue;
        context.fillStyle = cue.number !== null && cue.number >= 0 ? "#3dd68c" : "#e8e8ee";
        context.fillRect(x, 0, Math.max(1, Math.round(ratio)), height);
      }
      context.fillStyle = "#ffffff";
      context.fillRect(Math.floor(width / 2) - Math.round(ratio / 2), 0, Math.max(1, Math.round(ratio)), height);
    };
    draw();
    return () => cancelAnimationFrame(frame);
  }, [detail, cues]);

  return <canvas ref={canvas} className="scope" data-testid="scope" aria-hidden="true" />;
}
