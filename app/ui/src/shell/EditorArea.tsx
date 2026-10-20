import { For, Show } from "solid-js";
import { ChevronRight, X } from "lucide-solid";
import { editor, activeTab, closeTab, isDirty } from "@/state/editor";
import { statusOf } from "@/state/vcs";
import { CodeEditor } from "@/editor/CodeEditor";
import { FileIcon } from "./FileIcon";
import { setEditorActive } from "./editorActions";
import { editorViewFor } from "./editorViews";

function Tabs() {
  return (
    <div class="editor-tabs" role="tablist">
      <For each={editor.tabs}>
        {(tab) => {
          const active = () => editor.active === tab.path;
          const status = () => statusOf(tab.path);
          return (
            <div
              class="editor-tab"
              classList={{ active: active() }}
              role="tab"
              title={tab.path}
              onClick={() => setEditorActive(tab.path)}
              onAuxClick={(e) => e.button === 1 && closeTab(tab.path)}
            >
              <Show when={editorViewFor(tab)?.icon} fallback={<FileIcon name={tab.name} size={14} />}>
                {(Icon) => {
                  const I = Icon();
                  return <I size={14} tab={tab} />;
                }}
              </Show>
              <span class={status() ? `vcs-${status()}` : ""}>{tab.name}</span>
              <button
                class="tab-close"
                classList={{ dirty: isDirty(tab) }}
                title="Close (Ctrl+F4)"
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(tab.path);
                }}
              >
                <span class="dirty-dot" />
                <X size={13} />
              </button>
            </div>
          );
        }}
      </For>
    </div>
  );
}

function Breadcrumbs() {
  const parts = () => {
    const tab = activeTab();
    if (!tab) return [];
    const view = editorViewFor(tab);
    if (view) return view.breadcrumbs?.(tab) ?? [tab.name];
    return tab.path.split("/");
  };
  return (
    <div class="breadcrumbs">
      <For each={parts()}>
        {(p, i) => (
          <>
            <Show when={i() > 0}>
              <ChevronRight size={12} class="muted" />
            </Show>
            <span class="crumb">{p}</span>
          </>
        )}
      </For>
    </div>
  );
}

function Welcome() {
  const rows: [string, string][] = [
    ["Search Everywhere", "Double Shift"],
    ["Go to File", "Ctrl+Shift+N"],
    ["Recent Files", "Ctrl+E"],
    ["Navigation Bar", "Alt+Home"],
    ["Drop files here to open them", ""],
  ];
  return (
    <div class="editor-empty">
      <img src="/forge.svg" alt="" class="editor-empty-logo" />
      <div class="editor-empty-rows">
        <For each={rows}>
          {([label, key]) => (
            <div class="editor-empty-row">
              <span>{label}</span>
              <span class="kbd-hint">{key}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

export function EditorArea() {
  return (
    <main class="editor-area">
      <Show when={editor.tabs.length > 0} fallback={<Welcome />}>
        <Tabs />
        <Breadcrumbs />
        <div class="editor-stack">
          <For each={editor.tabs}>
            {(tab) => (
              <div class="editor-slot" classList={{ hidden: editor.active !== tab.path }}>
                <Show when={editorViewFor(tab)} fallback={<CodeEditor tab={tab} />}>
                  {(view) => {
                    const View = view().component;
                    return <View tab={tab} />;
                  }}
                </Show>
              </div>
            )}
          </For>
        </div>
      </Show>
    </main>
  );
}
