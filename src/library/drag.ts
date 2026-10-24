import type { LibraryItem } from "./types";

/** The drag format for library tracks. Only this app's own rows produce it. */
export const TRACKS_MIME = "application/x-opencrate-tracks";
/** The same tracks in full, so the player can show and play them without asking again. */
const ITEMS_MIME = "application/x-opencrate-items";

export function setDraggedTracks(data: DataTransfer, trackIds: number[], label: string, items: LibraryItem[] = []) {
  data.setData(TRACKS_MIME, JSON.stringify(trackIds));
  if (items.length > 0) data.setData(ITEMS_MIME, JSON.stringify(items));
  data.setData("text/plain", label);
  data.effectAllowed = "copyMove";
}

/** The dragged track ids, or null when the drag is not tracks from this app. */
export function draggedTracks(data: DataTransfer): number[] | null {
  try {
    const parsed: unknown = JSON.parse(data.getData(TRACKS_MIME));
    if (Array.isArray(parsed) && parsed.length > 0 && parsed.every((id) => Number.isInteger(id) && id > 0)) return parsed as number[];
  } catch { /* not ours */ }
  return null;
}

/** While dragging, the data is unreadable; only its types can be checked. */
export const carriesTracks = (data: DataTransfer) => Array.from(data.types).includes(TRACKS_MIME);

/** The dragged tracks in full, or null when the drag did not carry them. */
export function draggedItems(data: DataTransfer): LibraryItem[] | null {
  try {
    const parsed: unknown = JSON.parse(data.getData(ITEMS_MIME));
    if (Array.isArray(parsed) && parsed.length > 0 && parsed.every((item) => typeof item === "object" && item !== null && Number.isInteger((item as LibraryItem).id) && typeof (item as LibraryItem).title === "string")) {
      return parsed as LibraryItem[];
    }
  } catch { /* not ours */ }
  return null;
}

/** The drag format for a crate from the sidebar. */
const CRATE_MIME = "application/x-opencrate-crate";

export function setDraggedCrate(data: DataTransfer, crate: { id: number; name: string }) {
  data.setData(CRATE_MIME, JSON.stringify({ id: crate.id, name: crate.name }));
  data.setData("text/plain", crate.name);
  data.effectAllowed = "copy";
}

export const carriesCrate = (data: DataTransfer) => Array.from(data.types).includes(CRATE_MIME);

/** The dragged crate, or null when the drag is not a crate from this app. */
export function draggedCrate(data: DataTransfer): { id: number; name: string } | null {
  try {
    const parsed: unknown = JSON.parse(data.getData(CRATE_MIME));
    if (typeof parsed === "object" && parsed !== null) {
      const { id, name } = parsed as { id: unknown; name: unknown };
      if (Number.isInteger(id) && (id as number) > 0 && typeof name === "string") return { id: id as number, name };
    }
  } catch { /* not ours */ }
  return null;
}
