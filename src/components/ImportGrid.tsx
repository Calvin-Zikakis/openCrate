import { Fragment, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type MouseEvent, type PointerEvent, type ReactNode } from "react";
import { RELEASE_FIELDS, TAG_NAMES, sourceOf, tagOf, type Edit, type GridField, type Source } from "../library/importEdits";
import type { PlanFile, TagField } from "../library/types";
import { Icon } from "./Icon";

/** Every column, in order. The icons and the file name are always there; Title cannot be hidden. */
export const GRID_COLUMNS = [
  { id: "state", label: "State and actions", width: 92, fixed: true },
  { id: "file", label: "File", width: 170, fixed: true },
  { id: "release", label: "Release", width: 140 },
  { id: "track", label: "#", width: 44, field: "track" },
  { id: "title", label: "Title", width: 190, field: "title", fixed: true },
  { id: "artist", label: "Artist", width: 140, field: "artist" },
  { id: "album", label: "Album", width: 140, field: "album" },
  { id: "albumArtist", label: "Album artist", width: 130, field: "albumArtist" },
  { id: "label", label: "Label", width: 110, field: "label" },
  { id: "genre", label: "Genre", width: 110, field: "genre" },
  { id: "year", label: "Year", width: 58, field: "year" },
  { id: "key", label: "Key", width: 52 },
  { id: "bpm", label: "BPM", width: 56 },
  { id: "length", label: "Length", width: 60 },
  { id: "format", label: "Format", width: 104 },
  { id: "becomes", label: "Becomes", width: 90 },
  { id: "destination", label: "Goes to", width: 280 },
] as const;
export type GridColumn = (typeof GRID_COLUMNS)[number]["id"];
type Column = (typeof GRID_COLUMNS)[number];

const MIN_WIDTH = 30;
const MAX_WIDTH = 800;
const DEFAULT_HIDDEN: GridColumn[] = ["release", "albumArtist"];
const HIDDEN_KEY = "opencrate.importHiddenColumns";
const WIDTHS_KEY = "opencrate.importColumnWidths";

function load<T>(key: string, fallback: T, valid: (value: unknown) => value is T): T {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "null");
    if (valid(saved)) return saved;
  } catch { /* storage unavailable or unreadable: use the defaults */ }
  return fallback;
}

function save(key: string, value: unknown) {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* not remembered, still applied */ }
}

const isIdList = (value: unknown): value is GridColumn[] =>
  Array.isArray(value) && value.every((id) => GRID_COLUMNS.some((column) => column.id === id && !("fixed" in column)));
const isWidths = (value: unknown): value is Partial<Record<GridColumn, number>> =>
  typeof value === "object" && value !== null && !Array.isArray(value) &&
  Object.entries(value).every(([id, width]) => GRID_COLUMNS.some((column) => column.id === id) && typeof width === "number" && Number.isFinite(width));

const clamp = (width: number) => Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width)));

const dash = <span className="tag-missing">-</span>;
const fileName = (source: string) => source.split("/").at(-1) ?? source;

/** What the marks on a value mean, for tooltips and the legend. */
const SOURCE_NOTE: Record<Source, string> = {
  edited: "You typed this. It is written into the copy when the file is imported.",
  lookedUp: "From the lookup. It is written into the copy when the file is imported.",
  guessed: "Not in the file's tags; taken from the file name. Click to change it.",
  tidied: "Tidied: stray separators or padding were removed.",
  tags: "From the file's tags. Click to change it.",
  missing: "Not in the tags. Click to type it.",
};
const MARK: Partial<Record<Source, string>> = { guessed: "?", tidied: "*", lookedUp: "•", edited: "✎" };

/** A value that cannot be typed: key, BPM, and the file facts. */
function Value({ of }: { of: TagField | undefined }) {
  if (!of || of.value === null) return dash;
  const source = sourceOf(of, false);
  return <span className={`tag-${source}`} title={source === "tags" ? of.value : source === "tidied" ? `Tidied; was "${of.tidiedFrom}"` : SOURCE_NOTE[source]}>
    {of.value}{MARK[source] && <sup className="tag-guess-mark">{MARK[source]}</sup>}
  </span>;
}

