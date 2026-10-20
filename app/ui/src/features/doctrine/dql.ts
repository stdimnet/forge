/**
 * DQL in PHP source, on the UI side: a tokenizer (same rules as the Rust lexer in
 * `packs/phpforge/orm-doctrine/src/dql/lexer.rs`) and a finder for the strings that hold
 * DQL — `createQuery()` arguments and QueryBuilder fragments — used for the injected-language
 * highlighting. Completion, navigation and diagnostics come from the backend.
 */

export type DqlTokenKind =
  | "keyword"
  | "function"
  | "identifier"
  | "string"
  | "number"
  | "parameter"
  | "operator"
  | "comma"
  | "lparen"
  | "rparen"
  | "whitespace"
  | "unknown";

export interface DqlToken {
  kind: DqlTokenKind;
  start: number;
  end: number;
  text: string;
}

export const DQL_KEYWORDS = [
  "ALL", "AND", "ANY", "AS", "ASC", "AVG", "BETWEEN", "BOTH", "BY", "CASE", "COALESCE", "DELETE", "DESC",
  "DISTINCT", "ELSE", "EMPTY", "END", "ESCAPE", "EXISTS", "FALSE", "FROM", "GROUP", "HAVING", "HIDDEN", "IN",
  "INDEX", "INNER", "INSTANCE", "IS", "JOIN", "LEADING", "LEFT", "LIKE", "MEMBER", "NEW", "NOT", "NULL",
  "NULLIF", "OF", "OR", "ORDER", "OUTER", "PARTIAL", "SELECT", "SET", "SOME", "THEN", "TRAILING", "TRUE",
  "UPDATE", "WHEN", "WHERE", "WITH",
];

export const DQL_FUNCTIONS: [string, string][] = [
  ["ABS", "ABS(arithmetic)"],
  ["AVG", "AVG(field)"],
  ["CONCAT", "CONCAT(str1, str2, ...)"],
  ["COUNT", "COUNT([DISTINCT] expr)"],
  ["CURRENT_DATE", "CURRENT_DATE()"],
  ["CURRENT_TIME", "CURRENT_TIME()"],
  ["CURRENT_TIMESTAMP", "CURRENT_TIMESTAMP()"],
  ["DATE_ADD", "DATE_ADD(date, days, unit)"],
  ["DATE_DIFF", "DATE_DIFF(date1, date2)"],
  ["DATE_SUB", "DATE_SUB(date, days, unit)"],
  ["IDENTITY", "IDENTITY(association [, field])"],
  ["LENGTH", "LENGTH(str)"],
  ["LOCATE", "LOCATE(needle, haystack [, offset])"],
  ["LOWER", "LOWER(str)"],
  ["MAX", "MAX(field)"],
  ["MIN", "MIN(field)"],
  ["MOD", "MOD(a, b)"],
  ["SIZE", "SIZE(collection)"],
  ["SQRT", "SQRT(q)"],
  ["SUBSTRING", "SUBSTRING(str, start [, length])"],
  ["SUM", "SUM(field)"],
  ["TRIM", "TRIM([LEADING | TRAILING | BOTH] [char FROM] str)"],
  ["UPPER", "UPPER(str)"],
  ["COALESCE", "COALESCE(expr1, expr2, ...)"],
  ["NULLIF", "NULLIF(expr1, expr2)"],
];

const KW = new Set(DQL_KEYWORDS);
const FN = new Set(DQL_FUNCTIONS.map(([f]) => f));
const DECLARING = new Set(["FROM", "JOIN", "UPDATE", "DELETE"]);

const isIdentStart = (c: string) => /[\p{L}_\\]/u.test(c);
const isIdentChar = (c: string) => /[\p{L}\p{N}_\\.:]/u.test(c);

