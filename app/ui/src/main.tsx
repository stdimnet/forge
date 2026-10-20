/* @refresh reload */
import { render } from "solid-js/web";
import "./theme/theme.css";
import "./shell/shell.css";
import { setTheme, theme } from "./state/theme";
import { App } from "./App";
import { isTauri } from "./ipc";

// Browser-mode backend only: mocks must never run (and write demo files) in the desktop app.
if (!isTauri()) await import("./mock");
// Features load after the mocks so anything they invoke at startup has a backend.
await import("./features");

setTheme(theme());
render(() => <App />, document.getElementById("root")!);
