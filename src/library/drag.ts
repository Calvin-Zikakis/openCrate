/** The drag format for library tracks. Only this app's own rows produce it. */
export const TRACKS_MIME = "application/x-opencrate-tracks";

export function setDraggedTracks(data: DataTransfer, trackIds: number[], label: string) {
  data.setData(TRACKS_MIME, JSON.stringify(trackIds));
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
