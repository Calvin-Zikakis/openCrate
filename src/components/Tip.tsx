import { useState, type MouseEvent, type ReactNode } from "react";

/** Hover text that shows at once, where the system's own tooltip waits a second. Spread
 * `tipOf(text)` onto any element and render `<Tip tip={tip} />` once near the top of the tree. */
export interface TipState { text: string; x: number; y: number }

export function useTip() {
  const [tip, setTip] = useState<TipState | null>(null);
  const place = (element: HTMLElement, text: string) => {
    const box = element.getBoundingClientRect();
    setTip({ text, x: box.left, y: box.bottom + 4 });
  };
  const tipOf = (text: string) => ({
    "data-tip": text,
    onMouseEnter: (event: MouseEvent<HTMLElement>) => place(event.currentTarget, text),
    onMouseLeave: () => setTip(null),
    onFocus: (event: { currentTarget: HTMLElement }) => place(event.currentTarget, text),
    onBlur: () => setTip(null),
  });
  return { tip, tipOf };
}

export function Tip({ tip }: { tip: TipState | null }): ReactNode {
  if (!tip) return null;
  return <div className="grid-tip" role="tooltip" style={{ left: tip.x, top: tip.y }}>{tip.text}</div>;
}
