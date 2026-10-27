import { Fragment, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { checkValue, FIELD_LABEL, FIELD_OF_NAME, fieldForProblem, RELEASE_WIDE, type ImportEditField } from "../library/importEdits";
import type { PlanFile, TagView } from "../library/types";

type Field = { value: string | null; guessed: boolean; tidiedFrom: string | null; edited?: boolean };

function field(view: TagView | null, name: string): Field {
  const found = view?.fields.find((candidate) => candidate.name === name);
  return { value: found?.value ?? null, guessed: found?.guessed ?? false, tidiedFrom: found?.tidiedFrom ?? null };
}

/** A tag value: a faint dash when missing, a small "?" when it came from the file name, a small
 * "*" when it was tidied (hover shows what it was), and a pencil when it was typed here. */
function Value({ of }: { of: Field }) {
  if (of.value === null) return <span className="tag-missing" title={of.tidiedFrom === null ? "Not in the tags" : `Was "${of.tidiedFrom}"`}>-</span>;
  if (of.edited) return <span className="tag-edited" title="Typed here; goes into the imported copy">{of.value}<sup className="tag-guess-mark" aria-label="typed here">{"\u270E"}</sup></span>;
  const title = of.guessed ? "Not in the tags; taken from the file name" : of.tidiedFrom !== null ? `Tidied; was "${of.tidiedFrom}"` : of.value;
  return <span className={of.guessed ? "tag-guessed" : of.tidiedFrom !== null ? "tag-tidied" : undefined} title={title}>
    {of.value}{of.guessed && <sup className="tag-guess-mark">?</sup>}{of.tidiedFrom !== null && <sup className="tag-guess-mark" aria-label={`tidied, was ${of.tidiedFrom}`}>*</sup>}
  </span>;
}

/** Typing a value in the preview. `count`: files in the release, offered all at once for the
 * fields a release usually shares. Enter or clicking away keeps it, Escape leaves it as it was. */
function CellEditor({ field, initial, count, onSave, onCancel }: {
  field: ImportEditField;
  initial: string;
  count: number;
  onSave: (value: string, all: boolean) => void;
  onCancel: () => void;
}) {
  const offerAll = RELEASE_WIDE.has(field) && count > 1;
  const [text, setText] = useState(initial);
  const [all, setAll] = useState(offerAll);
  const [error, setError] = useState("");
  const box = useRef<HTMLDivElement>(null);
  const done = useRef(false);
  const save = (fromBlur: boolean) => {
    if (done.current) return;
    const problem = checkValue(field, text);
    if (problem) {
      // Clicking away from a value that cannot be used leaves things as they were.
      if (fromBlur) { done.current = true; onCancel(); } else setError(problem);
      return;
    }
    done.current = true;
    onSave(text, all);
  };
  return <div ref={box} className="cell-editor" onBlur={(event) => { if (!box.current?.contains(event.relatedTarget as Node | null)) save(true); }}>
    <input
      autoFocus
      aria-label={`New ${FIELD_LABEL[field]}`}
      aria-invalid={error !== ""}
      inputMode={field === "year" || field === "track" ? "numeric" : undefined}
      value={text}
      onChange={(event) => { setText(event.target.value); setError(""); }}
      onKeyDown={(event) => {
        if (event.key === "Enter") { event.preventDefault(); save(false); }
        if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); done.current = true; onCancel(); }
      }}
    />
    {offerAll && <label className="cell-editor-all"><input type="checkbox" checked={all} onChange={(event) => setAll(event.target.checked)} />{`All ${count} files`}</label>}
    {error && <small role="alert" className="inline-error">{error}</small>}
  </div>;
}

const dash = <span className="tag-missing">-</span>;

/** Every column, in order. Title cannot be hidden. */
export const PLAN_COLUMNS = [
  { id: "track", label: "#", width: 50 },
  { id: "title", label: "Title", width: 240, fixed: true },
  { id: "album", label: "Album", width: 170 },
  { id: "year", label: "Year", width: 64 },
  { id: "genre", label: "Genre", width: 120 },
  { id: "key", label: "Key", width: 64 },
  { id: "bpm", label: "BPM", width: 64 },
  { id: "length", label: "Length", width: 72 },
  { id: "format", label: "Format", width: 120 },
  { id: "becomes", label: "Becomes", width: 90 },
  { id: "destination", label: "Goes to", width: 300 },
] as const;
export type PlanColumn = (typeof PLAN_COLUMNS)[number]["id"];

const MIN_WIDTH = 40;
const MAX_WIDTH = 800;
const DEFAULT_HIDDEN: PlanColumn[] = ["track"];
const HIDDEN_KEY = "opencrate.previewHiddenColumns";
const WIDTHS_KEY = "opencrate.previewColumnWidths";

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

const isIdList = (value: unknown): value is PlanColumn[] =>
  Array.isArray(value) && value.every((id) => PLAN_COLUMNS.some((column) => column.id === id && !("fixed" in column)));
const isWidths = (value: unknown): value is Partial<Record<PlanColumn, number>> =>
  typeof value === "object" && value !== null && !Array.isArray(value) &&
  Object.entries(value).every(([id, width]) => PLAN_COLUMNS.some((column) => column.id === id) && typeof width === "number" && Number.isFinite(width));

