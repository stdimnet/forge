/* @refresh reload */
import { render } from "solid-js/web";
import { SplashApp } from "./SplashApp";

// The splash window's entry (`splash.html`): only the splash, none of the workbench.
render(() => <SplashApp />, document.getElementById("root")!);
// The live splash is in place underneath: fade the static one (page source) out.
const boot = document.getElementById("boot");
if (boot) {
  boot.classList.add("leaving");
  setTimeout(() => boot.remove(), 250);
}
