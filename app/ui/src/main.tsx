/* @refresh reload */
import { render } from "solid-js/web";
import "./theme/theme.css";
import "./shell/shell.css";
import "./mock";
import "./features";
import { setTheme, theme } from "./state/theme";
import { App } from "./App";

setTheme(theme());
render(() => <App />, document.getElementById("root")!);