const clamp = (width: number) => Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width)));

/** Draws a tag value, as a button that opens the editor when the preview can be typed in. */
type Show = (name: string, edit?: ImportEditField, fallback?: ReactNode) => ReactNode;

function cell(id: PlanColumn, file: PlanFile, show: Show) {
  const tags = file.tags;
  switch (id) {
    case "track": {
      const disc = field(tags, "Disc");
      return <>{Number(disc.value ?? "1") > 1 && <><Value of={disc} />-</>}{show("Track", "track")}</>;
    }
    case "title": {
      const name = <span title={file.source}>{file.source.split("/").at(-1) ?? file.source}</span>;
      return <>
        <strong>{show("Title", "title", field(tags, "Title").value === null ? name : undefined)}</strong>
        <small>{show("Artist", "artist")}</small>
      </>;
    }
    case "album": {
      const albumArtist = field(tags, "Album artist");
      const differs = albumArtist.value !== null && albumArtist.value !== field(tags, "Artist").value;
      return <>{show("Album", "album")}{differs && <small className="plan-sub">{show("Album artist", "albumArtist")}</small>}</>;
    }
    case "year": return show("Year", "year");
    case "genre": return show("Genre", "genre");
    case "key": return <Value of={field(tags, "Key")} />;
    case "bpm": return <Value of={field(tags, "BPM")} />;
    case "length": return tags?.length ?? dash;
    case "format": return tags?.audio ?? dash;
    case "becomes": return file.becomes ?? <span className="tag-missing" title="Goes in as it is">As is</span>;
    case "destination": return file.destination ? <span title={file.destination}>{file.destination}</span> : dash;
  }
}

/** Typing values into the preview: which were typed, and what to do with a new one. */
export interface PlanEditing {
  edited: (source: string, field: ImportEditField) => boolean;
  onEdit: (change: { sources: string[]; field: ImportEditField; value: string }) => void;
}

/** Several releases in one table: each starts with a row of its own (`header`) above its files. */
export interface PlanGroup {
  key: string;
  /** The release's name, for the fold button's label. */
  name: string;
  header: ReactNode;
  files: PlanFile[];
  /** Given, its values can be typed in. */
  editing?: PlanEditing;
}

/** The preview table: drag a heading's edge to resize, right-click a heading to choose columns.
 * Both choices are remembered in this browser. Given `groups`, it lists every release at once.
 * Given `editing` (or a group's own), clicking a value lets it be typed. */
