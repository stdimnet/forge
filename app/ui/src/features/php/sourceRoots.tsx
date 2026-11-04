/**
 * PHP source roots, like PHPStorm: Settings | Directories (Source and Test Source Folders
 * with their package prefix, derived from composer.json and changed by the user) and the
 * Project view's "Mark Directory as" › Sources Root / Test Sources Root / Unmark. Changes
 * go to the `php.sourceRoots` project setting; Rust merges them with composer.json
 * (`forge_file_templates::psr4::apply_overrides`) for New PHP Class, Move Class and the
 * "Namespace name doesn't match PSR-0/PSR-4 project structure" inspection. Source roots
 * get blue folders, test roots green ones.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { FolderOpen, Plus, RotateCcw, X } from "lucide-solid";
import type { FileEntry } from "@/ipc/workspace";
import {
  phpMetaApi,
  type SourceRootEntry,
  type SourceRootSetting,
} from "@/ipc/phpMeta";
import { templatesApi } from "@/ipc/templates";
import { getSetting, saveSetting } from "@/state/settings";
import type { SettingObject, SettingValue } from "@/ipc/settings";
import { isPluginActive } from "@/state/project";
import { workspace } from "@/state/workspace";
import { registerSettingsPage } from "@/features/settingsPages";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { item, type MenuEntry } from "@/features/contextMenu/state";
import { registerRowDecorator } from "@/features/project/ProjectView";
import {
  addStructureFolder,
  ProjectStructureSections,
  type StructureKind,
} from "@/features/projectStructure/DirectoriesSections";
import { t } from "@/i18n";

export const SOURCE_ROOTS_KEY = "php.sourceRoots";

type Kind = SourceRootSetting["kind"];

/** `overrides` with the entry for `path` replaced (`null` removes it). */
export function upsertRoot(
  overrides: SourceRootSetting[],
  path: string,
  next: SourceRootSetting | null,
): SourceRootSetting[] {
  const clean = path.replace(/^\.?\/+|\/+$/g, "");
  const rest = overrides.filter(
    (o) => o.path.replace(/^\.?\/+|\/+$/g, "") !== clean,
  );
  return next ? [...rest, { ...next, path: clean }] : rest;
}

/** The setting change for marking `path` as `kind` (or unmarking it with `null`). */
export function markChange(
  overrides: SourceRootSetting[],
  roots: SourceRootEntry[],
  path: string,
  kind: Exclude<Kind, "none"> | null,
  prefix: string,
): SourceRootSetting[] {
  const current = roots.find((r) => r.path === path);
  if (kind) {
    // Marking a composer.json root as it already is drops the change.
    if (
      current?.origin === "composer" &&
      !current.overridden &&
      current.kind === kind
    )
      return overrides;
    return upsertRoot(overrides, path, { path, prefix, kind });
  }
  if (!current) return overrides;
  // A composer.json root stays unmarked through a `none` entry; the user's own goes away.
  return current.origin === "composer"
    ? upsertRoot(overrides, path, {
        path,
        prefix: current.prefix,
        kind: "none",
      })
    : upsertRoot(overrides, path, null);
}

const storedOverrides = () =>
  (getSetting<SettingObject[]>(SOURCE_ROOTS_KEY, []) ??
    []) as unknown as SourceRootSetting[];

// The effective roots of the open project (for the tree and the menu).
const [roots, { refetch }] = createResource(
  () =>
    workspace() && isPluginActive("forge.php")
      ? JSON.stringify(storedOverrides())
      : null,
  () => phpMetaApi.sourceRoots().catch((): SourceRootEntry[] => []),
);

export const sourceRoots = () =>
  (roots() ?? []).filter((r) => r.kind !== "none");

registerRowDecorator((path, dir) => {
  if (!dir) return undefined;
  const r = sourceRoots().find((x) => x.path === path);
  if (!r) return undefined;
  return {
    class: r.kind === "test" ? "php-test-root" : "php-source-root",
    hint: r.prefix ? `${r.prefix}\\` : undefined,
  };
});

async function mark(
  entry: FileEntry,
  kind: Exclude<Kind, "none"> | null,
): Promise<void> {
  const all = roots() ?? [];
  // A new root's prefix: the namespace the folder maps to now (PSR-4), else none.
  const prefix = kind
    ? ((await templatesApi.phpNamespace(entry.path).catch(() => null)) ?? "")
    : "";
  const next = markChange(storedOverrides(), all, entry.path, kind, prefix);
  await saveSetting(
    SOURCE_ROOTS_KEY,
    next as unknown as SettingValue,
    "project",
  );
  void refetch();
}

/**
 * The PHP entries of the Project view's "Mark Directory as" (folders of PHP projects); the
 * submenu itself and the language-agnostic entries come from `features/projectStructure`.
 */
export function sourceRootItems(entry: FileEntry): MenuEntry[] {
  if (entry.kind !== "dir" || !isPluginActive("forge.php")) return [];
  const r = sourceRoots().find((x) => x.path === entry.path);
  const items: MenuEntry[] = [];
  // Like PHPStorm, the folder's current mark is offered as "Unmark".
  if (r?.kind !== "source")
    items.push(item("Sources Root", () => void mark(entry, "source")));
  if (r?.kind !== "test")
    items.push(item("Test Sources Root", () => void mark(entry, "test")));
  if (r)
    items.push(
      item(
        r.kind === "test"
          ? "Unmark as Test Sources Root"
          : "Unmark as Sources Root",
        () => void mark(entry, null),
      ),
    );
  return items;
}

const KIND_LABEL: Record<Kind, string> = {
  source: "Source Folders",
  test: "Test Source Folders",
  none: "Unmarked composer.json Roots",
};

