import type { ConvertFormat, ImportOptions } from "../library/types";

const KEY = "opencrate.importOptions";
export const DEFAULT_OPTIONS: ImportOptions = { convert: { lossless: null, lossy: null }, tidy: true };

const LOSSLESS: { value: ConvertFormat | null; label: string }[] = [
  { value: null, label: "Keep as they are" },
  { value: "aiff", label: "AIFF" },
  { value: "wav", label: "WAV" },
  { value: "flac", label: "FLAC" },
  { value: "mp3", label: "MP3 320" },
];
const LOSSY: { value: ConvertFormat | null; label: string }[] = [
  { value: null, label: "Keep as they are" },
  { value: "mp3", label: "MP3 320" },
];

const isFormat = (value: unknown): value is ConvertFormat | null =>
  value === null || value === "aiff" || value === "wav" || value === "flac" || value === "mp3";

/** The choices remembered in this browser, or the defaults. */
export function loadOptions(): ImportOptions {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (typeof saved === "object" && saved !== null) {
      const { convert, tidy } = saved as Partial<ImportOptions>;
      if (convert && isFormat(convert.lossless) && isFormat(convert.lossy) && typeof tidy === "boolean") {
        return { convert: { lossless: convert.lossless, lossy: convert.lossy === "mp3" ? "mp3" : null }, tidy };
      }
    }
  } catch { /* storage unavailable or unreadable: use the defaults */ }
  return DEFAULT_OPTIONS;
}

function saveOptions(options: ImportOptions) {
  try { localStorage.setItem(KEY, JSON.stringify(options)); } catch { /* applied, just not remembered */ }
}

interface Props {
  options: ImportOptions;
  onChange: (options: ImportOptions) => void;
  /** ffmpeg was found; without it, converting cannot be chosen. */
  canConvert: boolean;
}

/** What happens to files on import: converting by kind, and tidying names. */
export function ImportChoices({ options, onChange, canConvert }: Props) {
  const change = (next: ImportOptions) => { saveOptions(next); onChange(next); };
  const pick = (list: typeof LOSSLESS, kind: "lossless" | "lossy") =>
    <select aria-label={kind === "lossless" ? "Convert lossless files to" : "Convert lossy files to"} disabled={!canConvert}
      value={options.convert[kind] ?? ""}
      onChange={(event) => change({ ...options, convert: { ...options.convert, [kind]: list.find((item) => (item.value ?? "") === event.target.value)?.value ?? null } })}>
      {list.map((item) => <option key={item.label} value={item.value ?? ""}>{item.label}</option>)}
    </select>;
  return <div className="sync-card import-choices">
    <h2>When importing</h2>
    <div className="choice-grid">
      <label>Convert lossless files (FLAC, WAV, AIFF, ALAC) to {pick(LOSSLESS, "lossless")}</label>
      <label>Convert lossy files (MP3, AAC, OGG) to {pick(LOSSY, "lossy")}</label>
    </div>
    <p className="settings-copy">{canConvert
      ? "Files already in the chosen format are not converted. Converting lossy files to MP3 loses a little more quality."
      : "Converting needs ffmpeg, which was not found on this computer, so files go in as they are."}</p>
    <label className="setting-row">
      <span><strong>Tidy names</strong><small>{"Removes stray separators and spaces, such as the \";\" in \"Artist;\", and splits \"A;B\" into \"A, B\", before folders and file names are made."}</small></span>
      <input type="checkbox" checked={options.tidy} onChange={(event) => change({ ...options, tidy: event.target.checked })} />
    </label>
  </div>;
}