export function PlanTable({ files = [], groups, editing }: { files?: PlanFile[]; groups?: PlanGroup[]; editing?: PlanEditing }) {
  const allFiles = groups ? groups.flatMap((group) => group.files) : files;
  const [hidden, setHidden] = useState<Set<PlanColumn>>(() => new Set(load(HIDDEN_KEY, DEFAULT_HIDDEN, isIdList)));
  const [widths, setWidths] = useState<Partial<Record<PlanColumn, number>>>(() => load(WIDTHS_KEY, {}, isWidths));
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: PlanColumn; startX: number; startWidth: number } | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  /** Columns lie past the right edge: a fade and a hint say so, since a thin scrollbar hides itself. */
  const [moreRight, setMoreRight] = useState(false);
  /** Releases whose rows are folded away, leaving their heading row. */
  const [folded, setFolded] = useState<Set<string>>(new Set());
  /** The value being typed: in its cell, or under the file's problem that asked for it. */
  const [open, setOpen] = useState<{ source: string; field: ImportEditField; where: "cell" | "note" } | null>(null);
  const fold = (key: string) => setFolded((before) => {
    const next = new Set(before);
    if (!next.delete(key)) next.add(key);
    return next;
  });

  // "Becomes" only matters when something in this release would be converted.
  const converting = allFiles.some((file) => file.becomes !== null);
  const shown = (id: PlanColumn) => !hidden.has(id) && (id !== "becomes" || converting);
  const visible = PLAN_COLUMNS.filter((column) => shown(column.id));
  const widthOf = (id: PlanColumn) => clamp(widths[id] ?? PLAN_COLUMNS.find((column) => column.id === id)?.width ?? 100);
  const total = visible.reduce((sum, column) => sum + widthOf(column.id), 0);

  const setWidth = (id: PlanColumn, width: number | null, remember: boolean) => {
    setWidths((before) => {
      const next = { ...before };
      if (width === null) delete next[id]; else next[id] = clamp(width);
      if (remember) save(WIDTHS_KEY, next);
      return next;
    });
  };
  const toggle = (id: PlanColumn) => {
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
  const resizeStart = (id: PlanColumn) => (event: PointerEvent<HTMLSpanElement>) => {
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
  const resizeKey = (id: PlanColumn) => (event: KeyboardEvent<HTMLSpanElement>) => {
    const step = event.shiftKey ? 50 : 10;
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      setWidth(id, widthOf(id) + (event.key === "ArrowRight" ? step : -step), true);
    } else if (event.key === "Home") {
      event.preventDefault();
      setWidth(id, null, true);
    }
  };

  const fileRows = (file: PlanFile, release: PlanFile[], edits: PlanEditing | undefined) => {
    const notes = [...(file.problem ? [file.problem] : []), ...(file.shortened ? ["Name shortened to fit Windows' path limit."] : []), ...(file.tags?.warnings ?? [])];
    const editor = (name: string, edit: ImportEditField) => edits && <CellEditor
      key={`${file.source}/${edit}`}
      field={edit}
      initial={field(file.tags, name).value ?? ""}
      count={release.length}
      onSave={(value, all) => { setOpen(null); edits.onEdit({ sources: all ? release.map((item) => item.source) : [file.source], field: edit, value }); }}
      onCancel={() => setOpen(null)}
    />;
    const show: Show = (name, edit, fallback) => {
      const value = { ...field(file.tags, name), edited: edit !== undefined && edits?.edited(file.source, edit) };
      const drawn = fallback ?? <Value of={value} />;
      if (!edit || !edits) return drawn;
      if (open?.source === file.source && open.field === edit && open.where === "cell") return editor(name, edit);
      return <button type="button" className="tag-edit" title="Click to change it for the import" aria-label={`Change the ${FIELD_LABEL[edit]} of ${file.source}${value.value === null ? "" : `: ${value.value}`}`} onClick={() => setOpen({ source: file.source, field: edit, where: "cell" })}>{drawn}</button>;
    };
    const asked = file.problem && edits ? fieldForProblem(file.problem, field(file.tags, "Title").value !== null) : null;
    const askedName = asked && Object.entries(FIELD_OF_NAME).find(([, id]) => id === asked)?.[0];
    return <Fragment key={file.source}>
      <tr className={notes.length > 0 ? "plan-row has-notes" : "plan-row"}>
        {visible.map((column) => <td key={column.id} className={`plan-cell plan-${column.id}`}>{cell(column.id, file, show)}</td>)}
      </tr>
      {notes.length > 0 && <tr className="plan-notes">
        <td colSpan={visible.length}><ul className={file.problem ? "tag-warnings plan-blocked" : "tag-warnings"} aria-label={`Notes for ${file.source}`}>{notes.map((note) => <li key={note}>
          {note}
          {note === file.problem && asked && askedName && (open?.source === file.source && open.field === asked && open.where === "note"
            ? <span className="note-editor">{editor(askedName, asked)}</span>
            : <> <button type="button" className="link-button" onClick={() => setOpen({ source: file.source, field: asked, where: "note" })}>{`Type the ${FIELD_LABEL[asked]}`}</button></>)}
        </li>)}</ul></td>
      </tr>}
    </Fragment>;
  };
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
    <table className="analysis-table plan-table" style={tableStyle}>
      <colgroup>{visible.map((column) => <col key={column.id} style={{ width: widthOf(column.id) }} />)}</colgroup>
      <thead onContextMenu={(event) => { event.preventDefault(); setMenu({ x: event.clientX, y: event.clientY }); }}>
        <tr>{visible.map((column) => <th key={column.id} tabIndex={0} aria-label={column.label} title="Right-click to choose columns" onKeyDown={headingKey}>
          <span className="plan-heading">{column.label}</span>
          <span
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
          />
        </th>)}</tr>
      </thead>
      {groups
        ? groups.map((group) => <tbody key={group.key} className="plan-group">
          <tr className="plan-group-row" data-release={group.key}><td colSpan={visible.length}><div className="plan-group-head">
            <button type="button" className="plan-fold" aria-expanded={!folded.has(group.key)} aria-label={`${folded.has(group.key) ? "Show" : "Hide"} the files of ${group.name}`} title={folded.has(group.key) ? `Show the ${group.files.length} files` : "Hide the files"} onClick={() => fold(group.key)}><span aria-hidden="true">{folded.has(group.key) ? "\u25B8" : "\u25BE"}</span></button>
            {group.header}
          </div></td></tr>
          {!folded.has(group.key) && group.files.map((file) => fileRows(file, group.files, group.editing))}
        </tbody>)
        : <tbody>{files.map((file) => fileRows(file, files, editing))}</tbody>}
    </table>
    </div>
    {menu && <div ref={menuRef} className="column-menu" role="menu" aria-label="Columns" style={{ left: menu.x, top: menu.y }}>
      {PLAN_COLUMNS.map((column) => {
        const fixed = "fixed" in column || (column.id === "becomes" && !converting);
        return <button key={column.id} type="button" role="menuitemcheckbox" aria-checked={shown(column.id)} aria-disabled={fixed}
          onClick={() => { if (!fixed) toggle(column.id); }}>
          <span className="column-check" aria-hidden="true">{shown(column.id) ? "✓" : ""}</span>{column.id === "track" ? "# (track and disc)" : column.id === "becomes" && !converting ? "Becomes (nothing is converted)" : column.label}
        </button>;
      })}
      <div className="column-menu-divider" />
      <button type="button" role="menuitem" onClick={() => { setWidths({}); save(WIDTHS_KEY, {}); setMenu(null); }}>Reset column widths</button>
    </div>}
    </div>
  </>;
}
