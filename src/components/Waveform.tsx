import { cueLabel } from "./Analysis";
import type { TrackCue } from "../library/types";

/** Bars drawn across the bar; the stored overview is folded down to this many. */
const BARS = 160;

/** The loudest of each slice, so short peaks survive the folding. */
export function fold(peaks: readonly number[], bars: number): number[] {
  if (peaks.length === 0) return [];
  return Array.from({ length: bars }, (_, bar) => {
    const from = Math.floor((bar * peaks.length) / bars);
    const to = Math.max(from + 1, Math.floor(((bar + 1) * peaks.length) / bars));
    return Math.max(...peaks.slice(from, to));
  });
}

interface Props {
  peaks: number[] | null;
  cues: TrackCue[];
  positionMs: number;
  durationMs: number;
  /** The native slider on top, so dragging, clicking and the keyboard all seek. */
  children: React.ReactNode;
}

/** The waveform overview with the played part in the accent colour and rekordbox's cues marked. */
export function Waveform({ peaks, cues, positionMs, durationMs, children }: Props) {
  const bars = fold(peaks ?? [], BARS);
  const loudest = Math.max(1, ...bars);
  const played = durationMs > 0 ? Math.min(1, positionMs / durationMs) : 0;
  const marks = durationMs > 0
    ? cues.filter((cue) => cue.kind !== 3 && cue.startSeconds * 1000 <= durationMs)
    : [];
  return <div className="waveform" data-testid="waveform">
    <svg viewBox={`0 0 ${BARS} 24`} preserveAspectRatio="none" aria-hidden="true">
      {bars.length === 0
        ? <rect x="0" y="11.5" width={BARS} height="1" className="wave-rest" />
        : bars.map((peak, bar) => {
          const height = Math.max(1, (peak / loudest) * 22);
          return <rect key={bar} x={bar + 0.15} y={12 - height / 2} width="0.7" height={height} className={(bar + 0.5) / BARS <= played ? "wave-played" : "wave-rest"} />;
        })}
    </svg>
    {marks.map((cue, index) => <span key={`${cue.kind}-${cue.number}-${index}`}
      className={`cue-mark${cue.number !== null && cue.number >= 0 ? " hot" : ""}`}
      style={{ left: `${(cue.startSeconds * 100_000) / durationMs}%` }}
      title={`${cueLabel(cue)}${cue.name ? `: ${cue.name}` : ""}`} />)}
    {children}
  </div>;
}
