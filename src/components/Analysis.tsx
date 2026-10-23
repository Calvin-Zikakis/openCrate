import { useEffect, useState } from "react";
import type { LibraryAdmin, TrackAnalysis, TrackCue } from "../library/types";

const HOT = "ABCDEFGH";

/** `m:ss.mmm`, the way a player shows a position; the stored value keeps its fraction. */
export function clock(seconds: number): string {
  const total = Math.max(0, seconds);
  const whole = Math.floor(total);
  const millis = Math.min(999, Math.floor((total - whole) * 1000 + 1e-6));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
}

export function cueLabel(cue: TrackCue): string {
  if (cue.kind === 4) return cue.number !== null && cue.number >= 0 ? `Hot loop ${HOT[cue.number] ?? cue.number + 1}` : "Loop";
  if (cue.kind === 1) return "Fade in";
  if (cue.kind === 2) return "Fade out";
  if (cue.kind === 3) return "Load";
  if (cue.number !== null && cue.number >= 0) return `Hot cue ${HOT[cue.number] ?? cue.number + 1}`;
  return "Memory cue";
}

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

interface Props {
  admin: LibraryAdmin;
  trackId: number;
  title: string;
}

/** The cues and beat grid rekordbox has for one track, read only. */
export function Analysis({ admin, trackId, title }: Props) {
  const [analysis, setAnalysis] = useState<TrackAnalysis | null | undefined>(undefined);
  const [error, setError] = useState("");

  useEffect(() => {
    let stopped = false;
    setAnalysis(undefined);
    setError("");
    admin.trackAnalysis(trackId).then(
      (found) => { if (!stopped) setAnalysis(found); },
      (reason: unknown) => { if (!stopped) setError(message(reason)); },
    );
    return () => { stopped = true; };
  }, [admin, trackId]);

  if (error) return <p role="alert" className="inline-error">{error}</p>;
  if (analysis === undefined) return <p className="settings-copy" role="status">Reading...</p>;
  if (analysis === null) {
    return <p className="settings-copy">rekordbox has nothing for {title} yet. Read an export on the rekordbox screen once your folders are added.</p>;
  }
  return <div className="analysis">
    <p className="settings-copy">
      As rekordbox wrote it (export version {analysis.generation}){analysis.bpm !== null ? `, ${analysis.bpm} BPM` : ""}{analysis.musicalKey ? `, key ${analysis.musicalKey}` : ""}. openCrate never edits cues or grids.
    </p>
    <h3>Cues</h3>
    {analysis.cues.length === 0
      ? <p className="settings-copy">No cues or loops.</p>
      : <table className="analysis-table"><thead><tr><th>Kind</th><th>Name</th><th>Position</th><th>Loop end</th></tr></thead>
        <tbody>{analysis.cues.map((cue, index) => <tr key={index}>
          <td>{cueLabel(cue)}</td><td>{cue.name || "-"}</td><td>{clock(cue.startSeconds)}</td><td>{cue.endSeconds === null ? "-" : clock(cue.endSeconds)}</td>
        </tr>)}</tbody></table>}
    <h3>Beat grid</h3>
    {analysis.tempos.length === 0
      ? <p className="settings-copy">No beat grid.</p>
      : <table className="analysis-table"><thead><tr><th>From</th><th>Tempo</th><th>Meter</th><th>Beat</th></tr></thead>
        <tbody>{analysis.tempos.map((tempo, index) => <tr key={index}>
          <td>{clock(tempo.startSeconds)}</td><td>{tempo.bpm} BPM</td><td>{tempo.meter ?? "-"}</td><td>{tempo.beat ?? "-"}</td>
        </tr>)}</tbody></table>}
  </div>;
}
