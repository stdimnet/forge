/**
 * Settings | Languages & Frameworks | SQL Dialects, like PHPStorm's: the global and project
 * dialects and per-path mappings (file or folder → dialect and the data source its names
 * resolve against; the most specific path wins). Edits go to the dialog draft.
 */
import { createSignal, For, Show } from "solid-js";
import { Minus, Plus } from "lucide-solid";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { SQL_DIALECTS, type DialectMapping } from "@/ipc/sql";
import type { SettingObject } from "@/ipc/settings";
import { sources } from "@/features/database/store";
import { t } from "@/i18n";

const GLOBAL = "sql.dialect.default";
const PROJECT = "sql.dialect.project";
const MAPPINGS = "sql.dialect.mappings";

export function SqlDialectsPage() {
  const [sel, setSel] = createSignal<number | null>(null);
  const mappings = () => (draftValue<SettingObject[]>(MAPPINGS) ?? []) as unknown as DialectMapping[];
  const store = (list: DialectMapping[]) => setDraft(MAPPINGS, list as unknown as SettingObject[]);
  const update = (i: number, change: Partial<DialectMapping>) => store(mappings().map((m, k) => (k === i ? { ...m, ...change } : m)));
  const add = () => {
    const list = mappings();
    let n = 1;
    while (list.some((m) => m.path === (n === 1 ? "database" : `database${n}`))) n++;
    store([...list, { path: n === 1 ? "database" : `database${n}`, dialect: "generic" }]);
    setSel(list.length);
  };
  const remove = () => {
    const i = sel();
    if (i === null) return;
    store(mappings().filter((_, k) => k !== i));
    setSel(null);
  };
  const sqlSources = () => sources().filter((s) => s.driver !== "redis" && s.driver !== "mongodb");
  return (
    <div class="sql-dialects-page" data-testid="sql-dialects-page">
      <div class="set-section-body">
        <div class="set-row">
          <div class="set-line">
            <span class="set-label">{t("tools.sqlEditor.globalSqlDialectLabel")}</span>
            <select class="set-input set-select" value={draftValue<string>(GLOBAL) ?? "generic"} onChange={(e) => setDraft(GLOBAL, e.currentTarget.value)}>
              <For each={SQL_DIALECTS}>{(d) => <option value={d.id}>{d.label}</option>}</For>
            </select>
          </div>
        </div>
        <div class="set-row">
          <div class="set-line">
            <span class="set-label">{t("tools.sqlEditor.projectSqlDialectLabel")}</span>
            <select class="set-input set-select" value={draftValue<string>(PROJECT) ?? ""} onChange={(e) => setDraft(PROJECT, e.currentTarget.value)}>
              <option value="">{t("tools.sqlEditor.ltGlobalGt")}</option>
              <For each={SQL_DIALECTS}>{(d) => <option value={d.id}>{d.label}</option>}</For>
            </select>
          </div>
        </div>
      </div>
      <div class="set-desc sql-dialects-intro">
        {t("tools.sqlEditor.filesAndFoldersWithTheir")}
      </div>
      <div class="set-list sql-mappings">
        <div class="set-list-toolbar">
          <button class="icon-btn" title={t("common.add")} onClick={add}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title={t("common.remove")} disabled={sel() === null} onClick={remove}>
            <Minus size={14} />
          </button>
        </div>
        <table class="set-table sql-mappings-table">
          <thead>
            <tr>
              <th>{t("common.path")}</th>
              <th>{t("tools.sqlEditor.sqlDialect2")}</th>
              <th>{t("tools.sqlEditor.dataSource")}</th>
            </tr>
          </thead>
          <tbody>
            <For each={mappings()}>
              {(m, i) => (
                <tr classList={{ selected: sel() === i() }} onClick={() => setSel(i())}>
                  <td>
                    <input class="set-input set-path sql-map-path" value={m.path} spellcheck={false} onChange={(e) => update(i(), { path: e.currentTarget.value.trim() })} />
                  </td>
                  <td>
                    <select class="set-input" value={m.dialect ?? ""} onChange={(e) => update(i(), { dialect: e.currentTarget.value as DialectMapping["dialect"] })}>
                      <option value="">{t("tools.sqlEditor.ltInheritedGt")}</option>
                      <For each={SQL_DIALECTS}>{(d) => <option value={d.id}>{d.label}</option>}</For>
                    </select>
                  </td>
                  <td>
                    <select class="set-input" value={m.dataSource ?? ""} onChange={(e) => update(i(), { dataSource: e.currentTarget.value })}>
                      <option value="">{t("tools.sqlEditor.ltNoneGt")}</option>
                      <For each={sqlSources()}>{(s) => <option value={s.id}>{s.name}</option>}</For>
                    </select>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <Show when={!mappings().length}>
          <div class="set-list-empty">{t("tools.sqlEditor.noMappingsEverySqlFile")}</div>
        </Show>
      </div>
    </div>
  );
}
