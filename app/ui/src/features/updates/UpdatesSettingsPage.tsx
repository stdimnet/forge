/**
 * Settings | Appearance & Behavior | System Settings | Updates: whether this build can update,
 * the last check, Check Now, the skipped version, and the `updates.*` settings.
 */
import { createResource, Match, Show, Switch } from "solid-js";
import { Info, TriangleAlert } from "lucide-solid";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { updatesApi } from "@/ipc/updates";
import { openCheckForUpdates } from "./UpdateDialog";
import { t } from "@/i18n";

function lastChecked(secs?: number | null): string {
  if (!secs) return "Never";
  return new Date(secs * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

function UpdateStatus() {
  const [status, { refetch }] = createResource(() => updatesApi.status().catch(() => null));
  const checkNow = async () => {
    await openCheckForUpdates();
    void refetch();
  };
  return (
    <Show when={status()}>
      {(s) => (
        <div class="upd-status" data-testid="updates-status">
          <Switch>
            <Match when={s().mode === "notConfigured"}>
              <div class="upd-note warning">
                <TriangleAlert size={14} />
                <span>
                  {t("tools.updates.updatesAreNotConfiguredIn")}
                  <code>FORGE_UPDATER_PUBKEY</code>{t("tools.updates.soItCannotVerifyUpdates")}
                </span>
              </div>
            </Match>
            <Match when={s().mode === "devBuild"}>
              <div class="upd-note">
                <Info size={14} />
                <span>
                  {t("tools.updates.developmentBuildsDoNotCheck")}
                  <code>FORGE_SIMULATE_UPDATE=0.2.0</code>{t("tools.updates.orAdd")} <code>?simulateUpdate=0.2.0</code> {t("tools.updates.inBrowserMode")}
                </span>
              </div>
            </Match>
            <Match when={s().mode === "simulated"}>
              <div class="upd-note">
                <Info size={14} />
                <span>{t("tools.updates.updatesAreSimulatedTheOffered")}</span>
              </div>
            </Match>
          </Switch>
          <div class="upd-status-row">
            <span class="label">{t("tools.updates.currentVersion")}</span>
            <span class="mono">{s().currentVersion}</span>
          </div>
          <Show when={s().mode === "enabled"}>
            <div class="upd-status-row">
              <span class="label">{t("tools.updates.lastChecked")}</span>
              <span>{lastChecked(s().lastCheck)}</span>
            </div>
          </Show>
          <Show when={s().skippedVersion}>
            {(v) => (
              <div class="upd-status-row">
                <span class="label">{t("tools.updates.skippedVersion")}</span>
                <span class="mono">{v()}</span>
                <button
                  class="btn-secondary"
                  onClick={() => void updatesApi.clearSkipped().then(() => refetch())}
                  data-testid="updates-unskip"
                >
                  {t("tools.updates.offerAgain")}
                </button>
              </div>
            )}
          </Show>
          <div class="upd-status-row">
            <button class="btn-secondary" onClick={() => void checkNow()} data-testid="updates-check-now">
              {t("tools.updates.checkNow")}
            </button>
          </div>
        </div>
      )}
    </Show>
  );
}

export function UpdatesSettingsPage() {
  return <SchemaPage pageId="system.updates" sectionHeader={(s) => (s === "Automatic Checks" ? <UpdateStatus /> : null)} />;
}
