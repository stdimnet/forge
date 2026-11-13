/**
 * Features of the IDE window that no dialog window needs for itself (the keymap, popups and
 * context menus stay in `./index`, every window has them). The Settings dialog
 * window skips this module (see `main.tsx`), which keeps it from loading the editor
 * git, search and editor tool code just to show a form. Every other window
 * loads it after `./index`.
 */
import "./git";
import "./lsp";
import "./accessibility/ide";
import "./find";
import "./localHistory";
import "./hierarchy";
import "./mainMenu";
import "./navBar";
import "./compare";
import "./bookmarks";
import "./profiling";
import "./colors";
import "./editorTabs";
import "./carets";
import "./documentation";
import "./todo";
import "./editorNavigation";
import "./runAnything";
import "./fileStructure";
import "./related";
import "./coverage";
import "./agent-tasks";
import "./endpoints";
import "./remoteHost";
