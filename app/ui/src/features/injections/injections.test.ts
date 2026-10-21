import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { invoke } from "@/ipc";
import type { ContextActionsAnswer } from "@/ipc/codeintel";
import type { Fragment, Injection, InjectionRule } from "@/ipc/injections";
import { forgeWasm } from "@/wasm";
import { commands } from "@/state/commands";
import "@/mock/injectionsMock";
import "@/mock/codeintelMock";
import { editFragment, fragmentEditor } from "./fragmentEditor";
import { injectionsField, isInjectionHost, setInjections, injectionAt } from "./highlight";
import { tokenize } from "./languages";
import "./index";

const PHP = `<?php
$rows = $pdo->query('SELECT id, email FROM users WHERE active = 1');
// language=JSON
$config = '{"debug": true}';
preg_match('/^(\\d+)-[a-z]+$/i', $slug, $m);
$name = 'plain';
`;

const ready = async () => {
  await forgeWasm();
  await new Promise((r) => setTimeout(r, 0));
};

describe("language injections", () => {
  it("finds injections in Rust through the browser mock", async () => {
    const list = await invoke<Injection[]>("injections_find", { request: { text: PHP, path: "app/Repo.php", disabled: [], uninjected: [] } });
    expect(list.map((i) => [i.language, i.source])).toEqual([
      ["sql", "rule"],
      ["json", "comment"],
      ["regexp", "rule"],
    ]);
    expect(PHP.slice(list[2].ranges[0].from, list[2].ranges[0].to)).toBe("^(\\d+)-[a-z]+$");
    const off = await invoke<Injection[]>("injections_find", { request: { text: PHP, path: "app/Repo.php", disabled: ["php-sql-calls", "php-sql-keywords"], uninjected: [] } });
    expect(off.map((i) => i.language)).toEqual(["json", "regexp"]);
    const rules = await invoke<InjectionRule[]>("injections_rules");
    expect(rules.find((r) => r.id === "php-regexp")).toMatchObject({ hostLabel: "PHP", languageLabel: "RegExp" });
  });

  it("offers the intentions in Show Context Actions", async () => {
    const at = (needle: string) => {
      const offset = PHP.indexOf(needle);
      const before = PHP.slice(0, offset).split("\n");
      return { line: before.length - 1, character: before[before.length - 1].length };
    };
    const ask = (needle: string) => invoke<ContextActionsAnswer>("code_context_actions", { path: "app/Repo.php", content: PHP, ...at(needle) });
    const sql = (await ask("email")).actions.map((a) => a.title);
    expect(sql).toContain("Edit SQL Fragment");
    expect(sql).toContain("Un-inject Language");
    const plain = (await ask("plain")).actions.map((a) => a.title);
    expect(plain).toContain("Inject language or reference");
  });

  it("colors injected text with the injected language's parser", () => {
    const sql = tokenize("sql", "SELECT id FROM users");
    const kw = sql.filter((t) => t.cls === "cm-inj-keyword").map((t) => "SELECT id FROM users".slice(t.from, t.to));
    expect(kw).toEqual(["SELECT", "FROM"]);
    const re = tokenize("regexp", "^(\\d+)[a-z]*$");
    const classes = new Set(re.map((t) => t.cls));
    expect(classes).toContain("cm-inj-escape");
    expect(classes).toContain("cm-inj-number");
    expect(classes).toContain("cm-inj-keyword");
    expect(tokenize("nope", "x")).toEqual([]);
  });

  it("knows its host files", () => {
    expect(isInjectionHost("app/Repo.php")).toBe(true);
    expect(isInjectionHost("resources/views/a.blade.php")).toBe(false);
    expect(isInjectionHost("src/db.ts")).toBe(true);
    expect(isInjectionHost("README.md")).toBe(false);
  });

  it("syncs the fragment editor back into the string, escaped", async () => {
    await ready();
    const text = "<?php\n$q = 'SELECT * FROM t';\n";
    const host = new EditorView({ state: EditorState.create({ doc: text, extensions: [injectionsField, fragmentEditor()] }), parent: document.body });
    host.dispatch({ selection: { anchor: text.indexOf("FROM") } });
    expect(editFragment(host, "a.php")).toBe(true);
    const inner = EditorView.findFromDOM(host.dom.querySelector(".cm-fragment-panel .cm-editor") as HTMLElement)!;
    expect(inner.state.doc.toString()).toBe("SELECT * FROM t");
    expect(host.dom.querySelector(".cm-fragment-title")?.textContent).toBe("SQL Fragment (a.php)");
    // Typing a quote in the fragment writes an escaped quote into the PHP string.
    inner.dispatch({ changes: { from: inner.state.doc.length, insert: " WHERE name = 'x'" } });
    expect(host.state.doc.toString()).toBe("<?php\n$q = 'SELECT * FROM t WHERE name = \\'x\\'';\n");
    // Editing the string in the host refreshes the fragment.
    host.dispatch({ changes: { from: host.state.doc.toString().indexOf("*"), to: host.state.doc.toString().indexOf("*") + 1, insert: "id" } });
    expect(inner.state.doc.toString()).toBe("SELECT id FROM t WHERE name = 'x'");
    host.destroy();
  });

  it("maps known injections through edits until Rust answers", () => {
    const state = EditorState.create({ doc: "<?php $q = 'SELECT 1';", extensions: [injectionsField] });
    const inj: Injection = {
      language: "sql",
      label: "SQL",
      source: "rule",
      from: 11,
      to: 21,
      ranges: [{ from: 12, to: 20 }],
      literals: [{ from: 11, to: 21, contentFrom: 12, contentTo: 20, kind: "phpSingle" }],
      key: "k",
      text: "SELECT 1",
    };
    const s1 = state.update({ effects: setInjections.of([inj]) }).state;
    const s2 = s1.update({ changes: { from: 0, insert: "\n" } }).state;
    expect(injectionAt(s2, 14)?.ranges[0]).toEqual({ from: 13, to: 21 });
    expect(injectionAt(s2, 5)).toBeUndefined();
  });

  it("implements the actions", async () => {
    for (const id of ["InjectLanguageAction", "UnInjectLanguageAction", "EditInjectedFragment"]) expect(commands().has(id), id).toBe(true);
    const frag = await invoke<Fragment | null>("injections_fragment", {
      request: { text: PHP, path: "app/Repo.php", offset: PHP.indexOf("debug"), disabled: [], uninjected: [] },
    });
    expect(frag?.text).toBe('{"debug": true}');
  });
});
