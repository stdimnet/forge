/**
 * PHPUnit test detection in PHP source (for gutter run icons and "Run Context
 * Configuration") and the temporary configurations created from it.
 */
import type { RunConfiguration } from "@/ipc/run";

export interface TestTarget {
  kind: "class" | "method";
  /** Fully qualified class name. */
  fqcn: string;
  className: string;
  method?: string;
  /** 0-based line of the declaration. */
  line: number;
}

const CLASS_RE = /^\s*(?:(?:final|abstract|readonly)\s+)*class\s+(\w+)(?:\s+extends\s+([\\\w]+))?/;
const METHOD_RE = /^\s*(?:(?:public|protected|private|static|final)\s+)*function\s+(\w+)\s*\(/;

/** Test classes and methods in a PHP file. */
export function findTests(text: string): TestTarget[] {
  const lines = text.split("\n");
  const ns = /^\s*namespace\s+([\w\\]+)\s*;/m.exec(text)?.[1];
  const out: TestTarget[] = [];
  let cls: TestTarget | null = null;
  lines.forEach((line, i) => {
    const c = CLASS_RE.exec(line);
    if (c) {
      const [, name, parent] = c;
      const isTest = /Test$/.test(name) || /TestCase$/.test(parent ?? "");
      const abstract = /\babstract\b/.test(line);
      cls = isTest && !abstract ? { kind: "class", fqcn: ns ? `${ns}\\${name}` : name, className: name, line: i } : null;
      if (cls) out.push(cls);
      return;
    }
    if (!cls) return;
    const m = METHOD_RE.exec(line);
    if (!m || !/\bpublic\b|^\s*function/.test(line)) return;
    const name = m[1];
    // Attributes and the doc comment directly above the method.
    let j = i - 1;
    while (j >= 0 && /^\s*(#\[|\/\*\*|\*|$)/.test(lines[j])) j--;
    const before = lines.slice(j + 1, i).join("\n");
    const annotated = /#\[\s*(?:\\?PHPUnit\\Framework\\Attributes\\)?Test\s*\]/.test(before) || /@test\b/.test(before);
    if (/^test/.test(name) || annotated) {
      out.push({ kind: "method", fqcn: cls.fqcn, className: cls.className, method: name, line: i });
    }
  });
  return out;
}

/** The test at a 0-based line: the last declaration at or above it, else the class. */
export function testAt(tests: TestTarget[], line: number): TestTarget | undefined {
  let found: TestTarget | undefined;
  for (const t of tests) if (t.line <= line) found = t;
  return found ?? tests.find((t) => t.kind === "class");
}

/** Temporary PHPUnit configuration for a test class or method. */
export function testConfiguration(target: TestTarget, path: string): RunConfiguration {
  return {
    name: target.kind === "method" ? `${target.className}::${target.method}` : target.className,
    type: "phpunit",
    options:
      target.kind === "method"
        ? { scope: "method", class: target.fqcn, method: target.method!, file: path }
        : { scope: "class", class: target.fqcn, file: path },
  };
}

/** Temporary PHP Script configuration for a plain PHP file. */
export function scriptConfiguration(path: string): RunConfiguration {
  return { name: path.split("/").pop() ?? path, type: "php-script", options: { file: path } };
}
