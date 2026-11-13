import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { EditorInlayHint } from "@/ipc/lsp";
import "./settingsMock";
import "./inlayHintsMock";

const SRC = ["<?php", "$total = round($sum, 2);", "$lines->sum(fn ($line) => $line->price);"].join("\n");

const hints = (languageId = "php") =>
  invoke<EditorInlayHint[]>("lsp_inlay_hints", { path: "a.php", startLine: 0, endLine: 2, text: SRC, languageId });

describe("inlay hints in browser mode", () => {
  it("groups the demo server's hints and applies the default settings in Rust", async () => {
    const out = await hints();
    const labels = out.map((h) => [h.line, h.label, h.group]);
    // Variable types are off by default; parameter names and return types are on.
    expect(labels).toContainEqual([1, "precision:", "parameterName"]);
    expect(labels).toContainEqual([2, ": float", "returnType"]);
    expect(labels.some(([, l]) => l === ": float" && out.find((h) => h.label === l)?.group === "variableType")).toBe(false);
  });

  it("follows the language switch and the hint settings", async () => {
    await invoke("settings_set", { key: "editor.inlayHints.types", value: true, scope: "user" });
    expect((await hints()).some((h) => h.group === "variableType" && h.line === 1)).toBe(true);
    await invoke("settings_set", { key: "editor.inlayHints.languages.php", value: false, scope: "user" });
    expect(await hints()).toEqual([]);
    await invoke("settings_set", { key: "editor.inlayHints.languages.php", value: null, scope: "user" });
    await invoke("settings_set", { key: "editor.inlayHints.types", value: null, scope: "user" });
  });
});
