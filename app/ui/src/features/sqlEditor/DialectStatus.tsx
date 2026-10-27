/**
 * The SQL dialect widget of the status bar (active SQL file or console): shows the dialect
 * and the data source names resolve against; the popup switches both for the file (stored
 * as a SQL Dialects mapping of the project) and opens the settings page. Consoles use their
 * data source's dialect.
 */
import { Show } from "solid-js";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import { openSettings } from "@/features/settings/dialogState";
import { activeTab } from "@/state/editor";
import { getSetting, saveSetting } from "@/state/settings";
import type { SettingObject } from "@/ipc/settings";
import { showBalloon } from "@/keymap";
import { dialectLabel, SQL_DIALECTS, type DialectMapping, type SqlDialect } from "@/ipc/sql";
import { sourceDialect } from "@/ipc/db";
import { consoleSourceId, sourceById, sources } from "@/features/database/store";
import { consoleDialect, fileSql, refreshFileSql } from "./language";
import { reanalyze } from "./inspector";
import { isSqlEditor } from "./targets";

export const SQL_DIALECTS_PAGE = "lang.sqlDialects";
const MAPPINGS = "sql.dialect.mappings";

const mappingsSetting = (): DialectMapping[] => (getSetting<SettingObject[]>(MAPPINGS, []) ?? []) as unknown as DialectMapping[];

/** Set (or with `undefined`, clear) the dialect or data source of one file. */
export async function mapFile(path: string, change: { dialect?: SqlDialect | ""; dataSource?: string }): Promise<void> {
  const list = [...(mappingsSetting())];
  const i = list.findIndex((m) => m.path === path);
  const next: DialectMapping = { ...(i >= 0 ? list[i] : { path }), ...change };
  if (!next.dialect) delete next.dialect;
  if (!next.dataSource) delete next.dataSource;
  if (i >= 0) list.splice(i, 1);
  if (next.dialect || next.dataSource) list.push(next);
  try {
    await saveSetting(MAPPINGS, list as unknown as SettingObject[]);
  } catch (e) {
    showBalloon("Cannot save the SQL dialect", String(e instanceof Error ? e.message : e));
  }
  await refreshFileSql(path);
  reanalyze(path);
}

/** Whether the widget shows (the active tab holds SQL). */
export const showDialectStatus = (): boolean => {
  const tab = activeTab();
  return !!tab && isSqlEditor(tab.path);
};

export function DialectStatus() {
  const tab = () => activeTab();
  const consoleDs = () => {
    const t = tab();
    return t ? consoleSourceId(t.path) : undefined;
  };
  const dialect = (): SqlDialect | undefined => {
    const t = tab();
    if (!t) return undefined;
    const ds = consoleDs();
    if (ds) return consoleDialect(sourceDialect(sourceById(ds))) ?? undefined;
    return fileSql[t.path]?.dialect;
  };
  const dataSource = () => {
    const t = tab();
    const id = consoleDs() ?? (t ? fileSql[t.path]?.dataSourceId : undefined);
    return id ? sourceById(id) : undefined;
  };
  const open = (el: HTMLElement) => {
    const t = tab();
    if (!t) return;
    const console = !!consoleDs();
    const current = dialect();
    const hint = console ? "A console uses its data source's dialect" : undefined;
    const entries: ResolvedEntry[] = SQL_DIALECTS.map((d) => ({
      kind: "action" as const,
      label: d.label,
      checkable: true,
      checked: d.id === current,
      disabled: console,
      hint,
      run: () => void mapFile(t.path, { dialect: d.id }),
    }));
    if (!console) {
      const mapped = mappingsSetting().some((m) => m.path === t.path && m.dialect);
      entries.push({ kind: "action", label: "Use the Folder's or Project's Dialect", disabled: !mapped, run: () => void mapFile(t.path, { dialect: "" }) });
      entries.push("separator");
      const attached = fileSql[t.path]?.dataSourceId;
      entries.push({
        kind: "submenu",
        label: "Resolve Against",
        items: [
          { kind: "action", label: "<None>", checkable: true, checked: !attached, run: () => void mapFile(t.path, { dataSource: "" }) },
          ...sources()
            .filter((s) => s.driver !== "redis" && s.driver !== "mongodb")
            .map((s) => ({
              kind: "action" as const,
              label: s.name,
              checkable: true,
              checked: s.id === attached,
              run: () => void mapFile(t.path, { dataSource: s.id }),
            })),
        ],
      });
    }
    entries.push("separator");
    entries.push({ kind: "action", label: "SQL Dialects Settings…", run: () => openSettings(SQL_DIALECTS_PAGE) });
    const r = el.getBoundingClientRect();
    showResolvedMenu(entries, r.left, r.top - 2, { above: true, class: "status-popup" });
  };
  return (
    <Show when={dialect()}>
      {(d) => (
        <button
          class="status-item sql-dialect-status"
          data-status="sql-dialect"
          title={`SQL Dialect: ${dialectLabel(d())}${dataSource() ? ` · resolving against ${dataSource()!.name}` : ""}`}
          onClick={(e) => open(e.currentTarget)}
        >
          {dialectLabel(d())}
          <Show when={dataSource()}>{(ds) => <span class="sql-status-ds"> · {ds().name}</span>}</Show>
        </button>
      )}
    </Show>
  );
}
