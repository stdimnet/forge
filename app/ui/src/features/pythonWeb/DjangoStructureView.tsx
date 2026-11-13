/**
 * Django Structure tool window (PyCharm's): the settings in effect, every installed app of
 * the project with its models and their fields, and the template folders. Double-click (or
 * Enter on a selected row) opens the declaration.
 */
import { createSignal, For, onMount, Show, type JSX } from "solid-js";
import { Boxes, ChevronDown, ChevronRight, Database, FileCode, Folder, KeyRound, Link2, RefreshCw, Settings2, Table2 } from "lucide-solid";
import type { DjangoModel, WebLocation } from "@/ipc/pythonWeb";
import { loadWebProject, reveal, webLoading, webProject } from "./state";
import { t } from "@/i18n";

const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
const isOpen = (id: string) => !collapsed().has(id);
const toggle = (id: string) =>
  setCollapsed((s) => {
    const n = new Set(s);
    if (n.has(id)) n.delete(id);
    else n.add(id);
    return n;
  });

const RELATIONS = new Set(["ForeignKey", "OneToOneField", "ManyToManyField", "GenericForeignKey"]);

function Row(props: {
  id?: string;
  depth: number;
  icon: JSX.Element;
  label: JSX.Element;
  detail?: string;
  location?: WebLocation;
  children?: JSX.Element;
}) {
  const expandable = () => !!props.id && props.children !== undefined;
  return (
    <>
      <div
        class="tree-row pyweb-tree-row"
        style={{ "padding-left": `${6 + props.depth * 16}px` }}
        data-django-node={props.id}
        onClick={() => expandable() && toggle(props.id!)}
        onDblClick={() => props.location && void reveal(props.location)}
        title={props.location ? `${props.location.path}:${props.location.line + 1}` : undefined}
      >
        <span class="pyweb-chevron">
          <Show when={expandable()}>{isOpen(props.id!) ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
        </span>
        {props.icon}
        <span class="pyweb-tree-label">{props.label}</span>
        <Show when={props.detail}>
          <span class="muted pyweb-tree-detail">{props.detail}</span>
        </Show>
      </div>
      <Show when={expandable() && isOpen(props.id!)}>{props.children}</Show>
    </>
  );
}

function ModelNode(props: { model: DjangoModel; depth: number }) {
  const m = () => props.model;
  return (
    <Row
      id={`model:${m().app}.${m().name}`}
      depth={props.depth}
      icon={<Table2 size={13} class="pyweb-icon-model" />}
      label={m().name}
      detail={m().isAbstract ? "abstract" : m().bases.filter((b) => b !== "models.Model").join(", ")}
      location={m().location}
    >
      <For each={m().fields}>
        {(f) => (
          <Row
            depth={props.depth + 1}
            icon={RELATIONS.has(f.kind) ? <Link2 size={12} class="pyweb-icon-relation" /> : <KeyRound size={12} class="pyweb-icon-field" />}
            label={f.name}
            detail={f.target ? `${f.kind} → ${f.target}` : f.kind}
            location={f.location}
          />
        )}
      </For>
    </Row>
  );
}

export function DjangoStructureView() {
  onMount(() => {
    if (!webProject() && !webLoading()) void loadWebProject();
  });
  const django = () => webProject()?.django;
  const models = (app: string) => (webProject()?.models ?? []).filter((m) => m.app === app);
  const roots = () => (webProject()?.templateRoots ?? []).filter((r) => r.framework === "django" || r.framework === "");
  return (
    <div class="pyweb-structure">
      <Show when={django()} fallback={<div class="routes-empty">{webLoading() ? t("langs.pythonWeb.readingTheProjectEllipsis") : t("langs.pythonWeb.noDjangoProjectManagePy")}</div>}>
        {(d) => (
          <>
            <Row
              id="settings"
              depth={0}
              icon={<Settings2 size={13} class="pyweb-icon-settings" />}
              label={t("langs.pythonWeb.settings")}
              detail={d().settingsModule ?? t("langs.pythonWeb.djangoSettingsModuleNotFound")}
              location={d().settingsFile ? { path: d().settingsFile!, line: 0, character: 0 } : undefined}
            >
              <Row depth={1} icon={<FileCode size={12} />} label="manage.py" detail={d().manage} location={{ path: d().manage, line: 0, character: 0 }} />
              <Show when={d().rootUrlconf}>
                <Row depth={1} icon={<FileCode size={12} />} label={t("langs.pythonWeb.rootUrlconf")} detail={d().rootUrlconf} />
              </Show>
              <Row depth={1} icon={<FileCode size={12} />} label={t("langs.pythonWeb.templateEngine")} detail={d().jinja2 ? "Jinja2" : t("langs.pythonWeb.djangoTemplates")} />
            </Row>
            <Row id="apps" depth={0} icon={<Boxes size={13} class="pyweb-icon-app" />} label={t("langs.pythonWeb.apps")} detail={`${d().apps.filter((a) => a.dir).length} in the project`}>
              <For each={d().apps.filter((a) => a.dir)}>
                {(app) => (
                  <Row
                    id={`app:${app.label}`}
                    depth={1}
                    icon={<Boxes size={13} class="pyweb-icon-app" />}
                    label={app.label}
                    detail={app.entry}
                    location={{ path: `${app.dir}/models.py`, line: 0, character: 0 }}
                  >
                    <Show when={models(app.label).length} fallback={<Row depth={2} icon={<Database size={12} />} label={<span class="muted">{t("langs.pythonWeb.noModels")}</span>} />}>
                      <For each={models(app.label)}>{(m) => <ModelNode model={m} depth={2} />}</For>
                    </Show>
                  </Row>
                )}
              </For>
            </Row>
            <Row id="installed" depth={0} icon={<Boxes size={13} />} label={t("langs.pythonWeb.installedApps")} detail={`${d().apps.filter((a) => !a.dir).length}`}>
              <For each={d().apps.filter((a) => !a.dir)}>{(app) => <Row depth={1} icon={<Boxes size={12} />} label={app.entry} />}</For>
            </Row>
            <Row id="templates" depth={0} icon={<Folder size={13} class="pyweb-icon-folder" />} label={t("langs.pythonWeb.templates")} detail={`${webProject()?.templates.length ?? 0} templates`}>
              <For each={roots()}>{(r) => <Row depth={1} icon={<Folder size={12} class="pyweb-icon-folder" />} label={r.dir} detail={r.origin} />}</For>
            </Row>
          </>
        )}
      </Show>
    </div>
  );
}

export function DjangoStructureActions() {
  return (
    <button class="icon-btn" title={t("common.refresh")} onClick={() => void loadWebProject(true)}>
      <RefreshCw size={14} classList={{ spin: webLoading() }} />
    </button>
  );
}
