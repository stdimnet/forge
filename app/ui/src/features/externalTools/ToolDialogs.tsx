/**
 * The Create Tool / Edit Tool dialog of Settings | Tools | External Tools and its
 * "Insert Macro" dialog (macros with descriptions and a preview of their value for the
 * current editor, computed in Rust). Validation (name, program, output filters) is Rust's
 * `external_tool_validate`.
 */
import { createResource, createSignal, For, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, FolderOpen, Plus } from "lucide-solid";
import { isTauri } from "@/ipc";
import { runToolsApi, type ExternalTool, type MacroInfo } from "@/ipc/runTools";
import { workspace } from "@/state/workspace";
import { currentContext } from "./run";
import { DEFAULT_GROUP } from "./model";

/** Filter macros, as the Output filters hint lists them. */
const FILTER_MACROS = ["$FILE_PATH$", "$LINE$", "$COLUMN$"];

function InsertMacroDialog(props: { onInsert: (name: string) => void; onClose: () => void }) {
  const [macros] = createResource(() => runToolsApi.macros(currentContext()).catch(() => [] as MacroInfo[]));
  const [sel, setSel] = createSignal(0);
  const current = () => macros()?.[sel()];
  let list!: HTMLDivElement;
  onMount(() => list.focus());
  const insert = () => {
    const m = current();
    if (m) props.onInsert(m.name);
  };
  return (
    <div class="popup-backdrop modal et-modal" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div
        class="popup et-dialog et-macros"
        role="dialog"
        aria-label="Macros"
        onKeyDown={(e) => {
          const n = macros()?.length ?? 0;
          if (e.key === "Escape") props.onClose();
          else if (e.key === "Enter") insert();
          else if (e.key === "ArrowDown" && n) setSel((s) => Math.min(n - 1, s + 1));
          else if (e.key === "ArrowUp" && n) setSel((s) => Math.max(0, s - 1));
          else return;
          e.preventDefault();
          e.stopPropagation();
          list.querySelector(".et-macro.selected")?.scrollIntoView({ block: "nearest" });
        }}
      >
        <div class="et-title">Macros</div>
        <div class="et-macro-list" tabIndex={0} ref={list}>
          <For each={macros() ?? []}>
            {(m, i) => (
              <div class="et-macro" classList={{ selected: sel() === i() }} onClick={() => setSel(i())} onDblClick={insert}>
                <span class="et-macro-name">{m.name}</span>
                <span class="et-macro-desc">{m.description}</span>
              </div>
            )}
          </For>
        </div>
        <div class="et-preview-label">Macro preview:</div>
        <div class="et-preview" data-testid="et-macro-preview">
          {current()?.preview ?? (current()?.name === "$Prompt$" ? "Asks for a value when the tool runs" : "No value here")}
        </div>
        <div class="dialog-buttons et-buttons">
          <span class="et-spacer" />
          <button class="btn-primary" disabled={!current()} onClick={insert}>
            Insert
          </button>
          <button class="btn-secondary" onClick={props.onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

async function browse(directory: boolean): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory, multiple: false, defaultPath: workspace()?.root });
  return typeof picked === "string" ? picked : null;
}

type MacroField = "program" | "arguments" | "workingDirectory";

