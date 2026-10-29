/** The server list of "Upload to…", "Download from…" and "Sync with Deployed to…". */
import { createSignal, For, onMount, Show } from "solid-js";
import { Settings } from "lucide-solid";
import type { ServerView } from "@/ipc/deployment";
import { choice, openDeploymentSettings, type ServerChoice } from "./store";
import { KindIcon, KIND_LABELS } from "./DeploymentPage";

export function ChooseServerHost() {
  return <Show when={choice()}>{(c) => <ChooseServerPopup choice={c()} />}</Show>;
}

function ChooseServerPopup(props: { choice: ServerChoice }) {
  let popup!: HTMLDivElement;
  const servers = () => props.choice.servers;
  const start = Math.max(0, servers().findIndex((s) => s.id === props.choice.defaultId));
  const [index, setIndex] = createSignal(start);
  onMount(() => popup.focus());

  const pick = (s: ServerView | null) => props.choice.pick(s);
  const onKeyDown = (e: KeyboardEvent) => {
    const n = servers().length;
    if (e.key === "Escape") pick(null);
    else if (e.key === "ArrowDown") setIndex((i) => (i + 1) % n);
    else if (e.key === "ArrowUp") setIndex((i) => (i - 1 + n) % n);
    else if (e.key === "Enter") pick(servers()[index()] ?? null);
    else if (/^[1-9]$/.test(e.key) && Number(e.key) <= n) pick(servers()[Number(e.key) - 1]);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && pick(null)}>
      <div class="popup deploy-choose" role="dialog" aria-label={props.choice.title} tabIndex={-1} ref={popup} onKeyDown={onKeyDown}>
        <div class="deploy-choose-title">{props.choice.title}</div>
        <div class="deploy-choose-list" role="listbox">
          <For each={servers()}>
            {(s, i) => (
              <div
                class="deploy-choose-item"
                classList={{ selected: index() === i() }}
                role="option"
                aria-selected={index() === i()}
                data-server={s.id}
                onMouseEnter={() => setIndex(i())}
                onClick={() => pick(s)}
              >
                <span class="deploy-choose-key muted">{i() < 9 ? i() + 1 : ""}</span>
                <KindIcon kind={s.kind} size={14} />
                <span class="deploy-choose-name" classList={{ "deploy-default": s.id === props.choice.defaultId }}>
                  {s.name}
                </span>
                <span class="deploy-choose-kind muted">{KIND_LABELS[s.kind]}</span>
              </div>
            )}
          </For>
        </div>
        <div class="deploy-choose-footer">
          <button
            class="deploy-choose-config"
            onClick={() => {
              pick(null);
              openDeploymentSettings();
            }}
          >
            <Settings size={13} /> Configure Servers…
          </button>
        </div>
      </div>
    </div>
  );
}
