const paths = {
  tracks: "M4 5h16M4 12h16M4 19h10",
  releases: "M4 4h16v16H4zM9 9h6v6H9z",
  artists: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8M20 8v6M17 11h6",
  import: "M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5",
  clock: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 3",
  sync: "M20 7a9 9 0 0 0-15-2L2 8m0-6v6h6M4 17a9 9 0 0 0 15 2l3-3m0 6v-6h-6",
  settings: "M4 7h16M4 17h16M8 4v6M16 14v6",
  search: "M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
  // Its box is centred one unit right of the middle: a triangle centred exactly looks left-heavy.
  play: "m7.5 5 11 7-11 7z",
  pause: "M7 5h3v14H7zM14 5h3v14h-3z",
  close: "m6 6 12 12M6 18 18 6",
  back: "m15 5-7 7 7 7",
  folder: "M3 6h6l2 2h10v12H3z",
  volume: "m11 4-6 5H2v6h3l6 5zM16 8a6 6 0 0 1 0 8M19 4a11 11 0 0 1 0 16",
  next: "m5 5 10 7-10 7zM19 5v14",
  queue: "M3 6h13M3 12h13M3 18h8M16 15v6l5-3z",
  clean: "M12 3l1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6zM18 15l.8 2.2L21 18l-2.2.8L18 21l-.8-2.2L15 18l2.2-.8z",
} as const;

export type IconName = keyof typeof paths;
export function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[name]} /></svg>;
}
