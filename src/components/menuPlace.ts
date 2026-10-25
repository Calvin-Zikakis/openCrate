/** Where a small menu opened at (`x`, `y`) goes so it stays inside the window, as the track
 * menu does: near the bottom or right edge it opens up or to the left instead. */
export function inWindow(x: number, y: number, width = 220, height = 130): { left: number; top: number } {
  return {
    left: Math.max(8, Math.min(x, window.innerWidth - width - 8)),
    top: Math.max(8, Math.min(y, window.innerHeight - height - 8)),
  };
}