export function tokenizeDql(dql: string): DqlToken[] {
  const out: DqlToken[] = [];
  let i = 0;
  let prevKw = "";
  while (i < dql.length) {
    const start = i;
    const c = dql[i];
    let j = i + 1;
    let kind: DqlTokenKind;
    if (/\s/.test(c)) {
      while (j < dql.length && /\s/.test(dql[j])) j++;
      kind = "whitespace";
    } else if (c === "'" || c === '"') {
      while (j < dql.length) {
        if (dql[j] === c) {
          if (dql[j + 1] === c) {
            j += 2;
            continue;
          }
          j++;
          break;
        }
        j++;
      }
      kind = "string";
    } else if (/\d/.test(c)) {
      while (j < dql.length && /[\d.]/.test(dql[j])) j++;
      kind = "number";
    } else if (c === ":" && /[\p{L}\p{N}_]/u.test(dql[j] ?? "")) {
      while (j < dql.length && /[\p{L}\p{N}_]/u.test(dql[j])) j++;
      kind = "parameter";
    } else if (c === "?") {
      while (j < dql.length && /\d/.test(dql[j])) j++;
      kind = "parameter";
    } else if (isIdentStart(c)) {
      while (j < dql.length && isIdentChar(dql[j]) && !(dql[j] === ":" && !/\p{L}/u.test(dql[j + 1] ?? ""))) j++;
      const word = dql.slice(start, j).toUpperCase();
      let k = j;
      while (k < dql.length && /\s/.test(dql[k])) k++;
      if (DECLARING.has(prevKw)) kind = "identifier";
      else if (FN.has(word) && (dql[k] === "(" || word.startsWith("CURRENT_"))) kind = "function";
      else if (KW.has(word)) kind = "keyword";
      else kind = "identifier";
    } else if (c === ",") kind = "comma";
    else if (c === "(") kind = "lparen";
    else if (c === ")") kind = "rparen";
    else if ("=<>!+-*/".includes(c)) {
      while (j < dql.length && "=<>".includes(dql[j])) j++;
      kind = "operator";
    } else if ("{}.".includes(c)) kind = "operator";
    else kind = "unknown";
    if (kind === "keyword") prevKw = dql.slice(start, j).toUpperCase();
    else if (kind !== "whitespace") prevKw = "";
    out.push({ kind, start, end: j, text: dql.slice(start, j) });
    i = j;
  }
  return out;
}

export type DqlSpanKind = "keyword" | "function" | "entity" | "alias" | "field" | "parameter" | "string" | "number";

export interface DqlSpan {
  from: number;
  to: number;
  kind: DqlSpanKind;
}

/** Lexical/structural classification: entity names after FROM/JOIN, `alias.field` paths. */
export function classifyDql(dql: string): DqlSpan[] {
  const tokens = tokenizeDql(dql);
  const aliases = new Set<string>();
  const sig = tokens.filter((t) => t.kind !== "whitespace");
  // Identification variables: the identifier after an entity/path in FROM/JOIN/UPDATE.
  sig.forEach((t, i) => {
    if (t.kind !== "keyword" || !DECLARING.has(t.text.toUpperCase())) return;
    let k = i + 2;
    if (sig[k]?.kind === "keyword" && sig[k].text.toUpperCase() === "AS") k++;
    const a = sig[k];
    if (sig[i + 1]?.kind === "identifier" && a?.kind === "identifier" && !a.text.includes(".")) aliases.add(a.text);
  });
  const out: DqlSpan[] = [];
  let prevKw = "";
  for (const t of tokens) {
    if (t.kind === "whitespace") continue;
    const simple: Partial<Record<DqlTokenKind, DqlSpanKind>> = {
      keyword: "keyword",
      function: "function",
      string: "string",
      number: "number",
      parameter: "parameter",
    };
    const k = simple[t.kind];
    if (k) out.push({ from: t.start, to: t.end, kind: k });
    else if (t.kind === "identifier") {
      const isEntity = (DECLARING.has(prevKw) || prevKw === "OF") && (!t.text.includes(".") || t.text.includes("\\"));
      if (isEntity) out.push({ from: t.start, to: t.end, kind: "entity" });
      else {
        let pos = t.start;
        t.text.split(".").forEach((part, n) => {
          if (part) {
            if (n > 0) out.push({ from: pos, to: pos + part.length, kind: "field" });
            else if (aliases.has(part) || t.text.includes(".")) out.push({ from: pos, to: pos + part.length, kind: "alias" });
          }
          pos += part.length + 1;
        });
      }
    }
    prevKw = t.kind === "keyword" ? t.text.toUpperCase() : "";
  }
  return out;
}

