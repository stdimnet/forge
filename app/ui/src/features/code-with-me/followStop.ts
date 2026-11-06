/**
 * Which editor actions stop following: typing, deleting, moving the caret, selecting,
 * undo/redo and clicking into the editor.
 *
 * A click stops following only once the mouse button is released. Stopping hides the
 * "Following Alex" banner above the editor, which moves the text up; doing that while the
 * button is down would let CodeMirror map the click to the line below the one clicked.
 */
import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

const STOPPING_EVENTS = ["input", "delete", "move", "select", "undo", "redo"];

/** Call `stop` when the user acts in the editor; `active` says whether following is on. */
export function stopOnLocalActions(active: () => boolean, stop: () => void): Extension {
  let pressed = false;
  const release = (win: Window) => {
    const done = () => {
      win.removeEventListener("mouseup", done, true);
      win.removeEventListener("dragend", done, true);
      // After CodeMirror's own mouseup handler has set the final selection.
      setTimeout(() => {
        pressed = false;
        if (active()) stop();
      });
    };
    win.addEventListener("mouseup", done, true);
    win.addEventListener("dragend", done, true);
  };
  return [
    EditorView.updateListener.of((u) => {
      if (pressed || !active()) return;
      if (u.transactions.some((tr) => STOPPING_EVENTS.some((ev) => tr.isUserEvent(ev)))) stop();
    }),
    EditorView.domEventHandlers({
      mousedown: (_e, view) => {
        if (active() && !pressed) {
          pressed = true;
          release(view.dom.ownerDocument.defaultView ?? window);
        }
        return false;
      },
    }),
  ];
}
