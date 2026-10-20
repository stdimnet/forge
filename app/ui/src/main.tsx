/* @refresh reload */
import { render } from "solid-js/web";
import "./theme/theme.css";
import "./shell/shell.css";
import { setTheme, theme } from "./state/theme";
import { App } from "./App";
import { isTauri } from "./ipc";
import { dialogBoot } from "./shell/window/mode";
import { DialogHost, DialogWindowApp } from "./shell/window/DialogHost";

// Browser-mode backend only: mocks must never run (and write demo files) in the desktop app.
if (!isTauri()) await import("./mock");
// Features load after the mocks so anything they invoke at startup has a backend.
await import("./features");

setTheme(theme());
// `?dialog=<id>`: a dialog window (see shell/window/dialogs.ts) shows only that dialog.
const boot = dialogBoot();
document.documentElement.dataset.window = boot ? "dialog" : "main";
render(() => (boot ? <DialogWindowApp boot={boot} /> : <App />), document.getElementById("root")!);

// In-page dialogs (browser mode) and nested dialogs of a dialog window.
const dialogHost = document.createElement("div");
dialogHost.id = "dialog-host";
document.body.appendChild(dialogHost);
render(() => <DialogHost />, dialogHost);
