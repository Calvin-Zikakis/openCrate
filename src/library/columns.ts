/** The track list's columns: which show, how wide, in what order, and the owner's own default. */

export const TRACK_COLUMNS = [
  { id: "number", label: "#", width: 28, fixed: true },
  { id: "title", label: "Title / Artist", width: 0, fixed: true },
  { id: "release", label: "Release", width: 180 },
  { id: "genre", label: "Genre", width: 110 },
  { id: "label", label: "Label", width: 110 },
  { id: "year", label: "Year", width: 48 },
  { id: "bpm", label: "BPM", width: 44 },
  { id: "key", label: "Key", width: 40 },
  { id: "time", label: "Time", width: 42 },
  { id: "format", label: "Format", width: 48 },
  { id: "added", label: "Added", width: 92 },
] as const;
export type TrackColumn = (typeof TRACK_COLUMNS)[number]["id"];
export type ColumnSpec = (typeof TRACK_COLUMNS)[number];

export interface ColumnSettings { hidden: TrackColumn[]; widths: Partial<Record<TrackColumn, number>>; order: TrackColumn[] }

/** How the columns come before anyone touches them. */
export const BUILT_IN_COLUMNS: ColumnSettings = { hidden: ["genre", "label", "year"], widths: {}, order: [] };
export const COLUMN_MIN = 30;
export const COLUMN_MAX = 600;
/** The columns as they are now, in this browser. */
export const COLUMNS_KEY = "opencrate.trackColumns";
/** The owner's own default, which Reset goes back to; absent means the built-in one. */
export const COLUMNS_DEFAULT_KEY = "opencrate.trackColumnsDefault";

const ids = TRACK_COLUMNS.map((column) => column.id as string);
const isColumn = (id: unknown): id is TrackColumn => typeof id === "string" && ids.includes(id);

/** Settings as saved, with anything unknown dropped; null when nothing usable is there. */
function parse(text: string | null): ColumnSettings | null {
  try {
    const saved: unknown = JSON.parse(text ?? "null");
    if (typeof saved !== "object" || saved === null) return null;
    const { hidden, widths, order } = saved as Partial<ColumnSettings>;
    return {
      hidden: Array.isArray(hidden) ? hidden.filter((id): id is TrackColumn => isColumn(id) && !("fixed" in (TRACK_COLUMNS.find((column) => column.id === id) ?? {}))) : [],
      widths: typeof widths === "object" && widths !== null ? Object.fromEntries(Object.entries(widths).filter(([id, width]) => isColumn(id) && typeof width === "number" && Number.isFinite(width))) as ColumnSettings["widths"] : {},
      order: Array.isArray(order) ? order.filter(isColumn) : [],
    };
  } catch {
    return null;
  }
}

function read(key: string): ColumnSettings | null {
  try { return parse(localStorage.getItem(key)); } catch { return null; }
}
function write(key: string, settings: ColumnSettings | null) {
  try {
    if (settings === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(settings));
  } catch { /* applied for now, not remembered */ }
}

/** The owner's default, or the built-in one. */
export const defaultColumns = (): ColumnSettings => read(COLUMNS_DEFAULT_KEY) ?? BUILT_IN_COLUMNS;
export const hasOwnDefault = (): boolean => read(COLUMNS_DEFAULT_KEY) !== null;
export const loadColumns = (): ColumnSettings => read(COLUMNS_KEY) ?? defaultColumns();
export const saveColumns = (settings: ColumnSettings) => write(COLUMNS_KEY, settings);
export const saveDefaultColumns = (settings: ColumnSettings | null) => write(COLUMNS_DEFAULT_KEY, settings);

/** Whether two settings show the same columns the same way. */
export function sameColumns(a: ColumnSettings, b: ColumnSettings): boolean {
  const order = (settings: ColumnSettings) => orderedColumns(settings.order).map((column) => column.id).join(",");
  const hidden = (settings: ColumnSettings) => [...settings.hidden].sort().join(",");
  const widths = (settings: ColumnSettings) => TRACK_COLUMNS.map((column) => settings.widths[column.id] ?? column.width).join(",");
  return order(a) === order(b) && hidden(a) === hidden(b) && widths(a) === widths(b);
}

/** The columns in the owner's order, with any column the saved order does not know (new since)
 * in its usual place. Every column can move; only the number and the title cannot be hidden. */
export function orderedColumns(order: TrackColumn[]): ColumnSpec[] {
  const known = order.filter(isColumn);
  return [...TRACK_COLUMNS].sort((a, b) => {
    const at = (id: TrackColumn) => { const index = known.indexOf(id); return index === -1 ? TRACK_COLUMNS.findIndex((column) => column.id === id) + known.length : index; };
    return at(a.id) - at(b.id);
  });
}

/** The columns a settings shows, in order, for words: "#, Title / Artist, Release, ...". */
export const describeColumns = (settings: ColumnSettings): string =>
  orderedColumns(settings.order).filter((column) => !settings.hidden.includes(column.id)).map((column) => column.label).join(", ");
