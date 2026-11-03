import { useEffect, useId, useMemo, useState, type InputHTMLAttributes, type KeyboardEvent, type Ref } from "react";
import { rank } from "../library/suggest";

interface Props {
  /** Values the library already holds. With none, the input is an ordinary text field. */
  options: readonly string[];
  value: string;
  onChange: (text: string) => void;
  /** A row was taken, with Enter or a click. */
  onPick: (text: string) => void;
  /** Everything else the input needs. Its own key, blur and change handlers run after the list's. */
  inputProps?: Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "list" | "role"> & { "data-column"?: string };
  /** The input itself, for a caller that focuses it from a shortcut. */
  inputRef?: Ref<HTMLInputElement>;
}

/** Rows visible at once; Down past the last scrolls the list on. */
const VISIBLE = 5;

/** A text field with the library's values offered underneath as they match what is typed. The
 * list opens on typing, Down and Up move through it, Enter takes a row, Escape closes it and Tab
 * leaves with the text as typed. With the list closed every key reaches the input's own handler,
 * so a grid cell still moves between rows. */
export function Suggest({ options, value, onChange, onPick, inputProps = {}, inputRef }: Props) {
  const listId = useId();
  const rows = useMemo(() => rank(value, options), [value, options]);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const shown = open && rows.length > 0;
  useEffect(() => {
    if (!shown || active < 0) return;
    document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: "nearest" });
  }, [shown, active, listId]);
  if (options.length === 0) {
    return <input {...inputProps} ref={inputRef} value={value} onChange={(event) => onChange(event.target.value)} />;
  }
  const close = () => { setOpen(false); setActive(-1); };
  const pick = (text: string) => { close(); onPick(text); };
  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    if (shown) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : -1;
        setActive((before) => Math.min(rows.length - 1, Math.max(0, before + step)));
        return;
      }
      if (event.key === "Enter" && active >= 0) {
        event.preventDefault();
        pick(rows[active]!.value);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        close();
        return;
      }
      if (event.key === "Tab" || event.key === "Enter") close();
    }
    inputProps.onKeyDown?.(event);
  };
  return <span className="suggest">
    <input
      {...inputProps}
      ref={inputRef}
      role="combobox"
      aria-expanded={shown}
      aria-controls={listId}
      aria-autocomplete="list"
      aria-activedescendant={shown && active >= 0 ? `${listId}-${active}` : undefined}
      value={value}
      onChange={(event) => { onChange(event.target.value); setOpen(true); setActive(-1); }}
      onKeyDown={keys}
      onBlur={(event) => { close(); inputProps.onBlur?.(event); }}
    />
    {shown && <ul className="suggest-list" role="listbox" id={listId} style={{ maxHeight: `${VISIBLE * 26 + 8}px` }}>
      {rows.map((row, index) => <li
        key={row.value}
        id={`${listId}-${index}`}
        role="option"
        aria-selected={index === active}
        className={`suggest-row${index === active ? " is-active" : ""}`}
        // Mouse down rather than click, so the input keeps focus and no blur runs first.
        onMouseDown={(event) => { event.preventDefault(); pick(row.value); }}
        onMouseEnter={() => setActive(index)}
      >{row.value}{row.nearMiss && <span className="suggest-note">your library's spelling</span>}</li>)}
    </ul>}
  </span>;
}