// ---------------------------------------------------------------- DQL in PHP source

export type DqlMode = "full" | "expression" | "entity" | "join";

export interface DqlSegment {
  /** Offset in the DQL text. */
  dql: number;
  /** Offset in the PHP source. */
  src: number;
  len: number;
}

export interface DqlSite {
  dql: string;
  mode: DqlMode;
  method: string;
  segments: DqlSegment[];
}

const QB_EXPR = new Set(["select", "addSelect", "where", "andWhere", "orWhere", "having", "andHaving", "orHaving", "orderBy", "addOrderBy", "groupBy", "addGroupBy", "set"]);
const QB_JOIN = new Set(["join", "innerJoin", "leftJoin"]);
const QB_ENTITY = new Set(["from", "update", "delete"]);
const EXPR = new Set(["eq", "neq", "lt", "lte", "gt", "gte", "isNull", "isNotNull", "in", "notIn", "like", "notLike", "between", "andX", "orX", "not", "count", "countDistinct", "max", "min", "avg", "sum", "lower", "upper", "length", "concat", "isMemberOf", "isInstanceOf", "exists"]);

export function dqlMode(method: string, arg: number, object: string): DqlMode | null {
  if (method === "createQuery") return arg === 0 ? "full" : null;
  if (QB_EXPR.has(method)) return ["orderBy", "addOrderBy", "set"].includes(method) && arg !== 0 ? null : "expression";
  if (QB_JOIN.has(method)) return arg === 0 ? "join" : arg === 3 ? "expression" : null;
  if (QB_ENTITY.has(method)) return arg === 0 ? "entity" : null;
  if (EXPR.has(method) && /expr\(\)\s*$/.test(object)) return "expression";
  return null;
}

interface PhpStringLit {
  start: number;
  end: number;
  contentStart: number;
  contentEnd: number;
}

/** String literals of a PHP file, skipping comments. */
export function phpStrings(src: string): PhpStringLit[] {
  const out: PhpStringLit[] = [];
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === "/" && src[i + 1] === "/") {
      i = src.indexOf("\n", i);
      if (i < 0) break;
      continue;
    }
    if (c === "#" && src[i + 1] !== "[") {
      i = src.indexOf("\n", i);
      if (i < 0) break;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const e = src.indexOf("*/", i + 2);
      i = e < 0 ? src.length : e + 2;
      continue;
    }
    if (c === "'" || c === '"') {
      let j = i + 1;
      while (j < src.length && src[j] !== c) j += src[j] === "\\" ? 2 : 1;
      out.push({ start: i, end: Math.min(j + 1, src.length), contentStart: i + 1, contentEnd: Math.min(j, src.length) });
      i = j + 1;
      continue;
    }
    if (c === "<" && src.startsWith("<<<", i)) {
      const m = /^<<<[ \t]*(['"]?)([A-Za-z_]\w*)\1\r?\n/.exec(src.slice(i, i + 80));
      if (m) {
        const bodyStart = i + m[0].length;
        const endRe = new RegExp(`^[ \\t]*${m[2]}\\b`, "m");
        const rest = src.slice(bodyStart);
        const em = endRe.exec(rest);
        const bodyEnd = em ? bodyStart + em.index : src.length;
        const close = em ? bodyStart + em.index + em[0].length : src.length;
        out.push({ start: i, end: close, contentStart: bodyStart, contentEnd: Math.max(bodyStart, bodyEnd - 1) });
        i = close;
        continue;
      }
    }
    i++;
  }
  return out;
}

