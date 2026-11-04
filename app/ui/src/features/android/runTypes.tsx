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
  description: t("langs.android.buildTheModuleSApk"),
  icon: Smartphone,
  color: "var(--success)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Gradle arguments",
  noRunTarget: true,
  defaults: () => ({ options: { module: "app", variant: "debug" } }),
  fields: [
    { key: "module", label: t("common.module"), required: true, placeholder: "app" },
    { key: "variant", label: t("langs.android.buildVariant"), placeholder: "debug" },
    {
      key: "device",
      label: t("langs.android.deployTo"),
      kind: "enum",
      options: () => [
        { value: "", label: t("langs.android.theFirstRunningDevice") },
        ...onlineDevices().map((d) => ({ value: d.serial, label: `${deviceLabel(d.serial)} (${d.serial})` })),
      ],
    },
    { key: "activity", label: t("langs.android.launchActivity"), placeholder: t("langs.android.emptyTheDefaultActivityOf") },
    { key: "apk", label: "APK", placeholder: t("langs.android.emptyAppBuildOutputsApk") },
  ],
  suggestName: (cfg) => opt(cfg, "module") || "app",
});
