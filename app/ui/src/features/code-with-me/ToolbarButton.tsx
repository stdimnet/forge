/** The Code With Me icon of the main toolbar, left of the Run widget. */
import { Show } from "solid-js";
import { Users } from "lucide-solid";
import { inSession, popupOpen, session, setPopupOpen } from "./store";
import { SessionPopup, type StartHandler } from "./SessionPopup";
import { CustomPermissionsHost } from "./CustomPermissions";
import { SessionLogHost } from "./SessionLog";

export function ToolbarButton(props: { onStart: StartHandler }) {
  let btn!: HTMLButtonElement;
  const title = () => {
    const s = session();
    if (s.role === "host") return `Code With Me: sharing with ${s.participants.length - 1} guest(s)`;
    if (s.role === "guest") return `Code With Me: ${s.hostLabel ?? "host"}`;
    return "Code With Me";
  };
  return (
    <>
      <button
        ref={btn}
        class="icon-btn cwm-toolbar-btn"
        classList={{ active: inSession(), open: popupOpen() }}
        title={title()}
        aria-haspopup="dialog"
        aria-expanded={popupOpen()}
        data-action="CodeWithMe.StartSession"
        onClick={() => setPopupOpen((o) => !o)}
      >
        <Users size={16} />
        <Show when={inSession() && session().participants.length > 1}>
          <span class="cwm-toolbar-count">{session().participants.length}</span>
        </Show>
      </button>
      <Show when={popupOpen()}>
        <SessionPopup anchor={btn} onStart={props.onStart} />
      </Show>
      <CustomPermissionsHost />
      <SessionLogHost />
    </>
  );
}
