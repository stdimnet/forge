const RECENT_BONUS = 30;
const RECENT_STEP = 3;
const SAME_DIR_BONUS = 8;

const dirOf = (path: string) => path.slice(0, path.lastIndexOf("/") + 1);

/** Added to a file's match score: recently opened files, and files next to the one being edited. */
export function recencyBoost(path: string, recent: string[], activePath?: string | null): number {
  const rank = recent.indexOf(path);
  const recency = rank < 0 ? 0 : Math.max(0, RECENT_BONUS - RECENT_STEP * rank);
  const sameDir = activePath && path !== activePath && dirOf(path) === dirOf(activePath) ? SAME_DIR_BONUS : 0;
  return recency + sameDir;
}
