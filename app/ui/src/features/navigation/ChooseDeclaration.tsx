/** The "Choose Declaration" list (state and API in `chooseDeclaration.ts`). */
import { createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { FileIcon } from "@/shell/FileIcon";
import { closeChooser, type Chooser, type DeclarationTarget } from "./declarationChooser";

const fileName = (path: string) => path.split("/").pop() ?? path;
const dirName = (path: string) => path.split("/").slice(0, -1).join("/");

export function ChooseDeclarationPopup(props: { chooser: Chooser }) {
  const [index, setIndex] = createSignal(0);
  let el!: HTMLDivElement;
  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) closeChooser();
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, props.chooser.y - r.height - 24)}px`;
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, window.innerWidth - r.width - 8)}px`;
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));
  const open = (t: DeclarationTarget) => {
    closeChooser();
    props.chooser.open(t);
  };
  // List navigation while the popup has focus (not a shortcut: the popup owns these keys).
  const onKey: JSX.EventHandler<HTMLDivElement, KeyboardEvent> = (e) => {
    const n = props.chooser.targets.length;
    if (e.key === "ArrowDown") setIndex((i) => (i + 1) % n);
    else if (e.key === "ArrowUp") setIndex((i) => (i + n - 1) % n);
    else if (e.key === "Enter") open(props.chooser.targets[index()]);
    else if (e.key === "Escape") closeChooser();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div
      class="decl-chooser"
      ref={el}
      tabIndex={-1}
      role="listbox"
      aria-label={props.chooser.title}
      style={{ left: `${props.chooser.x}px`, top: `${props.chooser.y}px` }}
      onKeyDown={onKey}
    >
      <div class="decl-chooser-title">{props.chooser.title}</div>
      <For each={props.chooser.targets}>
        {(t, i) => (
          <>
          <Show when={t.group && t.group !== props.chooser.targets[i() - 1]?.group}>
            <div class="decl-chooser-group" role="presentation">
              {t.group}
            </div>
          </Show>
          <button
            class="decl-chooser-row"
            classList={{ selected: i() === index() }}
            role="option"
            aria-selected={i() === index()}
            onMouseEnter={() => setIndex(i())}
            onClick={() => open(t)}
          >
            <FileIcon name={fileName(t.path)} size={14} />
            <Show
              when={t.name}
              fallback={
                <Show
                  when={t.preview}
                  fallback={
                    <span class="decl-chooser-name">
                      {fileName(t.path)}:{t.line + 1}
                    </span>
                  }
                >
                  <span class="decl-chooser-code">{t.preview}</span>
                </Show>
              }
            >
              <span class="decl-chooser-name">{t.name}</span>
              <Show when={t.container}>
                <span class="decl-chooser-container">{t.container}</span>
              </Show>
            </Show>
            <span class="decl-chooser-path">
              <Show when={(t.preview || t.name) && !t.wholeFile}>
                <span class="decl-chooser-file">
                  {fileName(t.path)}:{t.line + 1}
                </span>
              </Show>
              <Show when={!t.name}>{dirName(t.path)}</Show>
            </span>
          </button>
          </>
        )}
      </For>
    </div>
  );
}