/** Moves focus to the same column one row up or down, skipping rows without that cell. */
export function moveFocus(from: HTMLInputElement, step: 1 | -1) {
  const column = from.dataset.column;
  let row = from.closest("tr")?.[step === 1 ? "nextElementSibling" : "previousElementSibling"];
  while (row) {
    const next = row.querySelector<HTMLInputElement>(`input[data-column="${column}"]`);
    if (next) { next.focus(); next.select(); return; }
    row = row[step === 1 ? "nextElementSibling" : "previousElementSibling"];
  }
}

export interface CellProps {
  column: GridColumn;
  label: string;
  value: string;
  source: Source;
  placeholder: string;
  /** Shown instead of the source's note. */
  note?: string;
  disabled?: boolean;
  onCommit: (text: string) => void;
  /** Right-click: offer the value to the whole release. */
  onMenu?: (event: MouseEvent<HTMLInputElement>, text: string) => void;
  /** The id of a datalist of known values. */
  list?: string;
}

/** One value the owner can type over. The text is theirs while the cell has focus; otherwise it
 * follows what the host last said. Enter keeps the value and moves down a row; Escape puts the
 * old value back. */
export function EditableCell({ column, label, value, source, placeholder, note, disabled = false, onCommit, onMenu, list }: CellProps) {
  const [text, setText] = useState(value);
  const [focused, setFocused] = useState(false);
  /** Set by Escape: the blur that follows must not keep what was typed. */
  const dropped = useRef(false);
  useEffect(() => { if (!focused) setText(value); }, [value, focused]);
  const commit = () => { if (text !== value) onCommit(text); };
  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" || event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      commit();
      moveFocus(event.currentTarget, event.key === "ArrowUp" ? -1 : 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      dropped.current = true;
      setText(value);
      event.currentTarget.blur();
    }
  };
  return <span className={`grid-cell is-${source}`} title={note ?? SOURCE_NOTE[source]}>
    <input
      className="grid-input"
      data-column={column}
      list={list}
      aria-label={label}
      value={text}
      placeholder={placeholder}
      disabled={disabled}
      spellCheck={false}
      onChange={(event) => setText(event.target.value)}
      // A click into a cell takes the whole value, as a spreadsheet does, so typing replaces it;
      // the default would leave the caret where the click landed and keep the old text.
      onMouseDown={(event) => {
        if (event.button !== 0 || document.activeElement === event.currentTarget) return;
        event.preventDefault();
        event.currentTarget.focus();
        event.currentTarget.select();
      }}
      onContextMenu={onMenu ? (event) => { event.preventDefault(); onMenu(event, text); } : undefined}
      onFocus={(event) => { setFocused(true); event.target.select(); }}
      onBlur={() => { setFocused(false); if (dropped.current) { dropped.current = false; return; } commit(); }}
      onKeyDown={keys}
    />
    {MARK[source] && <span className="grid-mark" aria-hidden="true">{MARK[source]}</span>}
  </span>;
}

/** What a release's rows can do, each as an icon at the start of every one of its rows. */
export interface GridActions {
  /** The release's state in a few words, for the icon's hover text. */
  status: string;
  lookUp?: { label: string; busy: boolean; onClick: () => void };
  import?: { label: string; onClick: () => void };
  remove?: { label: string; onClick: () => void };
}

/** One release in the grid: a row per file, each with the release's actions at its start. */
export interface GridRelease {
  key: string;
  name: string;
  files: PlanFile[];
  /** Shown in place of the file rows while there are none: reading, downloading, blocked. */
  placeholder: string;
  actions: GridActions;
  /** Something to say about the release (a lookup's answer, what an import did, an error). */
  note: ReactNode;
  /** Values can be typed: the release is finished and not yet imported. */
  editable: boolean;
  /** What the owner typed, by file. */
  edits: ReadonlyMap<string, Edit>;
}

