/** Wraps a cursor around a match list so ↑/↓ cycle through every hit. */
export function nextIndex(current: number, delta: number, total: number): number {
  if (total <= 0) return 0;
  return ((current + delta) % total + total) % total;
}
