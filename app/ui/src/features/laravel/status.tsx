/** Status bar widget "Laravel 12" and the settings page header. */
import { Show } from "solid-js";
import { openSettings } from "@/features/settings/dialogState";
import { ProjectTypeIcon } from "@/shell/ProjectTypeIcon";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { frameworkLabel, projectInfo } from "@/state/project";
import { laravelActive, laravelInfo } from "./state";
import { t } from "@/i18n";

const framework = () => projectInfo()?.frameworks.find((f) => f.id === "laravel");

/** `Laravel 12`, from the backend's analysis or the project detection. */
export function laravelLabel(): string {
  const v = laravelInfo()?.version ?? framework()?.version;
  return frameworkLabel({ id: "laravel", name: "Laravel", version: v, language: "php" });
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export function summary(): string {
  const i = laravelInfo();
  if (!i) return "";
  return [
    plural(i.routes, "route"),
    plural(i.models, "model"),
    plural(i.views, "view"),
    plural(i.components, "component"),
    plural(i.configKeys, "config key"),
  ].join(" · ");
}

export function LaravelStatus() {
  return (
    <button
      class="status-item status-project-type"
      data-status="laravel"
      title={t("langs.laravel.laravelClickToConfigureLaravel", { value: laravelInfo()?.version ?? framework()?.version ?? "", value2: summary() ? `\n${summary()}` : "" })}
      onClick={() => openSettings("lang.php.laravel")}
    >
      <ProjectTypeIcon type="laravel" size={14} />
      {laravelLabel()}
    </button>
  );
}

export const showLaravelStatus = () => laravelActive();

export function LaravelSettingsPage() {
  return (
    <div class="laravel-settings">
      <Show
        when={laravelActive()}
        fallback={<p class="laravel-settings-info muted">{t("langs.laravel.theOpenProjectDoesNot")}</p>}
      >
        <p class="laravel-settings-info">
          <ProjectTypeIcon type="laravel" size={16} />
          <span>
            <strong>{laravelLabel()}</strong> detected
            <Show when={summary()}>
              <span class="muted"> — {summary()}</span>
            </Show>
          </span>
        </p>
      </Show>
      <SchemaPage pageId="lang.php.laravel" />
    </div>
  );
}
