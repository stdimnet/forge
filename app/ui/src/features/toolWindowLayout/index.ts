/**
 * Window | Store Current Layout as Default and Restore Default Layout (`Shift+F12`), like
 * PHPStorm. The placement UI itself (Move To, Remove from Sidebar, drag and drop, the ⋯
 * button) lives in the shell's stripes and tool window headers.
 */
import { registerCommand } from "@/state/commands";
import { restoreDefaultLayout, storeLayoutAsDefault } from "@/state/layoutPersistence";

registerCommand({ id: "StoreDefaultLayout", title: "Store Current Layout as Default", category: "Window", run: storeLayoutAsDefault });
registerCommand({ id: "RestoreDefaultLayout", title: "Restore Default Layout", category: "Window", run: restoreDefaultLayout });
