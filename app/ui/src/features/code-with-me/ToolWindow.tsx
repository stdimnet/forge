/**
 * The Code With Me tool window (right, only while a session runs): the Participants tab
 * with Follow, Jump To and the host's guest actions, and the host's Session Log tab.
 */
import { createSignal, For, Show } from "solid-js";
import { runCommand } from "@/state/commands";
import { session } from "./store";
import { Participants } from "./Participants";
import { SessionLogList } from "./SessionLog";
import { followState } from "./follow";

type Tab = "participants" | "log";

const [tab, setTab] = createSignal<Tab>("participants");

const tabs = (): { id: Tab; title: string }[] =>
  session().role === "host"
    ? [
        { id: "participants", title: "Participants" },
        { id: "log", title: "Session Log" },
      ]
    : [{ id: "participants", title: "Participants" }];

/** The tabs in the tool window header. */
export function CodeWithMeTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <For each={tabs()}>
        {(t) => (
          <div
            class="run-tab"
            classList={{ active: (tab() === "log" && session().role === "host" ? "log" : "participants") === t.id }}
            role="tab"
            data-cwm-tab={t.id}
            onClick={() => setTab(t.id)}
          >
            <span class="run-tab-title">{t.title}</span>
          </div>
        )}
      </For>
    </div>
  );
}

export function CodeWithMeToolWindow() {
  return (
    <div class="cwm-tool-window">
      <Show
        when={tab() === "log" && session().role === "host"}
        fallback={
          <>
            <Participants />
            <div class="cwm-tool-actions">
              <Show when={followState().leader !== null}>
                <button class="btn-secondary" data-action="CodeWithMe.StopFollowing" onClick={() => void runCommand("CodeWithMe.StopFollowing")}>
                  Stop Following
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
                  Make Everyone Follow You
                </button>
              </Show>
            </div>
            <div class="cwm-popup-note muted">Click a participant to follow them; right-click for Jump To and more.</div>
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