interface Props {
  releases: GridRelease[];
  /** A value was typed for these files of a release. Blank text takes a typed value back. */
  onEdit: (releaseKey: string, sources: string[], field: GridField, text: string) => void;
  /** Values the library already has, offered while typing a genre or a label. */
  suggestions?: { genre: string[]; label: string[] };
}

type CellMenu = { x: number; y: number; release: GridRelease; field: GridField; text: string };

/** The import table: a row per file of every release, with its tags as they would be written.
 * Click a value to change it; right-click one to put it on the whole release. Drag a heading's
 * edge to resize, right-click a heading to choose columns; both are remembered in this browser. */
export function ImportGrid({ releases, onEdit, suggestions }: Props) {
  const allFiles = releases.flatMap((release) => release.files);
  const [hidden, setHidden] = useState<Set<GridColumn>>(() => new Set(load(HIDDEN_KEY, DEFAULT_HIDDEN, isIdList)));
  const [widths, setWidths] = useState<Partial<Record<GridColumn, number>>>(() => load(WIDTHS_KEY, {}, isWidths));
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [cellMenu, setCellMenu] = useState<CellMenu | null>(null);
  /** Hover text for the icons, shown at once: the system's own tooltip takes a second to come. */
  const [tip, setTip] = useState<{ text: string; x: number; y: number } | null>(null);
  const tipOf = (text: string) => ({
    "data-tip": text,
    onMouseEnter: (event: MouseEvent<HTMLElement>) => { const box = event.currentTarget.getBoundingClientRect(); setTip({ text, x: box.left, y: box.bottom + 4 }); },
    onMouseLeave: () => setTip(null),
    onFocus: (event: { currentTarget: HTMLElement }) => { const box = event.currentTarget.getBoundingClientRect(); setTip({ text, x: box.left, y: box.bottom + 4 }); },
    onBlur: () => setTip(null),
  });
  const menuRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: GridColumn; startX: number; startWidth: number } | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  /** Columns lie past the right edge: a fade and a hint say so, since a thin scrollbar hides itself. */
  const [moreRight, setMoreRight] = useState(false);

  // "Becomes" only matters when something would be converted.
  const converting = allFiles.some((file) => file.becomes !== null);
  const shown = (id: GridColumn) => !hidden.has(id) && (id !== "becomes" || converting);
  const visible = GRID_COLUMNS.filter((column) => shown(column.id));
  const widthOf = (id: GridColumn) => clamp(widths[id] ?? GRID_COLUMNS.find((column) => column.id === id)?.width ?? 100);
  const total = visible.reduce((sum, column) => sum + widthOf(column.id), 0);

  const setWidth = (id: GridColumn, width: number | null, remember: boolean) => {
    setWidths((before) => {
      const next = { ...before };
      if (width === null) delete next[id]; else next[id] = clamp(width);
      if (remember) save(WIDTHS_KEY, next);
      return next;
    });
  };
  const toggle = (id: GridColumn) => {
    setHidden((before) => {
      const next = new Set(before);
      if (next.has(id)) next.delete(id); else next.add(id);
      save(HIDDEN_KEY, [...next]);
      return next;
    });
  };

  const anyMenu = menu ?? cellMenu;
  // Opened near the bottom or right edge, a menu moves so all of it stays in the window.
  useLayoutEffect(() => {
    const element = menuRef.current;
    if (!anyMenu || !element) return;
    const box = element.getBoundingClientRect();
    const margin = 8;
    element.style.left = `${Math.max(margin, Math.min(anyMenu.x, window.innerWidth - box.width - margin))}px`;
    element.style.top = `${Math.max(margin, Math.min(anyMenu.y, window.innerHeight - box.height - margin))}px`;
  }, [anyMenu]);

  useEffect(() => {
    if (!anyMenu) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    menuRef.current?.querySelector<HTMLElement>("[role=menuitemcheckbox]:not([aria-disabled=true]), [role=menuitem]")?.focus({ preventScroll: true });
    const close = (event: Event) => {
      if (event instanceof MouseEvent && menuRef.current?.contains(event.target as Node)) return;
      setMenu(null);
      setCellMenu(null);
    };
    const escape = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape") { setMenu(null); setCellMenu(null); } };
    window.addEventListener("mousedown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", escape);
      requestAnimationFrame(() => { if (!document.activeElement || document.activeElement === document.body) opener?.focus({ preventScroll: true }); });
    };
  }, [anyMenu]);

  const openMenuFrom = (element: HTMLElement) => {
    const box = element.getBoundingClientRect();
    setMenu({ x: box.left, y: box.bottom });
  };
  const headingKey = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
      event.preventDefault();
      openMenuFrom(event.currentTarget);
    }
  };
  const resizeStart = (id: GridColumn) => (event: PointerEvent<HTMLSpanElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { id, startX: event.clientX, startWidth: widthOf(id) };
  };
  const resizeMove = (event: PointerEvent<HTMLSpanElement>) => {
    const active = drag.current;
    if (active) setWidth(active.id, active.startWidth + event.clientX - active.startX, false);
  };
  const resizeEnd = () => {
    if (!drag.current) return;
    drag.current = null;
    setWidths((current) => { save(WIDTHS_KEY, current); return current; });
  };
  const resizeKey = (id: GridColumn) => (event: KeyboardEvent<HTMLSpanElement>) => {
    const step = event.shiftKey ? 50 : 10;
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      setWidth(id, widthOf(id) + (event.key === "ArrowRight" ? step : -step), true);
    } else if (event.key === "Home") {
      event.preventDefault();
      setWidth(id, null, true);
    }
  };

  /** What deserves a look about a file, for the state icon's hover text. */
  const doubtsOf = (file: PlanFile | null) => {
    if (!file) return [];
    const doubts = [...(file.tags?.warnings ?? []).filter((warning) => !warning.includes("came from the file name"))];
    if (file.tags?.fields.some((field) => field.guessed)) doubts.push("Some values came from the file name (marked ?).");
    if (file.shortened) doubts.push("Name shortened to fit Windows' path limit.");
    return doubts;
  };

  /** The icons at the start of a row: the state, then the release's actions. */
  const icons = (release: GridRelease, file: PlanFile | null) => {
    const { actions } = release;
    const doubts = doubtsOf(file);
    const duplicate = file && !file.problem ? (file.tags?.warnings ?? []).find((warning) => warning.startsWith("Already in your library")) : undefined;
    const state = file?.problem
      ? <span className="grid-state is-blocked" role="img" aria-label="Cannot be imported yet" tabIndex={0} {...tipOf(`${file.problem} ${actions.status}`.trim())}>!</span>
      : duplicate
        ? <span className="grid-state is-duplicate" role="img" aria-label="Already in your library" tabIndex={0} {...tipOf(`${duplicate} Importing it would make a second copy; Remove from the list leaves it out.`)}>=</span>
      : file && doubts.length > 0
        ? <span className="grid-state is-doubtful" role="img" aria-label="Worth a look" tabIndex={0} {...tipOf(doubts.join(" "))}>?</span>
        : file
          ? <span className="grid-state is-ready" role="img" aria-label="Ready" tabIndex={0} {...tipOf(`Ready to import as shown. ${actions.status}`.trim())}>{"✓"}</span>
          : <span className="grid-state is-waiting" role="img" aria-label={actions.status} tabIndex={0} {...tipOf(actions.status)}><Icon name="clock" size={10} /></span>;
    return <span className="grid-icons">
      {state}
      {actions.lookUp && <button type="button" className="grid-action" disabled={actions.lookUp.busy} aria-label={actions.lookUp.label} {...tipOf(actions.lookUp.label)} onClick={actions.lookUp.onClick}><Icon name="search" size={13} /></button>}
      {actions.import && <button type="button" className="grid-action is-import" aria-label={actions.import.label} {...tipOf(actions.import.label)} onClick={actions.import.onClick}><Icon name="import" size={13} /></button>}
      {actions.remove && <button type="button" className="grid-action" aria-label={actions.remove.label} {...tipOf(actions.remove.label)} onClick={actions.remove.onClick}><Icon name="close" size={12} /></button>}
    </span>;
  };

  const cell = (column: Column, file: PlanFile, release: GridRelease) => {
    const tags = file.tags;
    if ("field" in column) {
      const field = column.field as GridField;
      const tag = tagOf(tags, field);
      const edit = release.edits.get(file.source)?.[field];
      const shownText = edit ?? tag?.value ?? "";
      const source = sourceOf(tag, edit !== undefined);
      const note = source === "tidied" && tag ? `Tidied; was "${tag.tidiedFrom}". Click to change it.` : undefined;
      const canType = release.editable && tags !== null;
      if (!canType) return <span className="grid-static">{tag?.value ?? dash}</span>;
      const forRelease = RELEASE_FIELDS.includes(field) && release.files.length > 1;
      return <EditableCell column={column.id} label={`${column.label === "#" ? "Track" : column.label} of ${fileName(file.source)}`} value={shownText} source={source} note={note}
        placeholder={field === "track" ? "-" : `Add ${TAG_NAMES[field].toLowerCase()}`}
        onCommit={(text) => onEdit(release.key, [file.source], field, text)}
        list={field === "genre" && (suggestions?.genre.length ?? 0) > 0 ? "import-known-genres" : field === "label" && (suggestions?.label.length ?? 0) > 0 ? "import-known-labels" : undefined}
        onMenu={forRelease ? (event, text) => setCellMenu({ x: event.clientX, y: event.clientY, release, field, text }) : undefined} />;
    }
    const fixed = (() => {
      switch (column.id) {
        case "state": return icons(release, file);
        case "file": return <span className="grid-file" title={file.source}>{fileName(file.source)}</span>;
        case "release": return <span title={release.name}>{release.name}</span>;
        case "key": return <Value of={tags?.fields.find((candidate) => candidate.name === "Key")} />;
        case "bpm": return <Value of={tags?.fields.find((candidate) => candidate.name === "BPM")} />;
        case "length": return tags?.length ?? dash;
        case "format": return tags?.audio ?? dash;
        case "becomes": return file.becomes ?? <span className="tag-missing" title="Goes in as it is">As is</span>;
        case "destination": return file.destination ? <span title={file.destination}>{file.destination}</span> : file.problem ? <span className="tag-blocked" title={file.problem}>{file.problem}</span> : dash;
        default: return dash;
      }
    })();
    // The same height as a typed cell, so a row's values sit on one line.
    return <span className="grid-static">{fixed}</span>;
  };

  const rows = (release: GridRelease) => {
    if (release.files.length === 0) {
      // Nothing to show yet: one line with the release's name and what it is waiting for.
      return <tr key={release.key} className="plan-row plan-placeholder">
        {visible.map((column) => <td key={column.id} className={`plan-cell plan-${column.id}`}>
          {column.id === "state" ? icons(release, null) : column.id === "file" ? <span className="grid-static grid-file" title={release.name}>{release.name}</span> : column.id === "title" ? <span className="grid-static tag-missing">{release.placeholder}</span> : null}
        </td>)}
      </tr>;
    }
    return <Fragment key={release.key}>
      {release.files.map((file) => <tr key={file.source} className={`plan-row${file.problem ? " is-blocked" : ""}`}>
        {visible.map((column) => <td key={column.id} className={`plan-cell plan-${column.id}`}>{cell(column, file, release)}</td>)}
      </tr>)}
      {release.note && <tr className="plan-notes"><td colSpan={visible.length}><div className="plan-release-note" role="status" aria-label={`About ${release.name}`}>{release.note}</div></td></tr>}
    </Fragment>;
  };
  const tableStyle: CSSProperties = { width: total };
  const measure = () => {
    const element = wrap.current;
    if (!element) return;
    setMoreRight(element.scrollWidth - element.clientWidth - element.scrollLeft > 1);
    element.style.setProperty("--plan-visible", `${element.clientWidth}px`);
  };
  // The box and the table both change size: with the window, and as columns change or rows arrive.
  useLayoutEffect(() => {
    const element = wrap.current;
    if (!element) return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    if (element.firstElementChild) observer.observe(element.firstElementChild);
    return () => observer.disconnect();
  }, []);
  return <>
    {moreRight && <p className="plan-scroll-hint" role="note">More columns to the right. Right-click a heading to choose columns.</p>}
    <div className="plan-table-frame" data-more-right={moreRight}>
    <div className="plan-table-wrap" ref={wrap} onScroll={measure}>
    <table className="analysis-table plan-table import-grid" style={tableStyle}>
      <colgroup>{visible.map((column) => <col key={column.id} style={{ width: widthOf(column.id) }} />)}</colgroup>
      <thead onContextMenu={(event) => { event.preventDefault(); setMenu({ x: event.clientX, y: event.clientY }); }}>
        <tr>{visible.map((column) => <th key={column.id} tabIndex={0} aria-label={column.label} title="Right-click to choose columns" onKeyDown={headingKey}>
          <span className="plan-heading">{column.id === "state" ? "" : column.label}</span>
          {column.id !== "state" && <span
            className="col-resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label={`Resize ${column.label}`}
            aria-valuemin={MIN_WIDTH}
            aria-valuemax={MAX_WIDTH}
            aria-valuenow={widthOf(column.id)}
            tabIndex={0}
            onPointerDown={resizeStart(column.id)}
            onPointerMove={resizeMove}
            onPointerUp={resizeEnd}
            onPointerCancel={resizeEnd}
            onDoubleClick={() => setWidth(column.id, null, true)}
            onKeyDown={resizeKey(column.id)}
          />}
        </th>)}</tr>
      </thead>
      <tbody>{releases.map(rows)}</tbody>
    </table>
    </div>
    {menu && <div ref={menuRef} className="column-menu" role="menu" aria-label="Columns" style={{ left: menu.x, top: menu.y }}>
      {GRID_COLUMNS.filter((column) => column.id !== "state").map((column) => {
        const fixed = "fixed" in column || (column.id === "becomes" && !converting);
        return <button key={column.id} type="button" role="menuitemcheckbox" aria-checked={shown(column.id)} aria-disabled={fixed}
          onClick={() => { if (!fixed) toggle(column.id); }}>
          <span className="column-check" aria-hidden="true">{shown(column.id) ? "✓" : ""}</span>{column.id === "track" ? "# (track number)" : column.id === "becomes" && !converting ? "Becomes (nothing is converted)" : column.label}
        </button>;
      })}
      <div className="column-menu-divider" />
      <button type="button" role="menuitem" onClick={() => { setWidths({}); save(WIDTHS_KEY, {}); setMenu(null); }}>Reset column widths</button>
    </div>}
    {cellMenu && <div ref={menuRef} className="column-menu" role="menu" aria-label="This value" style={{ left: cellMenu.x, top: cellMenu.y }}>
      <button type="button" role="menuitem" onClick={() => {
        const sources = cellMenu.release.files.filter((file) => file.tags !== null).map((file) => file.source);
        onEdit(cellMenu.release.key, sources, cellMenu.field, cellMenu.text);
        setCellMenu(null);
      }}>{cellMenu.text.trim() === "" ? `Clear ${TAG_NAMES[cellMenu.field].toLowerCase()} you typed on every file of ${cellMenu.release.name}` : `Use "${cellMenu.text}" as the ${TAG_NAMES[cellMenu.field].toLowerCase()} of every file of ${cellMenu.release.name}`}</button>
    </div>}
    </div>
    {tip && <div className="grid-tip" role="tooltip" style={{ left: tip.x, top: tip.y }}>{tip.text}</div>}
    <p className="grid-legend" title="Click any value to change it. Enter moves down a row. Right-click a value to put it on every file of the release.">
      <span className="tag-guessed">?</span> file name <span className="legend-dot">&middot;</span> <span className="tag-tidied">*</span> tidied <span className="legend-dot">&middot;</span> <span className="tag-lookedUp">{"•"}</span> lookup <span className="legend-dot">&middot;</span> <span className="tag-edited">{"✎"}</span> typed <span className="legend-dot">&middot;</span> click a value to change it, right-click to apply it to the whole release
    </p>
  </>;
}
