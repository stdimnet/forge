/**
 * The Device Manager tool window, like Android Studio's: the devices and emulators adb
 * sees (`adb devices -l`) with their state, and the virtual devices (`emulator
 * -list-avds`) with a button that starts them. Rows offer Logcat and running the app on
 * the device. Without an Android SDK it shows where to get one; Forge never downloads it.
 */
import { For, onMount, Show } from "solid-js";
import { CircleAlert, MonitorSmartphone, Play, RefreshCw, ScrollText, Settings, Smartphone, TabletSmartphone } from "lucide-solid";
import { androidApi, type Device } from "@/ipc/android";
import { showBalloon } from "@/keymap";
import { openSettings } from "@/features/settings/dialogState";
import { showToolWindow } from "@/state/layout";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { devicesSnapshot, loadingDevices, refreshDevices, startLogcat } from "./store";
import { APP } from "./runTypes";
import { t } from "@/i18n";
import "./android.css";

const STATES: Record<string, string> = {
  device: "Online",
  offline: "Offline",
  unauthorized: "Unauthorized",
  "no permissions": "No permissions",
  recovery: "Recovery",
  bootloader: "Bootloader",
  sideload: "Sideload",
};

function name(d: Device) {
  return (d.avd ?? d.model ?? d.serial).replace(/_/g, " ");
}

export function openLogcat(serial: string) {
  showToolWindow("logcat");
  void startLogcat(serial);
}

function runOn(serial: string) {
  void launch(useTemporary({ name: "app", type: APP, options: { module: "app", variant: "debug", device: serial } }));
}

export function DevicesView() {
  onMount(() => void (devicesSnapshot() ? undefined : refreshDevices()));
  const snap = devicesSnapshot;
  const start = async (avd: string) => {
    try {
      await androidApi.startEmulator(avd);
      showBalloon(`Starting ${avd.replace(/_/g, " ")}`, t("langs.android.theEmulatorWindowOpensWhen"), 5000);
      setTimeout(() => void refreshDevices(), 4000);
    } catch (e) {
      showBalloon(t("langs.android.theEmulatorCouldNotBe"), String(e).replace(/^Error: /, ""));
    }
  };
  return (
    <div class="android-devices" data-testid="android-devices">
      <div class="java-toolbar">
        <button class="icon-btn" title={t("common.refresh")} data-testid="android-refresh" disabled={loadingDevices()} onClick={() => void refreshDevices()}>
          <RefreshCw size={14} />
        </button>
        <span class="java-toolbar-sep" />
        <button class="icon-btn" title={t("langs.android.androidSdkSettings")} onClick={() => openSettings("lang.android")}>
          <Settings size={14} />
        </button>
      </div>
      <Show when={snap()?.hint}>
        <div class="android-hint" data-testid="android-sdk-hint">
          <CircleAlert size={14} class="java-warn" />
          <span>{snap()!.hint}</span>
        </div>
      </Show>
      <Show when={snap()?.error}>
        <div class="android-hint">
          <CircleAlert size={14} class="java-error-icon" />
          <span>{snap()!.error}</span>
        </div>
      </Show>
      <div class="android-section">{t("langs.android.devices")}</div>
      <Show when={snap()?.devices.length} fallback={<div class="muted android-empty">{snap()?.adb ? t("langs.android.noDevicesStartAnEmulator") : ""}</div>}>
        <For each={snap()!.devices}>
          {(d) => (
            <div class="android-device-row" data-serial={d.serial}>
              {d.emulator ? <MonitorSmartphone size={16} class="android-device-icon" /> : <Smartphone size={16} class="android-device-icon" />}
              <div class="android-device-text">
                <span class="android-device-name">{name(d)}</span>
                <span class="muted android-device-detail">
                  {d.serial}
                  {d.usb ? t("langs.android.usb", { usb: d.usb }) : ""}
                  {d.product && !d.emulator ? ` · ${d.product}` : ""}
                </span>
              </div>
              <span class="android-state" classList={{ online: d.state === "device", problem: d.state !== "device" }}>
                {STATES[d.state] ?? d.state}
              </span>
              <button class="icon-btn" title="Logcat" disabled={d.state !== "device"} onClick={() => openLogcat(d.serial)}>
                <ScrollText size={14} />
              </button>
              <button class="icon-btn" title={t("langs.android.runAppOnThisDevice")} disabled={d.state !== "device"} onClick={() => runOn(d.serial)}>
                <Play size={14} class="java-ok" />
              </button>
            </div>
          )}
        </For>
      </Show>
      <div class="android-section">{t("langs.android.virtual")}</div>
      <Show when={snap()?.avds.length} fallback={<div class="muted android-empty">{snap()?.emulator ? t("langs.android.noVirtualDevicesCreateOne") : snap()?.hint ? "" : t("langs.android.theAndroidEmulatorIsNot")}</div>}>
        <For each={snap()!.avds}>
          {(a) => (
            <div class="android-device-row" data-avd={a.name}>
              <TabletSmartphone size={16} class="android-device-icon" />
              <div class="android-device-text">
                <span class="android-device-name">{a.name.replace(/_/g, " ")}</span>
                <span class="muted android-device-detail">{a.serial ? t("langs.android.runningAs", { serial: a.serial }) : t("langs.android.virtualDevice")}</span>
              </div>
              <Show
                when={a.serial}
                fallback={
                  <button class="icon-btn" title={t("langs.android.start")} data-testid="android-avd-start" onClick={() => void start(a.name)}>
                    <Play size={14} class="java-ok" />
                  </button>
                }
              >
                <span class="android-state online">{t("langs.android.running")}</span>
                <button class="icon-btn" title="Logcat" onClick={() => openLogcat(a.serial!)}>
                  <ScrollText size={14} />
                </button>
              </Show>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
