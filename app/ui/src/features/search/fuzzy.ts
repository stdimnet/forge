/**
 * PHPStorm-style fuzzy matcher with camel-hump support ("OrCo" → OrderController).
 * TypeScript twin of `crates/forge-core/src/search.rs`; keep the scoring in sync.
 */

const BONUS_START = 8;
const BONUS_SEPARATOR = 6;
const BONUS_CAMEL = 6;
const BONUS_CONSECUTIVE = 4;
const BONUS_CASE = 1;
const PENALTY_GAP = 1;
const PENALTY_HUMP_MISS = 2;
const NEG = -1e9;

export interface FuzzyMatch {
  score: number;
  /** Matched character indices into the candidate, for highlighting. */
  positions: number[];
}

const isSeparator = (c: string) => "/\\_-. :$".includes(c);
const isUpper = (c: string) => c !== c.toLowerCase() && c === c.toUpperCase();
const isLowerOrDigit = (c: string) => (c !== c.toUpperCase() && c === c.toLowerCase()) || /\d/.test(c);

function positionBonus(prev: string | undefined, cur: string): number {
  if (prev === undefined) return BONUS_START;
  if (isSeparator(prev)) return BONUS_SEPARATOR;
  if (isLowerOrDigit(prev) && isUpper(cur)) return BONUS_CAMEL;
  return 0;
}

export interface FuzzyOptions {
  /**
   * PHPStorm-style word matching: after the first character, a match may only jump ahead
   * to a word start (camel hump or after a separator). "line" then finds "Move Line Up"
   * but not "Collapse Recursively".
   */
  strict?: boolean;
}

/** Best in-order, case-insensitive alignment of `query` in `candidate`, or null. */
export function fuzzyMatch(query: string, candidate: string, opts: FuzzyOptions = {}): FuzzyMatch | null {
  const q = [...query].filter((c) => !/\s/.test(c));
  const c = [...candidate];
  const m = q.length;
  const n = c.length;
  if (m === 0) return { score: 0, positions: [] };
  if (m > n) return null;
  const ql = q.map((x) => x.toLowerCase());
  const cl = c.map((x) => x.toLowerCase());
  const bonus = c.map((ch, j) => positionBonus(j > 0 ? c[j - 1] : undefined, ch));

  const dp: number[][] = Array.from({ length: m }, () => new Array<number>(n).fill(NEG));
  const from: number[][] = Array.from({ length: m }, () => new Array<number>(n).fill(-1));
  for (let i = 0; i < m; i++) {
    let best = NEG;
    let bestAt = -1;
    for (let j = 0; j < n; j++) {
      if (i > 0 && j >= 2 && dp[i - 1][j - 2] > best) {
        best = dp[i - 1][j - 2];
        bestAt = j - 2;
      }
      if (ql[i] !== cl[j]) continue;
      let s = 1 + bonus[j];
      if (q[i] === c[j] && isUpper(q[i])) s += BONUS_CASE;
      if (isUpper(q[i]) && bonus[j] === 0) s -= PENALTY_HUMP_MISS;
      if (i === 0) {
        dp[i][j] = s;
        continue;
      }
      // A run into a word start earns the boundary bonus, not the run bonus too.
      const consecutive = j >= 1 && dp[i - 1][j - 1] > NEG ? dp[i - 1][j - 1] + (bonus[j] > 0 ? 0 : BONUS_CONSECUTIVE) : NEG;
      const gapped = best > NEG && !(opts.strict && bonus[j] === 0) ? best - PENALTY_GAP : NEG;
      if (consecutive === NEG && gapped === NEG) continue;
      if (consecutive >= gapped) {
        dp[i][j] = s + consecutive;
        from[i][j] = j - 1;
      } else {
        dp[i][j] = s + gapped;
        from[i][j] = bestAt;
      }
    }
  }
  let end = -1;
  let score = NEG;
  dp[m - 1].forEach((s, j) => {
    if (s > score) {
      score = s;
      end = j;
    }
  });
  if (score === NEG) return null;
  const positions = new Array<number>(m);
  for (let i = m - 1, j = end; i >= 0; i--) {
    positions[i] = j;
    j = from[i][j];
  }
  return { score, positions };
}

/** Rank `items` by fuzzy score of `text(item)`; non-matching items are dropped. */
export function fuzzyFilter<T>(
  query: string,
  items: T[],
  text: (item: T) => string,
  limit = Infinity,
  opts: FuzzyOptions = {},
): { item: T; match: FuzzyMatch }[] {
  const out: { item: T; match: FuzzyMatch; len: number }[] = [];
  for (const item of items) {
    const t = text(item);
    const match = fuzzyMatch(query, t, opts);
    if (match) out.push({ item, match, len: t.length });
  }
  out.sort((a, b) => b.match.score - a.match.score || a.len - b.len);
  return out.slice(0, limit).map(({ item, match }) => ({ item, match }));
}

/** Score a file path the way the Rust backend does (name, or full path if the query has "/"). */
export function matchFilePath(query: string, path: string): (FuzzyMatch & { matchedPath: boolean }) | null {
  const name = path.split("/").pop() ?? path;
  const matchedPath = query.includes("/");
  const target = matchedPath ? path : name;
  const m = fuzzyMatch(query, target);
  if (!m) return null;
  const tiebreak = Math.floor([...target].length / 8) + Math.floor((path.split("/").length - 1) / 3);
  return { score: m.score - tiebreak, positions: m.positions, matchedPath };
}
