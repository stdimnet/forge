/**
 * Renders dialogs: in-page over a backdrop (`DialogHost`, browser mode) or as the whole page
 * of a dialog window (`DialogWindowApp`, `?dialog=<id>`).
 */
import { createResource, For, onMount, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { isTauri } from "@/ipc";
import { windowApi } from "@/ipc/window";
import { openWorkspace, restoreWorkspace } from "@/state/workspace";
import { dialogDef, dialogTitle, inPageDialogs, requestCloseDialog, type InPageDialog } from "./dialogs";
import type { DialogBoot } from "./mode";
import { windowLabel } from "./mode";
import { trackMaximized } from "./chrome";
import { WindowFrame } from "./WindowFrame";

async function prepare(id: string, props: unknown): Promise<true> {
  try {
    await dialogDef(id)?.prepare?.(props);
  } catch (e) {
    console.warn(`dialog "${id}" could not load its data`, e);
  }
  return true;
}

function InPage(props: { dialog: InPageDialog }) {
  const def = dialogDef(props.dialog.id)!;
  const [ready] = createResource(() => prepare(def.id, props.dialog.props));
  const close = () => requestCloseDialog(def.id);
  return (
    <div class="wf-backdrop" data-dialog={def.id} onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <Show when={ready()}>
        <WindowFrame
          inPage
          title={dialogTitle(def, props.dialog.props)}
          width={def.width}
          height={def.height}
          onRequestClose={close}
        >
          <Dynamic component={def.component} props={props.dialog.props} />
        </WindowFrame>
      </Show>
    </div>
  );
}

/** In-page dialogs of this window (browser mode). */
export function DialogHost() {
  return <For each={inPageDialogs()}>{(d) => <InPage dialog={d} />}</For>;
}

/** The page of a dialog window: the frame with only that dialog in it. */
export function DialogWindowApp(props: { boot: DialogBoot }) {
  const id = props.boot.id;
  const def = dialogDef(id);
  const dialogProps = props.boot.props ?? {};
  const [ready] = createResource(async () => {
    // The project the IDE window has open (project-scoped settings, run configurations).
    if (isTauri()) await restoreWorkspace().catch(() => undefined);
    else await openWorkspace("demo").catch(() => undefined);
    return prepare(id, dialogProps);
  });
  if (def) document.title = dialogTitle(def, dialogProps);
  onMount(() => {
    trackMaximized();
    if (!isTauri()) return;
    void windowLabel().then((me) => windowApi.onCloseRequested((e) => e.window === me && requestCloseDialog(id)));
  });
  return (
    <Show when={def} fallback={<div class="wf-missing">Unknown dialog "{id}"</div>}>
      {(d) => (
        <WindowFrame title={dialogTitle(d(), dialogProps)} onRequestClose={() => requestCloseDialog(id)}>
          <Show when={ready()}>
            <Dynamic component={d().component} props={dialogProps} />
          </Show>
        </WindowFrame>
      )}
    </Show>
  );
}
