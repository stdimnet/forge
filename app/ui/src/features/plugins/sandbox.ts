/**
 * The documents plugin frames load: the `forge` runtime and the plugin's bundle inline, under
 * a Content Security Policy that allows nothing but those scripts.
 */
import runtimeSource from "./runtime.js?raw";

/** Escape a script for an inline `<script>` element. */
export const inlineScript = (src: string) => src.replace(/<\/(script)/gi, "<\\/$1").replace(/<!--/g, "<\\!--");

export const PLUGIN_CSP = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'";
export const VIEW_CSP = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:";

export function pluginDocument(plugin: { id: string; name: string; version: string }, bundle: string): string {
  const info = JSON.stringify({ id: plugin.id, name: plugin.name, version: plugin.version });
  const runtime = runtimeSource.replaceAll("__FORGE_PLUGIN__", () => info);
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${PLUGIN_CSP}"></head><body>
<script>${inlineScript(runtime)}</script>
<script>${inlineScript(bundle)}</script>
<script>globalThis.__forgeActivate();</script>
</body></html>`;
}

