import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

export const SIDEBAR_MIN = 180;
export const SIDEBAR_MAX = 420;
export const SIDEBAR_DEFAULT = 218;
const KEY = "opencrate.sidebarWidth";

export const clampSidebar = (width: number) => Math.round(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, width)));

/** The width chosen earlier in this browser, or the default. */
export function loadSidebarWidth(): number {
  try {
    const saved = Number(localStorage.getItem(KEY));
    if (Number.isFinite(saved) && saved > 0) return clampSidebar(saved);
  } catch { /* storage unavailable: use the default */ }
  return SIDEBAR_DEFAULT;
}

function saveSidebarWidth(width: number) {
  try { localStorage.setItem(KEY, String(width)); } catch { /* not remembered, still applied */ }
}

/** The handle on the sidebar's right edge: drag, arrow keys, or double-click to reset. */
export function SidebarResizer({ width, onChange }: { width: number; onChange: (width: number) => void }) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const set = (next: number, remember: boolean) => {
    const bounded = clampSidebar(next);
    onChange(bounded);
    if (remember) saveSidebarWidth(bounded);
  };
  return <div
    className={`sidebar-resizer${dragging ? " is-dragging" : ""}`}
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize sidebar"
    aria-valuemin={SIDEBAR_MIN}
    aria-valuemax={SIDEBAR_MAX}
    aria-valuenow={width}
    tabIndex={0}
    title="Drag to resize; double-click to reset"
    onPointerDown={(event: PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      start.current = { x: event.clientX, width };
      setDragging(true);
    }}
    onPointerMove={(event) => { if (start.current) set(start.current.width + event.clientX - start.current.x, false); }}
    onPointerUp={(event) => {
      if (!start.current) return;
      set(start.current.width + event.clientX - start.current.x, true);
      start.current = null;
      setDragging(false);
    }}
    onPointerCancel={() => { start.current = null; setDragging(false); }}
    onDoubleClick={() => set(SIDEBAR_DEFAULT, true)}
    onKeyDown={(event: KeyboardEvent<HTMLDivElement>) => {
      const step = event.shiftKey ? 40 : 10;
      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        set(width + (event.key === "ArrowRight" ? step : -step), true);
      } else if (event.key === "Home") {
        event.preventDefault();
        set(SIDEBAR_DEFAULT, true);
      }
    }}
  />;
}
