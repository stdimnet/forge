/**
 * Mermaid diagrams of the Markdown preview. Mermaid (MIT) is loaded on first use and renders
 * with `securityLevel: "strict"` (its own DOMPurify pass); the SVG is then shown through an
 * `<img>` data URL, so it can never run scripts or reach the page.
 */
type MermaidApi = typeof import("mermaid").default;

let mermaid: Promise<MermaidApi> | null = null;
let configuredTheme = "";
let seq = 0;
/** Rendering touches global Mermaid state: one diagram at a time. */
let queue: Promise<unknown> = Promise.resolve();
const cache = new Map<string, Promise<string>>();

function load(): Promise<MermaidApi> {
  mermaid ??= import("mermaid").then((m) => m.default);
  return mermaid;
}

/** The diagram as an SVG data URL; rejects with Mermaid's message for invalid diagrams. */
export function renderMermaid(code: string, dark: boolean): Promise<string> {
  const theme = dark ? "dark" : "default";
  const key = `${theme}\n${code}`;
  const cached = cache.get(key);
  if (cached) return cached;
  const job = queue.then(async () => {
    const api = await load();
    if (configuredTheme !== theme) {
      api.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        theme,
        htmlLabels: false,
        flowchart: { htmlLabels: false },
        fontFamily: "Inter, -apple-system, Segoe UI, sans-serif",
      });
      configuredTheme = theme;
    }
    const { svg } = await api.render(`forge-mermaid-${++seq}`, code);
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  });
  queue = job.catch(() => undefined);
  cache.set(key, job);
  // Keep the cache small while typing.
  if (cache.size > 64) cache.delete(cache.keys().next().value!);
  job.catch(() => cache.delete(key));
  return job;
}