/** Settings | Directories. */
function DirectoriesPage() {
  const overrides = () =>
    (draftValue<SettingObject[]>(SOURCE_ROOTS_KEY) ??
      []) as unknown as SourceRootSetting[];
  const [entries] = createResource(
    () => JSON.stringify(overrides()),
    () =>
      phpMetaApi.sourceRoots(overrides()).catch((): SourceRootEntry[] => []),
  );
  const [path, setPath] = createSignal("");
  const [kind, setKind] = createSignal<Exclude<Kind, "none"> | StructureKind>(
    "source",
  );
  const [prefix, setPrefix] = createSignal("");
  const set = (next: SourceRootSetting[]) =>
    setDraft(SOURCE_ROOTS_KEY, next as unknown as SettingValue);
  const add = () => {
    const p = path()
      .trim()
      .replace(/^\.?\/+|\/+$/g, "");
    if (!p) return;
    const k = kind();
    if (k === "source" || k === "test")
      set(
        upsertRoot(overrides(), p, {
          path: p,
          prefix: prefix().trim().replace(/\\+$/, ""),
          kind: k,
        }),
      );
    else addStructureFolder(p, k);
    setPath("");
    setPrefix("");
  };
  const of = (k: Kind) => (entries() ?? []).filter((e) => e.kind === k);
  return (
    <div class="php-settings php-dirs">
      <section>
        <h3>{t("langs.php.contentRoot")}</h3>
        <div class="php-dirs-root">
          <FolderOpen size={14} />
          <span>{workspace()?.root ?? ""}</span>
        </div>
        <p class="php-hint">
          {t("langs.php.foldersMarkedAsSourcesOr")}
        </p>
      </section>
      <For each={["source", "test", "none"] as Kind[]}>
        {(k) => (
          <Show when={k !== "none" || of(k).length > 0}>
            <section>
              <h3 class={`php-dirs-title php-dirs-${k}`}>{KIND_LABEL[k]}</h3>
              <div class="php-list php-dirs-list" data-kind={k}>
                <For
                  each={of(k)}
                  fallback={<div class="php-hint">{t("common.nothingToShow")}</div>}
                >
                  {(e) => (
                    <div class="php-list-row php-dirs-row" data-path={e.path}>
                      <span class="php-dirs-path">{e.path || "."}</span>
                      <input
                        class="php-dirs-prefix"
                        type="text"
                        placeholder={t("langs.php.packagePrefix")}
                        title={t("langs.php.packagePrefix")}
                        disabled={k === "none"}
                        value={e.prefix}
                        onChange={(ev) =>
                          set(
                            upsertRoot(overrides(), e.path, {
                              path: e.path,
                              prefix: ev.currentTarget.value
                                .trim()
                                .replace(/\\+$/, ""),
                              kind: k,
                            }),
                          )
                        }
                      />
                      <span class="php-hint php-dirs-origin">
                        {e.origin === "composer"
                          ? e.overridden
                            ? t("langs.php.composerJsonChanged")
                            : "composer.json"
                          : "added"}
                      </span>
                      <span class="php-dirs-actions">
                        <Show
                          when={k !== "none"}
                          fallback={
                            <button
                              class="icon-btn"
                              title={t("langs.php.markAgain")}
                              onClick={() =>
                                set(upsertRoot(overrides(), e.path, null))
                              }
                            >
                              <RotateCcw size={13} />
                            </button>
                          }
                        >
                          <Show when={e.overridden}>
                            <button
                              class="icon-btn"
                              title={t("langs.php.resetToComposerJson")}
                              onClick={() =>
                                set(upsertRoot(overrides(), e.path, null))
                              }
                            >
                              <RotateCcw size={13} />
                            </button>
                          </Show>
                          <button
                            class="icon-btn"
                            title={t("langs.php.unmark")}
                            onClick={() =>
                              set(
                                markChange(
                                  overrides(),
                                  entries() ?? [],
                                  e.path,
                                  null,
                                  e.prefix,
                                ),
                              )
                            }
                          >
                            <X size={13} />
                          </button>
                        </Show>
                      </span>
                    </div>
                  )}
                </For>
              </div>
            </section>
          </Show>
        )}
      </For>
      <ProjectStructureSections />
      <section>
        <h3>{t("langs.php.addFolder")}</h3>
        <div class="php-dirs-add">
          <input
            type="text"
            placeholder={t("langs.php.srcDomain")}
            value={path()}
            onInput={(e) => setPath(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
          />
          <input
            type="text"
            placeholder={t("langs.php.packagePrefix")}
            disabled={kind() !== "source" && kind() !== "test"}
            value={prefix()}
            onInput={(e) => setPrefix(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
          />
          <select
            value={kind()}
            onChange={(e) =>
              setKind(
                e.currentTarget.value as Exclude<Kind, "none"> | StructureKind,
              )
            }
          >
            <option value="source">{t("langs.php.sources")}</option>
            <option value="test">{t("langs.php.tests")}</option>
            <option value="resource">{t("langs.php.resources")}</option>
            <option value="testResource">{t("langs.php.testResources")}</option>
            <option value="excluded">{t("langs.php.excluded")}</option>
          </select>
          <button class="icon-btn" title={t("common.add")} onClick={add}>
            <Plus size={14} />
          </button>
        </div>
      </section>
    </div>
  );
}

registerSettingsPage({
  id: "directories",
  title: t("settings.page.directories"),
  group: "Directories",
  order: 0,
  keywords: [
    "sources root",
    "source folder",
    "test sources",
    "package prefix",
    "psr-4",
    "psr-0",
    "namespace",
    "mark directory as",
    "content root",
    "excluded",
    "exclude files",
    "resource root",
    "test resources",
    "cancel exclusion",
  ],
  component: DirectoriesPage,
});
