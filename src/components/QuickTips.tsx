import { useEffect, useState } from "react";

/** Hover text for the whole app, shown the moment the pointer arrives: every element with a
 * `title` gets openCrate's own tooltip instead of the system's, which waits about a second. The
 * title is lifted off the element while the pointer is on it (so the system's does not come
 * too) and put back when it leaves, so nothing else changes. Elements that carry `data-tip`
 * show their own and are left alone. */
export function QuickTips() {
  const [tip, setTip] = useState<{ text: string; x: number; y: number; above: boolean } | null>(null);
  useEffect(() => {
    let current: HTMLElement | null = null;
    // A title set again while lifted (a button that says "Looking up..." and then not) replaces
    // the text shown and the one put back; without this the tip and the element kept the old one.
    const retitled = new MutationObserver(() => {
      const text = current?.getAttribute("title");
      if (!current || text === null || text === undefined) return;
      current.dataset.quickTitle = text;
      current.removeAttribute("title");
      setTip((before) => (before && text ? { ...before, text } : null));
    });
    const show = (element: HTMLElement) => {
      const text = element.getAttribute("title");
      if (!text) return;
      current = element;
      element.dataset.quickTitle = text;
      element.removeAttribute("title");
      retitled.observe(element, { attributes: true, attributeFilter: ["title"] });
      const box = element.getBoundingClientRect();
      const above = box.bottom + 60 > window.innerHeight;
      setTip({ text, x: Math.max(8, Math.min(box.left, window.innerWidth - 360)), y: above ? box.top - 4 : box.bottom + 4, above });
    };
    const hide = () => {
      retitled.disconnect();
      if (current?.dataset.quickTitle !== undefined) {
        current.setAttribute("title", current.dataset.quickTitle);
        delete current.dataset.quickTitle;
      }
      current = null;
      setTip(null);
    };
    const over = (event: Event) => {
      const target = event.target instanceof Element ? event.target : null;
      const element = target?.closest<HTMLElement>("[title], [data-quick-title]") ?? null;
      if (element === current) return;
      hide();
      if (element && !element.closest("[data-tip]")) show(element);
    };
    const out = (event: MouseEvent) => {
      if (!current) return;
      const to = event.relatedTarget instanceof Node ? event.relatedTarget : null;
      if (!to || !current.contains(to)) hide();
    };
    document.addEventListener("mouseover", over, true);
    document.addEventListener("mouseout", out, true);
    document.addEventListener("focusin", over, true);
    document.addEventListener("focusout", hide, true);
    window.addEventListener("scroll", hide, true);
    document.addEventListener("mousedown", hide, true);
    return () => {
      document.removeEventListener("mouseover", over, true);
      document.removeEventListener("mouseout", out, true);
      document.removeEventListener("focusin", over, true);
      document.removeEventListener("focusout", hide, true);
      window.removeEventListener("scroll", hide, true);
      document.removeEventListener("mousedown", hide, true);
      hide();
    };
  }, []);
  if (!tip) return null;
  return <div className={`grid-tip quick-tip${tip.above ? " is-above" : ""}`} role="tooltip" style={{ left: tip.x, top: tip.y }}>{tip.text}</div>;
}
