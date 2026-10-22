/** Status bar widget "Symfony 7.2 · dev" with a menu of Symfony actions. */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { ProjectTypeIcon } from "@/shell/ProjectTypeIcon";
import { openSettings } from "@/features/settings/dialogState";
import { showToolWindow } from "@/state/layout";
import { setActiveTab } from "./console";
import { openConsolePopup } from "./ConsolePopup";
import { loadSymfony, shortVersion, symfonyActive, symfonyInfo } from "./store";

function Menu(props: { x: number; y: number; onClose: () => void }) {
  const onDoc = (e: MouseEvent) => {
    if (!(e.target as Element).closest(".sf-status-menu")) props.onClose();
  };
  onMount(() => document.addEventListener("mousedown", onDoc, true));
  onCleanup(() => document.removeEventListener("mousedown", onDoc, true));
  const items: ([string, () => void] | null)[] = [
    ["Show Routes", () => (setActiveTab("routes"), showToolWindow("symfony"))],
    ["Show Services", () => (setActiveTab("services"), showToolWindow("symfony"))],
    ["Run Console Command…", () => openConsolePopup()],
    null,
    ["Refresh Symfony Index", () => void loadSymfony(true)],
    ["Symfony Settings…", () => openSettings("lang.php.symfony")],
  ];
  return (
    <div class="context-menu popup sf-status-menu" style={{ left: `${props.x}px`, bottom: `${props.y}px` }}>
      <For each={items}>
        {(it) =>
          it ? (
            <button
              class="context-item"
              onClick={() => {
                it[1]();
                props.onClose();
              }}
            >
              {it[0]}
            </button>
          ) : (
            <div class="context-sep" />
          )
        }
      </For>
    </div>
  );
}

export function SymfonyStatus() {
  const [menu, setMenu] = createSignal<{ x: number; y: number } | null>(null);
  return (
    <Show when={symfonyActive() && symfonyInfo()}>
      {(i) => (
        <>
          <button
            class="status-item status-project-type"
            title={`Symfony ${i().version ?? ""} (${i().environment})${i().containerXml ? `\nContainer: ${i().containerXml}` : ""}\n${i().routes} routes, ${i().services} services, ${i().templates} templates`}
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              setMenu(menu() ? null : { x: Math.min(r.left, window.innerWidth - 240), y: window.innerHeight - r.top + 4 });
            }}
          >
            <ProjectTypeIcon type="symfony" size={14} />
            Symfony {shortVersion(i().version)}
            <span class="muted">· {i().environment}</span>
          </button>
          <Show when={menu()}>
            {(m) => (
              <Portal>
                <Menu x={m().x} y={m().y} onClose={() => setMenu(null)} />
              </Portal>
            )}
          </Show>
        </>
      )}
    </Show>
  );
}
