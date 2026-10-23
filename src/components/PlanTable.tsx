import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import type { PlanFile, TagView } from "../library/types";

type Field = { value: string | null; guessed: boolean; tidiedFrom: string | null };

function field(view: TagView | null, name: string): Field {
  const found = view?.fields.find((candidate) => candidate.name === name);
  return { value: found?.value ?? null, guessed: found?.guessed ?? false, tidiedFrom: found?.tidiedFrom ?? null };
}

/** A tag value: a faint dash when missing, a small "?" when it came from the file name, and a
 * small "*" when it was tidied (hover shows what it was). */
function Value({ of }: { of: Field }) {
  if (of.value === null) return <span className="tag-missing" title={of.tidiedFrom === null ? "Not in the tags" : `Was "${of.tidiedFrom}"`}>-</span>;
  const title = of.guessed ? "Not in the tags; taken from the file name" : of.tidiedFrom !== null ? `Tidied; was "${of.tidiedFrom}"` : of.value;
  return <span className={of.guessed ? "tag-guessed" : of.tidiedFrom !== null ? "tag-tidied" : undefined} title={title}>
    {of.value}{of.guessed && <sup className="tag-guess-mark">?</sup>}{of.tidiedFrom !== null && <sup className="tag-guess-mark" aria-label={`tidied, was ${of.tidiedFrom}`}>*</sup>}
  </span>;
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

function cell(id: PlanColumn, file: PlanFile) {
  const tags = file.tags;
  switch (id) {
    case "track": {
      const disc = field(tags, "Disc");
      return <>{Number(disc.value ?? "1") > 1 && <><Value of={disc} />-</>}<Value of={field(tags, "Track")} /></>;
    }
    case "title": {
      const title = field(tags, "Title");
      return <>
        {title.value === null ? <strong title={file.source}>{file.source.split("/").at(-1) ?? file.source}</strong> : <strong><Value of={title} /></strong>}
        <small><Value of={field(tags, "Artist")} /></small>
      </>;
    }
    case "album": {
      const albumArtist = field(tags, "Album artist");
      const differs = albumArtist.value !== null && albumArtist.value !== field(tags, "Artist").value;
      return <><Value of={field(tags, "Album")} />{differs && <small className="plan-sub"><Value of={albumArtist} /></small>}</>;
    }
    case "year": return <Value of={field(tags, "Year")} />;
    case "genre": return <Value of={field(tags, "Genre")} />;
    case "key": return <Value of={field(tags, "Key")} />;
    case "bpm": return <Value of={field(tags, "BPM")} />;
    case "length": return tags?.length ?? dash;
    case "format": return tags?.audio ?? dash;
    case "becomes": return file.becomes ?? <span className="tag-missing" title="Goes in as it is">As is</span>;
    case "destination": return file.destination ? <span title={file.destination}>{file.destination}</span> : dash;
  }
}

/** The preview table: drag a heading's edge to resize, right-click a heading to choose columns.
 * Both choices are remembered in this browser. */
export function PlanTable({ files }: { files: PlanFile[] }) {
  const [hidden, setHidden] = useState<Set<PlanColumn>>(() => new Set(load(HIDDEN_KEY, DEFAULT_HIDDEN, isIdList)));
  const [widths, setWidths] = useState<Partial<Record<PlanColumn, number>>>(() => load(WIDTHS_KEY, {}, isWidths));
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: PlanColumn; startX: number; startWidth: number } | null>(null);

  // "Becomes" only matters when something in this release would be converted.
  const converting = files.some((file) => file.becomes !== null);
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

  useEffect(() => {
    if (!menu) return;
    menuRef.current?.querySelector<HTMLElement>("[role=menuitemcheckbox]:not([aria-disabled=true])")?.focus();
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

  const tableStyle: CSSProperties = { width: total };
  return <div className="plan-table-wrap">
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
      <tbody>{files.map((file) => {
        const notes = [...(file.problem ? [file.problem] : []), ...(file.shortened ? ["Name shortened to fit Windows' path limit."] : []), ...(file.tags?.warnings ?? [])];
        return [
          <tr key={file.source} className={notes.length > 0 ? "plan-row has-notes" : "plan-row"}>
            {visible.map((column) => <td key={column.id} className={`plan-cell plan-${column.id}`}>{cell(column.id, file)}</td>)}
          </tr>,
          notes.length > 0 && <tr key={`${file.source}:notes`} className="plan-notes">
            <td colSpan={visible.length}><ul className={file.problem ? "tag-warnings plan-blocked" : "tag-warnings"} aria-label={`Notes for ${file.source}`}>{notes.map((note) => <li key={note}>{note}</li>)}</ul></td>
          </tr>,
        ];
      })}</tbody>
    </table>
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
  </div>;
}
