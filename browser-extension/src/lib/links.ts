/**
 * Find `file:line` references in text: JavaScript stack traces, PHP errors and traces,
 * compiler output and plain `path:12:3`, so selected text or a link can be opened in Forge.
 */

export interface FileReference {
  file: string;
  line?: number;
  column?: number;
}

const EXT = "[A-Za-z0-9]{1,8}";
/** A path or URL ending in a file extension; no spaces, quotes or brackets. */
const PATH = `((?:[a-z][a-z0-9+.-]*:\\/\\/|[A-Za-z]:[\\\\/])?[^\\s()'"<>\\[\\]{},;]*?\\.${EXT})`;

const PATTERNS: { re: RegExp; map: (m: RegExpExecArray) => FileReference }[] = [
  // PHP: "in /var/www/app/User.php on line 42", "in /x.php:42"
  {
    re: new RegExp(`${PATH} on line (\\d+)`, "g"),
    map: (m) => ({ file: m[1], line: Number(m[2]) }),
  },
  // PHP traces "#0 /app/x.php(42): …", TypeScript "src/a.ts(12,5): error"
  {
    re: new RegExp(`${PATH}\\((\\d+)(?:,(\\d+))?\\)`, "g"),
    map: (m) => ({ file: m[1], line: Number(m[2]), column: m[3] ? Number(m[3]) : undefined }),
  },
  // Python: File "/app/x.py", line 12
  {
    re: new RegExp(`File "([^"]+)", line (\\d+)`, "g"),
    map: (m) => ({ file: m[1], line: Number(m[2]) }),
  },
  // Generic and JS stacks: path:12:3, http://localhost:5173/src/App.tsx?t=1:12:3
  {
    re: new RegExp(`${PATH}(?:\\?[^\\s:()'"<>]*)?:(\\d+)(?::(\\d+))?`, "g"),
    map: (m) => ({ file: m[1], line: Number(m[2]), column: m[3] ? Number(m[3]) : undefined }),
  },
];

/** Strip a scheme-less trailing `#L12` (GitHub-style) into a line. */
function fromHashLine(text: string): FileReference | null {
  const m = new RegExp(`^${PATH}(?:\\?[^#\\s]*)?#L(\\d+)(?:C(\\d+))?$`).exec(text.trim());
  return m ? { file: m[1], line: Number(m[2]), column: m[3] ? Number(m[3]) : undefined } : null;
}

/** All references in `text`, in order of appearance (each position reported once). */
export function findFileReferences(text: string): FileReference[] {
  const found: { index: number; end: number; ref: FileReference }[] = [];
  for (const { re, map } of PATTERNS) {
    re.lastIndex = 0;
    for (let m = re.exec(text); m; m = re.exec(text)) {
      const index = m.index;
      const end = index + m[0].length;
      if (found.some((f) => index < f.end && end > f.index)) continue;
      found.push({ index, end, ref: map(m) });
    }
  }
  return found.sort((a, b) => a.index - b.index).map((f) => f.ref);
}

/** The first reference in selected text or a link URL; a bare file path counts too. */
export function firstFileReference(text: string): FileReference | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const hash = fromHashLine(trimmed);
  if (hash) return hash;
  const refs = findFileReferences(trimmed);
  if (refs.length) return refs[0];
  const bare = new RegExp(`^${PATH}$`).exec(trimmed);
  return bare ? { file: bare[1] } : null;
}
