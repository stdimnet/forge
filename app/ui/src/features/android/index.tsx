/**
 * Android in the UI (JavaForge), like Android Studio. Everything is computed in Rust
 * (`commands/android.rs`, `packs/javaforge/android`); this module plugs Android into the
 * contribution points:
 *
 * - Settings | Languages & Frameworks | Android SDK (location, Logcat options);
 * - the Device Manager (`DevicesView.tsx`) and Logcat (`LogcatView.tsx`) tool windows;
 * - the Android App run configuration type (`runTypes.tsx`);
 * - a hint when the project is an Android project but no SDK is installed (Forge never
 *   downloads it).
 */
import { createResource, For, Show } from "solid-js";
import { CircleAlert, CircleCheck, MonitorSmartphone, ScrollText } from "lucide-solid";
import { isDialogWindow } from "@/shell/window/mode";
import { androidApi } from "@/ipc/android";
import { balloonsFor } from "@/keymap/dispatch";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerToolWindow } from "@/features/registry";
import { openSettings } from "@/features/settings/dialogState";
import { registerCommand } from "@/state/commands";
import { onProjectDetected } from "@/state/project";
import { DevicesView } from "./DevicesView";
import { LogcatView } from "./LogcatView";
import { active, clearLogcat, refreshDevices, refreshStatus } from "./store";
import "./runTypes";
import "@/features/java/java.css";
import "@/features/scala/scala.css";
import "./android.css";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Android");

function AndroidSdkPage() {
  const [status] = createResource(() => androidApi.status().catch(() => null));
  return (
    <div class="java-page" data-android-page>
      <div class="java-status">
        <div class="java-status-row">
          <Show when={status()?.sdk} fallback={<CircleAlert size={14} class="java-warn" />}>
            <CircleCheck size={14} class="java-ok" />
          </Show>
          <span>
            Android SDK: <b>{status()?.sdk ? `found (${status()!.sdk!.source})` : "not found"}</b>
            <Show when={status()?.sdk}>
              <span class="scala-path"> {status()!.sdk!.path}</span>
            </Show>
          </span>
        </div>
        <div class="java-status-row">
          <Show when={status()?.adb} fallback={<CircleAlert size={14} class="java-warn" />}>
            <CircleCheck size={14} class="java-ok" />
          </Show>
          <span>
            adb: <b>{status()?.adb ? "found" : "not found (SDK platform-tools)"}</b>
            <Show when={status()?.adb}>
              <span class="scala-path"> {status()!.adb}</span>
            </Show>
          </span>
        </div>
        <For each={status()?.modules ?? []}>
          {(m) => (
            <div class="java-status-row">
              <CircleCheck size={14} class="java-ok" />
              <span>
                Module <b>{m.gradlePath}</b> ({m.kind === "application" ? "application" : "library"}
                {m.applicationId ?? m.namespace ? `, ${m.applicationId ?? m.namespace}` : ""}
                {m.minSdk ? `, minSdk ${m.minSdk}` : ""}
                {m.targetSdk ? `, targetSdk ${m.targetSdk}` : ""})
              </span>
            </div>
          )}
        </For>
        <Show when={status()?.hint}>
          <p class="muted java-hint">{status()!.hint}</p>
        </Show>
      </div>
      <SchemaPage pageId="lang.android" />
    </div>
  );
}

registerSettingsPage({
  id: "lang.android",
  title: t("settings.page.lang.android"),
  group: "Languages & Frameworks",
  order: 29,
  keywords: ["android", "sdk", "adb", "logcat", "emulator", "avd", "ANDROID_HOME"],
  component: AndroidSdkPage,
});

registerToolWindow({
  id: "android-devices",
  title: t("toolwindow.android-devices.title"),
  icon: MonitorSmartphone,
  anchor: "right",
  order: 42,
  component: DevicesView,
  when: active,
});

registerToolWindow({
  id: "logcat",
  title: t("toolwindow.logcat.title"),
  icon: ScrollText,
  anchor: "bottom",
  order: 45,
  component: LogcatView,
  when: active,
});

registerCommand({ id: "android.refreshDevices", title: "Refresh Android Devices", category: "Android", run: () => void refreshDevices() });
registerCommand({ id: "android.clearLogcat", title: "Clear Logcat", category: "Android", run: () => void clearLogcat() });

let hinted = false;

if (typeof document !== "undefined" && !isDialogWindow()) {
  onProjectDetected(async () => {
    if (!active() || hinted) return;
    const s = await refreshStatus();
    if (!s || s.remote || s.sdk) return;
    hinted = true;
    showBalloon("Android SDK not found", s.hint ?? "", 0, [{ label: "Android SDK Settings…", run: () => openSettings("lang.android") }]);
  });
}