/** DQL sites (createQuery / QueryBuilder string arguments) of a PHP file. */
export function findDqlSites(src: string): DqlSite[] {
  const strings = phpStrings(src);
  const byStart = new Map(strings.map((s) => [s.start, s]));
  const sites: DqlSite[] = [];
  const seenCalls = new Set<number>();
  for (const s of strings) {
    // Walk back to the call's opening parenthesis.
    let depth = 0;
    let i = s.start - 1;
    let open = -1;
    while (i >= 0 && s.start - i < 4000) {
      const c = src[i];
      if (c === ")" || c === "]") depth++;
      else if ((c === "(" || c === "[") && depth > 0) depth--;
      else if (c === "(") {
        open = i;
        break;
      } else if (c === "[" || c === ";" || c === "{" || c === "}") break;
      else if (c === "'" || c === '"') {
        const lit = strings.find((x) => x.end - 1 === i);
        if (lit) i = lit.start;
      }
      i--;
    }
    if (open < 0 || seenCalls.has(open)) continue;
    const head = src.slice(Math.max(0, open - 200), open);
    const m = /(?:(->|::|\?->)\s*)?([A-Za-z_]\w*)\s*$/.exec(head);
    if (!m) continue;
    const method = m[2];
    const object = head.slice(0, head.length - m[0].length);
    seenCalls.add(open);
    // Split the argument list and collect string pieces per argument.
    let arg = 0;
    let pieces: DqlSegment[] = [];
    let dql = "";
    let k = open + 1;
    depth = 0;
    const flush = () => {
      const mode = dqlMode(method, arg, object);
      if (mode && pieces.length) sites.push({ dql, mode, method, segments: pieces });
      pieces = [];
      dql = "";
    };
    while (k < src.length) {
      const lit = byStart.get(k);
      if (lit) {
        if (depth === 0) {
          pieces.push({ dql: dql.length, src: lit.contentStart, len: lit.contentEnd - lit.contentStart });
          dql += src.slice(lit.contentStart, lit.contentEnd);
        }
        k = lit.end;
        continue;
      }
      const c = src[k];
      if (c === "(" || c === "[") depth++;
      else if ((c === ")" || c === "]") && depth > 0) depth--;
      else if (c === ")") break;
      else if (c === "," && depth === 0) {
        flush();
        arg++;
      } else if (depth === 0) {
        const cls = /^([\w\\]+)::class\b/.exec(src.slice(k, k + 200));
        if (cls && (k === 0 || !/[\w\\$]/.test(src[k - 1]))) {
          dql += cls[1].split("\\").pop();
          k += cls[0].length;
          continue;
        }
      }
      k++;
    }
    flush();
  }
  return sites;
}

/** Map a DQL offset of a site to the PHP source. */
export function dqlToSource(site: DqlSite, offset: number): number | null {
  const seg = site.segments.find((s) => s.dql <= offset && offset <= s.dql + s.len);
  return seg ? seg.src + (offset - seg.dql) : null;
}

/** Map a source offset to the DQL of a site. */
export function sourceToDql(site: DqlSite, offset: number): number | null {
  const seg = site.segments.find((s) => s.src <= offset && offset <= s.src + s.len);
  return seg ? seg.dql + (offset - seg.src) : null;
}

/** Highlight spans of every DQL site, in source offsets. */
export function dqlSourceSpans(src: string): { sites: DqlSite[]; spans: DqlSpan[] } {
  const sites = findDqlSites(src);
  const spans: DqlSpan[] = [];
  for (const site of sites) {
    for (const sp of classifyDql(site.dql)) {
      const from = dqlToSource(site, sp.from);
      const to = dqlToSource(site, sp.to);
      if (from != null && to != null && to > from) spans.push({ from, to, kind: sp.kind });
    }
  }
  spans.sort((a, b) => a.from - b.from);
  return { sites, spans };
}
