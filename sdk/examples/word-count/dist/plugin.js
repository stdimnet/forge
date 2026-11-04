// example.word-count: built by the Forge plugin SDK
"use strict";
var __forgePlugin = (() => {
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

  // ../../sdk/examples/word-count/src/index.ts
  var index_exports = {};
  __export(index_exports, {
    activate: () => activate,
    count: () => count
  });
  var STATUS = "example.word-count.status";
  var TOOL_WINDOW = "example.word-count.details";
  function count(text, wordsPerMinute = 230) {
    const words = text.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu)?.length ?? 0;
    return {
      words,
      characters: [...text].length,
      lines: text === "" ? 0 : text.split("\n").length,
      minutes: Math.max(1, Math.round(words / Math.max(1, wordsPerMinute)))
    };
  }
  var escape = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
  async function current(editor) {
    if (!editor) return null;
    const onlySelection = await forge.settings.get("example.word-count.countSelection") ?? true;
    const wpm = await forge.settings.get("example.word-count.wordsPerMinute") ?? 230;
    const { from, to } = editor.selection;
    const selected = onlySelection && to > from;
    const text = selected ? editor.text.slice(from, to) : editor.text;
    return { counts: count(text, wpm), scope: selected ? "selection" : "file", file: editor.path.split("/").pop() ?? editor.path };
  }
  async function render(editor) {
    const state = await current(editor);
    if (!state) {
      await forge.statusBar.update(STATUS, { text: "" });
      await forge.toolWindows.setHtml(TOOL_WINDOW, `<p style="color:var(--fg-muted)">Open a file to count its words.</p>`);
      return;
    }
    const { counts, scope, file } = state;
    await forge.statusBar.update(STATUS, {
      text: `${counts.words.toLocaleString()} words`,
      tooltip: `Word Count (${scope}): ${counts.characters.toLocaleString()} characters, ${counts.lines} lines`
    });
    const row = (label, value) => `<tr><td style="color:var(--fg-muted);padding:3px 16px 3px 0">${label}</td><td>${value}</td></tr>`;
    await forge.toolWindows.setHtml(
      TOOL_WINDOW,
      `<h3 style="margin:0 0 8px;font-size:13px">${escape(file)} <span style="color:var(--fg-muted);font-weight:normal">(${scope})</span></h3>
     <table style="border-collapse:collapse">
       ${row("Words", counts.words.toLocaleString())}
       ${row("Characters", counts.characters.toLocaleString())}
       ${row("Lines", counts.lines.toLocaleString())}
       ${row("Reading time", `about ${counts.minutes} min`)}
     </table>`
    );
  }
  function activate(context) {
    context.subscriptions.push(
      forge.commands.register("example.word-count.show", async () => {
        const state = await current(await forge.editor.getActive());
        await forge.window.showMessage(
          state ? `${state.file}: ${state.counts.words} words, ${state.counts.characters} characters (${state.scope})` : "Open a file to count its words."
        );
      }),
      forge.editor.onDidChange((editor) => void render(editor)),
      forge.settings.onDidChange(() => void forge.editor.getActive().then(render))
    );
    void forge.editor.getActive().then(render);
  }
  return __toCommonJS(index_exports);
})();
