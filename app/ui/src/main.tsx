/* @refresh reload */
import { render } from "solid-js/web";
import "./theme/theme.css";
import "./shell/shell.css";
import { setTheme, theme } from "./state/theme";
import { isTauri } from "./ipc";
import { dialogBoot } from "./shell/window/mode";

// Browser-mode backend only: mocks must never run (and write demo files) in the desktop app.
if (!isTauri()) await import("./mock");

setTheme(theme());
const root = document.getElementById("root")!;

/** The IDE window (or a dialog window) with all features. */
async function bootWorkbench() {
  // The IDE language is selected before the features register their titles.
  const { initI18n, LANGUAGE_SETTING } = await import("./i18n");
  const { settingsApi } = await import("./ipc/settings");
  await initI18n(async () => (await settingsApi.getAll()).values[LANGUAGE_SETTING] as string | undefined);
  // Features load after the mocks so anything they invoke at startup has a backend.
  await import("./features");
  // The Settings dialog window shows a form: it skips the IDE-only features.
  if (dialogBoot()?.id !== "settings") await import("./features/ideOnly");
  const { App } = await import("./App");
  const { DialogHost, DialogWindowApp } = await import("./shell/window/DialogHost");
  // `?dialog=<id>`: a dialog window (see shell/window/dialogs.ts) shows only that dialog.
  const boot = dialogBoot();
  document.documentElement.dataset.window = boot ? "dialog" : "main";
  render(() => (boot ? <DialogWindowApp boot={boot} /> : <App />), root);

  // In-page dialogs (browser mode) and nested dialogs of a dialog window.
  const dialogHost = document.createElement("div");
  dialogHost.id = "dialog-host";
  document.body.appendChild(dialogHost);
  render(() => <DialogHost />, dialogHost);
}

// `?splash`: the startup splash window. It loads no features, so it shows at once. In
// browser mode the preview continues into the workbench when loading is done.
if (new URLSearchParams(location.search).has("splash")) {
  document.documentElement.dataset.window = "splash";
  const { SplashApp } = await import("./splash/SplashApp");
  const dispose = render(
    () => (
      <SplashApp
        onDone={
          isTauri()
            ? undefined
            : () => {
                dispose();
                void bootWorkbench();
              }
        }
      />
    ),
    root,
  );
} else {
  await bootWorkbench();
}
