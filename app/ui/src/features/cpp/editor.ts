/**
 * CForge in the editor:
 *
 * - completion of CMake commands, their keywords and `${VARIABLES}` in `CMakeLists.txt` and
 *   `*.cmake` files (`cmake_complete`, Rust),
 * - Reformat Code with clang-format for C and C++ files (`cpp.formatter`),
 * - `.clang-format`, `.clang-tidy` and `.clangd` as YAML,
 * - Reload CMake Project after saving a CMake file (`cmake.reloadOnChange`).
 *
 * Syntax highlighting of C, C++ and CMake is bundled (`@codemirror/lang-cpp`, the CMake
 * legacy mode); code intelligence comes from clangd.
 */
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { EditorState, type Extension } from "@codemirror/state";
import { yaml } from "@codemirror/lang-yaml";
import { cppApi } from "@/ipc/cpp";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLanguage } from "@/editor/languages";
import { registerFormatProvider } from "@/features/lsp/format";
import { onFileSaved } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { cmakeState, cppActive, reloadCMake, showCppBalloon } from "./state";

const fileName = (path: string) => path.replace(/\\/g, "/").split("/").pop() ?? path;
export const isCMakePath = (path: string) => fileName(path) === "CMakeLists.txt" || path.endsWith(".cmake");
export const isCppPath = (path: string) => /\.(c|cc|cpp|cxx|c\+\+|h|hh|hpp|hxx|ipp|inl|cu|ixx|cppm)$/i.test(path);

registerLanguage({ id: "clang-config", load: () => yaml() });

// ---- CMake completion ---------------------------------------------------------------------------

function cmakeCompletion() {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!cppActive()) return null;
    const before = ctx.state.doc.sliceString(Math.max(0, ctx.pos - 1), ctx.pos);
    if (!ctx.explicit && !/[\w{ (]/.test(before)) return null;
    const text = ctx.state.doc.toString();
    // Byte offsets in Rust; CMake files are ASCII in practice.
    const answer = await cppApi.complete(text, new TextEncoder().encode(text.slice(0, ctx.pos)).length).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    const from = new TextDecoder().decode(new TextEncoder().encode(text).slice(0, answer.from)).length;
    return {
      from,
      options: answer.items.map((item) => ({
        label: item.label,
        detail: item.detail || undefined,
        type: item.kind === "command" ? "function" : item.kind === "variable" ? "variable" : "keyword",
        apply: item.kind === "command" ? (view, _completion, f, t) => {
          view.dispatch({ changes: { from: f, to: t, insert: item.apply }, selection: { anchor: f + item.apply.length - 1 } });
        } : item.apply,
      })),
      validFor: /^\w*$/,
    };
  };
}

registerEditorExtension(({ path }): Extension | null => {
  if (!isCMakePath(path)) return null;
  const data = [{ autocomplete: cmakeCompletion() }];
  return EditorState.languageData.of(() => data);
});

// ---- Reformat Code ----------------------------------------------------------------------------

registerFormatProvider(async (path, text) => {
  if (!isCppPath(path) || !cppActive()) return null;
  const formatter = getSetting<string>("cpp.formatter", "clangFormat");
  if (formatter === "none") return text;
  if (formatter === "lsp") return null;
  try {
    return (await cppApi.format(path, text)) ?? null;
  } catch (e) {
    showCppBalloon("clang-format", String(e).replace(/^Error: /, ""), 6000);
    return text;
  }
});

// ---- Reload on change -------------------------------------------------------------------------

let reloadTimer: ReturnType<typeof setTimeout> | undefined;

if (typeof document !== "undefined") {
  onFileSaved((path) => {
    if (!cppActive() || !isCMakePath(path) && !/CMake(User)?Presets\.json$/.test(path)) return;
    if (!getSetting<boolean>("cmake.reloadOnChange", true)) return;
    if (cmakeState()?.status === "noCMake") return;
    clearTimeout(reloadTimer);
    reloadTimer = setTimeout(() => void reloadCMake(), 400);
  });
}
