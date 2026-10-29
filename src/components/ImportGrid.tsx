import { Fragment, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { RELEASE_FIELDS, TAG_NAMES, sharedValue, sourceOf, tagOf, type Edit, type GridField, type Source } from "../library/importEdits";
import type { PlanFile, TagField } from "../library/types";

/** Every column, in order. Title cannot be hidden; the state column is always there. */
export const GRID_COLUMNS = [
  { id: "state", label: "State", width: 30, fixed: true },
  { id: "track", label: "#", width: 48, field: "track" },
  { id: "title", label: "Title", width: 210, field: "title", fixed: true },
  { id: "artist", label: "Artist", width: 150, field: "artist" },
  { id: "album", label: "Album", width: 150, field: "album" },
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
const DEFAULT_HIDDEN: GridColumn[] = ["albumArtist"];
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
function moveFocus(from: HTMLInputElement, step: 1 | -1) {
  const column = from.dataset.column;
  let row = from.closest("tr")?.[step === 1 ? "nextElementSibling" : "previousElementSibling"];
  while (row) {
    const next = row.querySelector<HTMLInputElement>(`input[data-column="${column}"]`);
    if (next) { next.focus(); next.select(); return; }
    row = row[step === 1 ? "nextElementSibling" : "previousElementSibling"];
  }
}

interface CellProps {
  column: GridColumn;
  label: string;
  value: string;
  source: Source;
  placeholder: string;
  /** Shown instead of the source's note. */
  note?: string;
  disabled?: boolean;
  onCommit: (text: string) => void;
}

/** One value the owner can type over. The text is theirs while the cell has focus; otherwise it
 * follows what the host last said. Enter keeps the value and moves down a row; Escape puts the
 * old value back. */
function EditableCell({ column, label, value, source, placeholder, note, disabled = false, onCommit }: CellProps) {
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
      aria-label={label}
      value={text}
      placeholder={placeholder}
      disabled={disabled}
      spellCheck={false}
      onChange={(event) => setText(event.target.value)}
      // A click into a cell takes the whole value, as a spreadsheet does, so typing replaces it;
      // the default would leave the caret where the click landed and keep the old text.
      onMouseDown={(event) => {
        if (document.activeElement === event.currentTarget) return;
        event.preventDefault();
        event.currentTarget.focus();
        event.currentTarget.select();
      }}
      onFocus={(event) => { setFocused(true); event.target.select(); }}
      onBlur={() => { setFocused(false); if (dropped.current) { dropped.current = false; return; } commit(); }}
      onKeyDown={keys}
    />
    {MARK[source] && <span className="grid-mark" aria-hidden="true">{MARK[source]}</span>}
  </span>;
}

/** One release in the grid: a heading row, then a row per file. */
export interface GridRelease {
  key: string;
  name: string;
  header: ReactNode;
  files: PlanFile[];
  /** Values can be typed: the release is finished and not yet imported. */
  editable: boolean;
  /** What the owner typed, by file. */
  edits: ReadonlyMap<string, Edit>;
}

interface Props {
  releases: GridRelease[];
  /** A value was typed for these files of a release. Blank text takes a typed value back. */
  onEdit: (releaseKey: string, sources: string[], field: GridField, text: string) => void;
}

/** The import table: every file of every release, with its tags as they would be written. Click
 * a value to change it. Drag a heading's edge to resize, right-click a heading to choose columns;
 * both are remembered in this browser. A release's rows fold under its heading. */
export function ImportGrid({ releases, onEdit }: Props) {
  const allFiles = releases.flatMap((release) => release.files);
  const [hidden, setHidden] = useState<Set<GridColumn>>(() => new Set(load(HIDDEN_KEY, DEFAULT_HIDDEN, isIdList)));
  const [widths, setWidths] = useState<Partial<Record<GridColumn, number>>>(() => load(WIDTHS_KEY, {}, isWidths));
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: GridColumn; startX: number; startWidth: number } | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  /** Columns lie past the right edge: a fade and a hint say so, since a thin scrollbar hides itself. */
  const [moreRight, setMoreRight] = useState(false);
  const [folded, setFolded] = useState<Set<string>>(new Set());
  const fold = (key: string) => setFolded((before) => {
    const next = new Set(before);
    if (!next.delete(key)) next.add(key);
    return next;
  });

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

  // Opened near the bottom or right edge, the menu moves so all of it stays in the window.
  useLayoutEffect(() => {
    const element = menuRef.current;
    if (!menu || !element) return;
    const box = element.getBoundingClientRect();
    const margin = 8;
    element.style.left = `${Math.max(margin, Math.min(menu.x, window.innerWidth - box.width - margin))}px`;
    element.style.top = `${Math.max(margin, Math.min(menu.y, window.innerHeight - box.height - margin))}px`;
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    menuRef.current?.querySelector<HTMLElement>("[role=menuitemcheckbox]:not([aria-disabled=true])")?.focus({ preventScroll: true });
    const close = (event: Event) => {
      if (event instanceof MouseEvent && menuRef.current?.contains(event.target as Node)) return;
      setMenu(null);
    };
    const escape = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape") setMenu(null); };
    window.addEventListener("mousedown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", escape);
      requestAnimationFrame(() => { if (!document.activeElement || document.activeElement === document.body) opener?.focus({ preventScroll: true }); });
    };
  }, [menu]);

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

  /** The state mark at the start of a row: what stops the file, or what deserves a look. */
  const stateMark = (file: PlanFile) => {
    if (file.problem) return <span className="grid-state is-blocked" role="img" aria-label="Cannot be imported yet" title={file.problem}>!</span>;
    const doubts = [...(file.tags?.warnings ?? []).filter((warning) => !warning.includes("came from the file name"))];
    if (file.tags?.fields.some((field) => field.guessed)) doubts.push("Some values came from the file name.");
    if (doubts.length > 0) return <span className="grid-state is-doubtful" role="img" aria-label="Worth a look" title={doubts.join(" ")}>?</span>;
    return <span className="grid-state is-ready" role="img" aria-label="Ready" title="Ready to import as shown">{"✓"}</span>;
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
      if (!canType) return tag?.value ?? (field === "title" ? <span title={file.source}>{fileName(file.source)}</span> : dash);
      return <EditableCell column={column.id} label={`${column.label === "#" ? "Track" : column.label} of ${fileName(file.source)}`} value={shownText} source={source} note={note}
        placeholder={field === "title" ? fileName(file.source) : field === "track" ? "-" : `Add ${TAG_NAMES[field].toLowerCase()}`}
        onCommit={(text) => onEdit(release.key, [file.source], field, text)} />;
    }
    switch (column.id) {
      case "state": return stateMark(file);
      case "key": return <Value of={tags?.fields.find((candidate) => candidate.name === "Key")} />;
      case "bpm": return <Value of={tags?.fields.find((candidate) => candidate.name === "BPM")} />;
      case "length": return tags?.length ?? dash;
      case "format": return tags?.audio ?? dash;
      case "becomes": return file.becomes ?? <span className="tag-missing" title="Goes in as it is">As is</span>;
      case "destination": return file.destination ? <span title={file.destination}>{file.destination}</span> : file.problem ? <span className="tag-blocked" title={file.problem}>{file.problem}</span> : dash;
      default: return dash;
    }
  };

  /** Values the whole release shares, typed once for every file. */
  const releaseRow = (release: GridRelease) => {
    if (!release.editable || release.files.length < 2) return null;
    const sources = release.files.filter((file) => file.tags !== null).map((file) => file.source);
    return <tr className="plan-row plan-release-row">
      {visible.map((column) => {
        const field = "field" in column ? column.field as GridField : null;
        if (column.id === "title") return <td key={column.id} className="plan-cell plan-title"><span className="grid-whole" title="Typed here, a value goes on every file of the release">Whole release</span></td>;
        if (!field || !RELEASE_FIELDS.includes(field)) return <td key={column.id} className={`plan-cell plan-${column.id}`} />;
        const shared = sharedValue(release.files, field);
        const typedForAll = sources.length > 0 && sources.every((source) => release.edits.get(source)?.[field] !== undefined);
        const typed = typedForAll ? release.edits.get(sources[0]!)?.[field] : undefined;
        const sameTyped = typed !== undefined && sources.every((source) => release.edits.get(source)?.[field] === typed);
        const value = sameTyped ? typed : shared.value;
        const source: Source = sameTyped ? "edited" : shared.mixed ? "missing" : shared.value === "" ? "missing" : "tags";
        return <td key={column.id} className={`plan-cell plan-${column.id}`}>
          <EditableCell column={column.id} label={`${column.label} of every file in ${release.name}`} value={value} source={source}
            note={shared.mixed && !sameTyped ? "The files differ here. Type a value to put it on all of them." : "Goes on every file of the release."}
            placeholder={shared.mixed ? "Mixed" : `Add ${TAG_NAMES[field].toLowerCase()}`}
            onCommit={(text) => onEdit(release.key, sources, field, text)} />
        </td>;
      })}
    </tr>;
  };

  const fileRows = (release: GridRelease) => release.files.map((file) => {
    // Problems and real warnings get a line; the file-name one is said by the "?" marks already.
    const notes = [...(file.shortened ? ["Name shortened to fit Windows' path limit."] : []), ...(file.tags?.warnings ?? []).filter((warning) => !warning.includes("came from the file name"))];
    if (file.problem && !file.destination) notes.unshift(file.problem);
    return <Fragment key={file.source}>
      <tr className={`plan-row${file.problem ? " is-blocked" : ""}${notes.length > 0 ? " has-notes" : ""}`}>
        {visible.map((column) => <td key={column.id} className={`plan-cell plan-${column.id}`}>{cell(column, file, release)}</td>)}
      </tr>
      {notes.length > 0 && <tr className="plan-notes">
        <td colSpan={visible.length}><ul className={file.problem ? "tag-warnings plan-blocked" : "tag-warnings"} aria-label={`Notes for ${file.source}`}>{notes.map((note) => <li key={note}>{note}</li>)}</ul></td>
      </tr>}
    </Fragment>;
  });
  const tableStyle: CSSProperties = { width: total };
  const measure = () => {
    const element = wrap.current;
    if (!element) return;
    setMoreRight(element.scrollWidth - element.clientWidth - element.scrollLeft > 1);
    // Release headings stay inside the visible part of the table, wrapping if they must.
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
    {moreRight && <p className="plan-scroll-hint" role="note">More columns are off to the right: scroll sideways, drag a heading's edge to make a column narrower, or right-click a heading to hide some.</p>}
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
      {releases.map((release) => <tbody key={release.key} className="plan-group">
        <tr className="plan-group-row"><td colSpan={visible.length}><div className="plan-group-head">
          <button type="button" className="plan-fold" aria-expanded={!folded.has(release.key)} aria-label={`${folded.has(release.key) ? "Show" : "Hide"} the files of ${release.name}`} title={folded.has(release.key) ? `Show the ${release.files.length} files` : "Hide the files"} onClick={() => fold(release.key)}><span aria-hidden="true">{folded.has(release.key) ? "▸" : "▾"}</span></button>
          {release.header}
        </div></td></tr>
        {!folded.has(release.key) && releaseRow(release)}
        {!folded.has(release.key) && fileRows(release)}
      </tbody>)}
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
    </div>
    <p className="grid-legend">
      <span className="tag-guessed">?</span> from the file name, <span className="tag-tidied">*</span> tidied, <span className="tag-lookedUp">{"•"}</span> from a lookup, <span className="tag-edited">{"✎"}</span> typed by you.
      Click any value to change it; Enter moves down a row, and the Whole release row puts a value on every file.
    </p>
  </>;
}
