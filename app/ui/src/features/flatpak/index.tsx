/**
 * Settings | Tools | Flatpak: whether Forge runs in a Flatpak sandbox and starts host tools
 * through `flatpak-spawn --host`, plus the setting that controls it.
 */
import { Show, createResource } from "solid-js";
import { Box } from "lucide-solid";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { flatpakApi, type FlatpakStatus } from "@/ipc/flatpak";
import { getSetting } from "@/state/settings";
import { t } from "@/i18n";
import "./flatpak.css";

export const RUN_ON_HOST_SETTING = "system.flatpak.runOnHost";

/** The status line's text. */
export function flatpakStatusText(s: FlatpakStatus | undefined, runOnHost: boolean): string {
  if (!s) return t("settings.flatpak.status.loading");
  if (!s.inFlatpak) return t("settings.flatpak.status.native");
  const id = s.flatpakId ?? "Flatpak";
  return runOnHost ? t("settings.flatpak.status.host", { id }) : t("settings.flatpak.status.sandbox", { id });
}

function FlatpakPage() {
  const [status] = createResource(() => flatpakApi.status().catch(() => undefined));
  const runOnHost = () => getSetting<boolean>(RUN_ON_HOST_SETTING, true);
  return (
    <div class="flatpak-page">
      <div class="flatpak-status" data-testid="flatpak-status">
        <Box size={14} />
        <span>{flatpakStatusText(status(), runOnHost())}</span>
      </div>
      <Show when={status()?.inFlatpak && !status()?.hostOsVisible}>
        <div class="flatpak-note">{t("settings.flatpak.noHostOs")}</div>
      </Show>
      <SchemaPage pageId="tools.flatpak" />
    </div>
  );
}

registerSettingsPage({
  id: "tools.flatpak",
  title: t("settings.page.tools.flatpak"),
  group: "Tools",
  order: 90,
  keywords: ["flatpak", "sandbox", "flatpak-spawn", "host"],
  component: FlatpakPage,
});
