/** "850 ms", "1.4 s": how long a search took. */
export function formatDuration(ms: number): string {
  return ms < 1000 ? `${Math.max(1, Math.round(ms))} ms` : `${(ms / 1000).toFixed(ms < 10000 ? 2 : 1)} s`;
}