/** Create Tool / Edit Tool. Resolves `onDone` with the tool, or null when cancelled. */
export function EditToolDialog(props: {
  tool: ExternalTool;
  creating: boolean;
  groups: string[];
  others: ExternalTool[];
  onDone: (tool: ExternalTool | null) => void;
}) {
  const [tool, setTool] = createSignal<ExternalTool>({ ...props.tool, outputFilters: [...props.tool.outputFilters] });
  const [advanced, setAdvanced] = createSignal(true);
  const [problems, setProblems] = createSignal<string[]>([]);
  const [macroFor, setMacroFor] = createSignal<MacroField | null>(null);
  const inputs: Partial<Record<MacroField, HTMLInputElement>> = {};
  const set = (patch: Partial<ExternalTool>) => setTool((t) => ({ ...t, ...patch }));

  const insertMacro = (field: MacroField, name: string) => {
    const el = inputs[field];
    const value = tool()[field];
    const at = el?.selectionStart ?? value.length;
    const end = el?.selectionEnd ?? at;
    set({ [field]: value.slice(0, at) + name + value.slice(end) } as Partial<ExternalTool>);
    setMacroFor(null);
    queueMicrotask(() => {
      el?.focus();
      el?.setSelectionRange(at + name.length, at + name.length);
    });
  };

  const ok = async () => {
    const t = {
      ...tool(),
      name: tool().name.trim(),
      group: tool().group.trim() || DEFAULT_GROUP,
      outputFilters: tool()
        .outputFilters.map((f) => f.trim())
        .filter(Boolean),
    };
    const found = await runToolsApi.validate(t, props.others).catch((err) => [String(err)]);
    setProblems(found);
    if (!found.length) props.onDone(t);
  };

  const MacroButton = (p: { field: MacroField }) => (
    <button class="btn-secondary et-macro-btn" title="Insert Macro…" onClick={() => setMacroFor(p.field)}>
      <Plus size={13} /> Insert Macro…
    </button>
  );

  return (
    <div class="popup-backdrop modal et-modal">
      <div
        class="popup et-dialog"
        role="dialog"
        aria-label={props.creating ? "Create Tool" : "Edit Tool"}
        onKeyDown={(e) => {
          if (macroFor()) return;
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            props.onDone(null);
          } else if (e.key === "Enter" && !(e.target as Element).matches("textarea, button")) {
            e.preventDefault();
            e.stopPropagation();
            void ok();
          }
        }}
      >
        <div class="et-title">{props.creating ? "Create Tool" : "Edit Tool"}</div>
        <div class="et-form">
          <div class="et-row">
            <label class="et-label" for="et-name">
              Name:
            </label>
            <input id="et-name" class="set-input et-grow" value={tool().name} onInput={(e) => set({ name: e.currentTarget.value })} />
            <label class="et-label et-group-label" for="et-group">
              Group:
            </label>
            <input
              id="et-group"
              class="set-input et-group"
              list="et-groups"
              value={tool().group}
              placeholder={DEFAULT_GROUP}
              onInput={(e) => set({ group: e.currentTarget.value })}
            />
            <datalist id="et-groups">
              <For each={props.groups}>{(g) => <option value={g} />}</For>
            </datalist>
          </div>
          <div class="et-row">
            <label class="et-label" for="et-desc">
              Description:
            </label>
            <input id="et-desc" class="set-input et-grow" value={tool().description} onInput={(e) => set({ description: e.currentTarget.value })} />
          </div>
          <div class="et-section">Tool Settings</div>
          <div class="et-row">
            <label class="et-label" for="et-program">
              Program:
            </label>
            <div class="et-path">
              <input
                id="et-program"
                ref={(el) => (inputs.program = el)}
                class="set-input et-mono"
                value={tool().program}
                placeholder="vendor/bin/phpstan"
                onInput={(e) => set({ program: e.currentTarget.value })}
              />
              <button
                class="icon-btn"
                title="Browse for a file"
                onClick={async () => {
                  const v = await browse(false);
                  if (v) set({ program: v });
                }}
              >
                <FolderOpen size={14} />
              </button>
            </div>
            <MacroButton field="program" />
          </div>
          <div class="et-row">
            <label class="et-label" for="et-args">
              Arguments:
            </label>
            <input
              id="et-args"
              ref={(el) => (inputs.arguments = el)}
              class="set-input et-mono et-grow"
              value={tool().arguments}
              placeholder="analyse $FilePath$"
              onInput={(e) => set({ arguments: e.currentTarget.value })}
            />
            <MacroButton field="arguments" />
          </div>
          <div class="et-row">
            <label class="et-label" for="et-wd">
              Working directory:
            </label>
            <div class="et-path">
              <input
                id="et-wd"
                ref={(el) => (inputs.workingDirectory = el)}
                class="set-input et-mono"
                value={tool().workingDirectory}
                placeholder="$ProjectFileDir$"
                onInput={(e) => set({ workingDirectory: e.currentTarget.value })}
              />
              <button
                class="icon-btn"
                title="Browse for a directory"
                onClick={async () => {
                  const v = await browse(true);
                  if (v) set({ workingDirectory: v });
                }}
              >
                <FolderOpen size={14} />
              </button>
            </div>
            <MacroButton field="workingDirectory" />
          </div>
          <button class="et-section et-toggle" onClick={() => setAdvanced((a) => !a)}>
            {advanced() ? <ChevronDown size={14} /> : <ChevronRight size={14} />} Advanced Options
          </button>
          <Show when={advanced()}>
            <div class="et-advanced">
              <label class="set-check">
                <input type="checkbox" checked={tool().synchronize} onChange={(e) => set({ synchronize: e.currentTarget.checked })} />
                <span>Synchronize files after execution</span>
              </label>
              <label class="set-check">
                <input type="checkbox" checked={tool().openConsole} onChange={(e) => set({ openConsole: e.currentTarget.checked })} />
                <span>Open console for tool output</span>
              </label>
              <label class="set-check et-indent" classList={{ disabled: !tool().openConsole }}>
                <input
                  type="checkbox"
                  disabled={!tool().openConsole}
                  checked={tool().activateOnStdout}
                  onChange={(e) => set({ activateOnStdout: e.currentTarget.checked })}
                />
                <span>Make console active on message in stdout</span>
              </label>
              <label class="set-check et-indent" classList={{ disabled: !tool().openConsole }}>
                <input
                  type="checkbox"
                  disabled={!tool().openConsole}
                  checked={tool().activateOnStderr}
                  onChange={(e) => set({ activateOnStderr: e.currentTarget.checked })}
                />
                <span>Make console active on message in stderr</span>
              </label>
              <label class="et-label et-filters-label" for="et-filters">
                Output filters:
              </label>
              <textarea
                id="et-filters"
                class="set-input et-mono et-filters"
                rows={3}
                value={tool().outputFilters.join("\n")}
                placeholder="$FILE_PATH$:$LINE$:$COLUMN$"
                onInput={(e) =>
                  set({
                    outputFilters: e.currentTarget.value.split("\n"),
                  })
                }
              />
              <div class="set-desc">
                One regular expression per line. Use {FILTER_MACROS.join(", ")} to turn matches into links, e.g.{" "}
                <code>$FILE_PATH$:$LINE$</code>.
              </div>
            </div>
          </Show>
          <Show when={problems().length}>
            <div class="et-problems">
              <For each={problems()}>{(p) => <div class="set-error">{p}</div>}</For>
            </div>
          </Show>
        </div>
        <div class="dialog-buttons et-buttons">
          <span class="et-spacer" />
          <button class="btn-primary" onClick={() => void ok()}>
            OK
          </button>
          <button class="btn-secondary" onClick={() => props.onDone(null)}>
            Cancel
          </button>
        </div>
      </div>
      <Show when={macroFor()}>
        {(field) => <InsertMacroDialog onInsert={(name) => insertMacro(field(), name)} onClose={() => setMacroFor(null)} />}
      </Show>
    </div>
  );
}
