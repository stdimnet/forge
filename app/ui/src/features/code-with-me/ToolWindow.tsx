/**
 * The Code With Me tool window (right, only while a session runs): the Participants tab
 * with each connection's path (Direct / Relay, Connecting…, Reconnecting…), the relay state,
 * Follow, Jump To and the host's guest actions, and the host's Session Log tab.
 */
import { createSignal, For, Show } from "solid-js";
import { runCommand } from "@/state/commands";
import { session } from "./store";
import { Participants } from "./Participants";
import { RelayLine } from "./ConnectionStatus";
import { SessionLogList } from "./SessionLog";
import { followState } from "./follow";
import { navKeys } from "./a11y";
import { t } from "@/i18n";

type Tab = "participants" | "log";

const [tab, setTab] = createSignal<Tab>("participants");

const tabs = (): { id: Tab; title: string }[] =>
  session().role === "host"
    ? [
        { id: "participants", title: t("tools.codeWithMe.participants") },
        { id: "log", title: t("tools.codeWithMe.sessionLog") },
      ]
    : [{ id: "participants", title: t("tools.codeWithMe.participants") }];

/** The tabs in the tool window header. */
export function CodeWithMeTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <For each={tabs()}>
        {(item) => (
          <div
            class="run-tab"
            classList={{ active: (tab() === "log" && session().role === "host" ? "log" : "participants") === item.id }}
            role="tab"
            tabIndex={0}
            aria-selected={(tab() === "log" && session().role === "host" ? "log" : "participants") === item.id}
            data-cwm-tab={item.id}
            onKeyDown={(e) => {
              if (e.key !== "Enter" && e.key !== " ") return;
              e.preventDefault();
              setTab(item.id);
            }}
            onClick={() => setTab(item.id)}
          >
            <span class="run-tab-title">{item.title}</span>
          </div>
        )}
      </For>
    </div>
  );
}

export function CodeWithMeToolWindow() {
  return (
    <div class="cwm-tool-window" tabIndex={-1} onKeyDown={navKeys}>
      <Show
        when={tab() === "log" && session().role === "host"}
        fallback={
          <>
            <Participants paths />
            <RelayLine />
            <div class="cwm-tool-actions">
              <Show when={followState().leader !== null}>
                <button class="btn-secondary" data-action="CodeWithMe.StopFollowing" onClick={() => void runCommand("CodeWithMe.StopFollowing")}>
                  {t("tools.codeWithMe.stopFollowing")}
                </button>
              </Show>
              <Show when={session().role === "host"}>
                <button
                  class="btn-secondary"
                  classList={{ active: !!session().followEveryone }}
                  aria-pressed={!!session().followEveryone}
                  data-action="CodeWithMe.MakeEveryoneFollow"
                  onClick={() => void runCommand("CodeWithMe.MakeEveryoneFollow")}
                >
                  {t("tools.codeWithMe.makeEveryoneFollow")}
                </button>
              </Show>
            </div>
            <div class="cwm-popup-note muted">{t("tools.codeWithMe.toolWindowHint")}</div>
          </>
        }
      >
        <div class="cwm-tool-log">
          <SessionLogList />
        </div>
      </Show>
    </div>
  );
}
