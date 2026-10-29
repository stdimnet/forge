/**
 * Add Project from Template…, like Rider's Add | New Project: the templates `dotnet new
 * list` reports (parsed in Rust, `dotnet_templates`), a name, the folder, language and
 * target framework; `dotnet new` and `dotnet sln add` run in the Run tool window.
 */
import { createMemo, createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { render } from "solid-js/web";
import { LayoutTemplate } from "lucide-solid";
import { dotnetApi, type Template } from "@/ipc/dotnet";
import { suspendKeymap } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { dotnetWorkspace, errorText, refreshSolution, runDotnetCommand, showDotnetBalloon } from "./state";

const [open, setOpen] = createSignal(false);

export function openAddProjectDialog(): void {
  setOpen(true);
}

const FRAMEWORKS = [
  { value: "", label: "Template default" },
  { value: "net10.0", label: ".NET 10" },
  { value: "net9.0", label: ".NET 9" },
  { value: "net8.0", label: ".NET 8 (LTS)" },
];

function Dialog() {
  const release = suspendKeymap();
  onCleanup(release);
  const [templates] = createResource(() => dotnetApi.templates().catch((e) => ({ templates: [] as Template[], fromSdk: false, error: errorText(e) })));
  const [filter, setFilter] = createSignal("");
  const [picked, setPicked] = createSignal<Template | null>(null);
  const [name, setName] = createSignal("");
  const [dir, setDir] = createSignal<string | null>(null);
  const [language, setLanguage] = createSignal("");
  const [framework, setFramework] = createSignal("");
  const [addToSolution, setAddToSolution] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const solution = () => dotnetWorkspace()?.solution ?? "";
  const list = createMemo(() => {
    const f = filter().trim().toLowerCase();
    return (templates()?.templates ?? []).filter((t) => t.project && (!f || t.name.toLowerCase().includes(f) || t.shortNames.some((s) => s.includes(f)) || t.tags.some((x) => x.toLowerCase().includes(f))));
  });
  const template = () => picked() ?? list()[0] ?? null;
  const folderOf = (n: string) => {
    const sln = solution();
    const base = sln.includes("/") ? sln.slice(0, sln.lastIndexOf("/") + 1) : "";
    const src = dotnetWorkspace()?.projects.some((p) => p.path.startsWith(`${base}src/`)) ? "src/" : "";
    const tests = template()?.tags.includes("Test") && dotnetWorkspace()?.projects.some((p) => p.path.startsWith(`${base}tests/`)) ? "tests/" : "";
    return `${base}${tests || src}${n}`;
  };
  const directory = () => dir() ?? folderOf(name().trim() || "NewProject");
  const close = () => setOpen(false);
  const create = async () => {
    const t = template();
    if (!t || !name().trim()) return;
    setBusy(true);
    try {
      const cmds = await dotnetApi.newProject({
        template: t.shortNames[0] ?? "",
        name: name().trim(),
        directory: directory(),
        language: language() || undefined,
        framework: framework() || undefined,
        solution: addToSolution() ? solution() || undefined : undefined,
      });
      close();
      for (const c of cmds) {
        const code = await runDotnetCommand(c);
        if (code !== 0) {
          showDotnetBalloon("Cannot create the project", "See the Run tool window for dotnet's output.", 8000);
          return;
        }
      }
      await refreshSolution(true);
      showDotnetBalloon(`Created ${name().trim()}`, directory(), 5000);
    } catch (e) {
      showDotnetBalloon("Cannot create the project", errorText(e), 8000);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dialog dn-add-dialog"
        style={{ width: "min(720px, calc(100vw - 32px))" }}
        role="dialog"
        data-testid="dotnet-add-project"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "SELECT") void create();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">Add Project from Template</div>
        <div class="dialog-body dn-add-body">
          <div class="dn-add-templates">
            <input placeholder="Filter templates" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} spellcheck={false} />
            <div class="dn-add-list" data-testid="dotnet-templates">
              <For each={list()} fallback={<div class="muted dn-message">{templates.loading ? "Reading dotnet new list…" : "No templates"}</div>}>
                {(t) => (
                  <div class="dn-add-item" classList={{ selected: template()?.name === t.name }} onClick={() => setPicked(t)}>
                    <LayoutTemplate size={14} class="muted" />
                    <span>{t.name}</span>
                    <span class="muted dn-detail">{t.shortNames.join(", ")}</span>
                  </div>
                )}
              </For>
            </div>
            <Show when={templates() && !templates()!.fromSdk}>
              <div class="muted dn-note">Common templates: {templates()!.error ?? "dotnet new list did not run"}</div>
            </Show>
          </div>
          <div class="dn-add-form">
            <label class="form-label">Name</label>
            <input ref={(el) => queueMicrotask(() => el.focus())} value={name()} placeholder="Bookstore.Shipping" spellcheck={false} data-testid="dotnet-new-name" onInput={(e) => setName(e.currentTarget.value)} />
            <label class="form-label">Folder</label>
            <input value={directory()} spellcheck={false} onInput={(e) => setDir(e.currentTarget.value)} />
            <label class="form-label">Language</label>
            <select value={language()} onChange={(e) => setLanguage(e.currentTarget.value)}>
              <option value="">{template()?.languages[0] ?? "C#"} (default)</option>
              <For each={(template()?.languages ?? []).slice(1)}>{(l) => <option value={l}>{l}</option>}</For>
            </select>
            <label class="form-label">Framework</label>
            <select value={framework()} onChange={(e) => setFramework(e.currentTarget.value)}>
              <For each={FRAMEWORKS}>{(f) => <option value={f.value}>{f.label}</option>}</For>
            </select>
            <Show when={solution()}>
              <label class="dn-check">
                <input type="checkbox" checked={addToSolution()} onChange={(e) => setAddToSolution(e.currentTarget.checked)} />
                Add to {solution().split("/").pop()}
              </label>
            </Show>
          </div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" disabled={busy() || !name().trim() || !template()} onClick={() => void create()}>
            Create
          </button>
        </div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "dotnet-add-project-overlay";
  document.body.appendChild(host);
  render(
    () => (
      <Show when={open()}>
        <Dialog />
      </Show>
    ),
    host,
  );
}
