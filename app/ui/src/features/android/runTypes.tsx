/**
 * The Android App run configuration, like Android Studio's: build the module's APK with
 * Gradle (`assemble<Variant>`), install it on the device (`adb install -r`) and start the
 * launcher activity of the merged (or source) manifest. Built in Rust (`commands/android.rs`).
 */
import { Smartphone } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { registerRunConfigurationType } from "@/features/run/types";
import { deviceLabel, onlineDevices, PLUGIN_ID } from "./store";
import { t } from "@/i18n";

export const APP = "android-app";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";

registerRunConfigurationType({
  id: APP,
  name: "Android App",
  description: "Build the module's APK with Gradle, install it on a device or emulator with adb and start its launcher activity.",
  icon: Smartphone,
  color: "var(--success)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Gradle arguments",
  noRunTarget: true,
  defaults: () => ({ options: { module: "app", variant: "debug" } }),
  fields: [
    { key: "module", label: "Module", required: true, placeholder: "app" },
    { key: "variant", label: "Build variant", placeholder: "debug" },
    {
      key: "device",
      label: "Deploy to",
      kind: "enum",
      options: () => [
        { value: "", label: t("langs.android.theFirstRunningDevice") },
        ...onlineDevices().map((d) => ({ value: d.serial, label: `${deviceLabel(d.serial)} (${d.serial})` })),
      ],
    },
    { key: "activity", label: "Launch activity", placeholder: "Empty: the default activity of the manifest" },
    { key: "apk", label: "APK", placeholder: "Empty: app/build/outputs/apk/<variant>/app-<variant>.apk" },
  ],
  suggestName: (cfg) => opt(cfg, "module") || "app",
});
