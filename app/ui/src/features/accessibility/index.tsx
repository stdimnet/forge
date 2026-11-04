/**
 * Accessibility (every window): screen reader live regions, dialog focus management, focus
 * cycling between the areas of the window, "Focus Editor" (Escape in a tool window) and the
 * Presentation Assistant. Editor announcements and menu entries live in `./ide` (IDE windows).
 * The rules for writing accessible UI: docs/guide/developer/accessibility.md.
 */
import { render } from "solid-js/web";
import { registerCommand, onCommandInvoked } from "@/state/commands";
import { getSetting, saveSetting } from "@/state/settings";
import { actionById, keymapSnapshot, registerKeymapActions } from "@/keymap";
import { activeView } from "@/features/navigation/editorCommands";
import { liveRegion } from "./announce";
import { installDialogAccessibility } from "./dialogs";
import { focusArea, focusEditor, focusToolWindowContent, shownToolWindows, trackAreaFocus } from "./areas";
import { present, presentationOf } from "./presentation";
import { PresentationAssistant } from "./PresentationAssistant";
import "./accessibility.css";

export { announce, announceForScreenReader, screenReaderMode } from "./announce";
export { focusArea, focusEditor } from "./areas";

registerKeymapActions([
  {
    id: "FocusEditor",
    title: "Focus Editor",
    group: "Main Menu/Window/Active Tool Window",
    context: "toolwindow",
    shortcuts: { $default: ["Escape"], macOS: ["Escape"] },
  },
  {
    id: "Forge.FocusNextArea",
    title: "Focus Next Area",
    group: "Main Menu/Window",
    description: "Moves keyboard focus to the next part of the window: toolbar, navigation bar, tool windows, editor, status bar.",
    shortcuts: { $default: ["Ctrl+Shift+F6"], macOS: ["Ctrl+Shift+F6"] },
  },
  {
    id: "Forge.FocusPreviousArea",
    title: "Focus Previous Area",
    group: "Main Menu/Window",
    shortcuts: { $default: ["Ctrl+Alt+Shift+F6"], macOS: ["Ctrl+Alt+Shift+F6"] },
  },
  { id: "Forge.TogglePresentationAssistant", title: "Presentation Assistant", group: "Main Menu/View/Appearance" },
]);

registerCommand({
  id: "FocusEditor",
  title: "Focus Editor",
  category: "Window",
  run: () => {
    const view = activeView();
    if (view) view.focus();
    else focusEditor();
  },
});
registerCommand({ id: "Forge.FocusNextArea", title: "Focus Next Area", category: "Window", run: () => void focusArea(1) });
registerCommand({ id: "Forge.FocusPreviousArea", title: "Focus Previous Area", category: "Window", run: () => void focusArea(-1) });
registerCommand({
  id: "Forge.TogglePresentationAssistant",
  title: "Presentation Assistant",
  category: "View",
  run: () => saveSetting("appearance.presentationAssistant.enabled", !getSetting<boolean>("appearance.presentationAssistant.enabled", false)),
});

// Activate…ToolWindow actions that a feature implements itself (toggling its window) still
// move focus into the window they show, as Alt+<n> does in PHPStorm.
onCommandInvoked((inv) => {
  if (!/^Activate\w*ToolWindow$/.test(inv.command.id) || typeof document === "undefined") return;
  const before = new Set(shownToolWindows());
  setTimeout(() => {
    const shown = shownToolWindows().filter((id) => !before.has(id));
    if (shown.length === 1) focusToolWindowContent(shown[0]);
  }, 60);
});

// Presentation Assistant: every command run through the registry.
onCommandInvoked((inv) => {
  const p = presentationOf(
    inv,
    {
      enabled: getSetting<boolean>("appearance.presentationAssistant.enabled", false) === true,
      alternateKeymap: getSetting<boolean>("appearance.presentationAssistant.alternateKeymap", true) !== false,
    },
    { action: actionById, alternate: () => keymapSnapshot()?.alternate },
  );
  if (p) present(p, getSetting<number>("appearance.presentationAssistant.duration", 4000));
});

if (typeof document !== "undefined" && document.body) {
  liveRegion("polite");
  liveRegion("assertive");
  installDialogAccessibility();
  trackAreaFocus();
  const host = document.createElement("div");
  host.id = "forge-presentation-assistant";
  document.body.appendChild(host);
  render(() => <PresentationAssistant />, host);
}
