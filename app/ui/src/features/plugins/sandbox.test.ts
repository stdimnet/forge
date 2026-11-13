import { describe, expect, it } from "vitest";
import { inlineScript, pluginDocument, PLUGIN_CSP } from "./sandbox";

describe("plugin sandbox documents", () => {
  const plugin = { id: "acme.x", name: "X $& </script>", version: "1.0.0" };

  it("inlines the runtime and the bundle under a strict CSP", () => {
    const doc = pluginDocument(plugin, "var __forgePlugin = { activate() {} };");
    expect(doc).toContain(`content="${PLUGIN_CSP}"`);
    expect(PLUGIN_CSP).toContain("default-src 'none'");
    expect(doc).toContain('var PLUGIN = {"id":"acme.x"');
    expect(doc).not.toContain("__FORGE_PLUGIN__");
    expect(doc).toContain("globalThis.__forgeActivate()");
  });

  it("cannot close its script element early", () => {
    const doc = pluginDocument(plugin, 'const s = "</script><img src=x onerror=alert(1)>";');
    // Only the three script elements the host writes are closed.
    expect(doc.match(/<\/script>/g)).toHaveLength(3);
    expect(inlineScript("a</SCRIPT>b<!--c")).toBe("a<\\/SCRIPT>b<\\!--c");
  });

  it("runs the runtime: the forge global posts calls to the host", async () => {
    const posted: unknown[] = [];
    const runtime = pluginDocument(plugin, "").split("<script>")[1].split("</script>")[0];
    const fakeParent = { postMessage: (m: unknown) => posted.push(m) };
    const listeners: ((e: { source: unknown; data: unknown }) => void)[] = [];
    const fakeWindow = { addEventListener: (_: string, l: (typeof listeners)[0]) => listeners.push(l) };
    const scope: Record<string, unknown> = {};
    new Function("parent", "window", "globalThis", runtime)(fakeParent, fakeWindow, scope);
    const forge = scope.forge as { statusBar: { update(id: string, s: object): Promise<unknown> }; apiVersion: number };
    expect(forge.apiVersion).toBe(1);
    const done = forge.statusBar.update("acme.x.status", { text: "3 words" });
    expect(posted[0]).toMatchObject({ forgePlugin: 1, kind: "call", method: "statusBar.update", args: ["acme.x.status", { text: "3 words" }] });
    const id = (posted[0] as { id: number }).id;
    // Messages from anything but the host are ignored.
    listeners[0]({ source: {}, data: { forgeHost: 1, kind: "result", id, value: "forged" } });
    listeners[0]({ source: fakeParent, data: { forgeHost: 1, kind: "result", id, value: "ok" } });
    await expect(done).resolves.toBe("ok");
  });
});
