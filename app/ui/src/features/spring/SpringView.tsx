/**
 * Spring tool window (right stripe): Beans (stereotypes, `@Bean` methods, Spring Data
 * repositories, CDI beans and producers, with the beans a running application adds),
 * Persistence (JPA entities with their fields, relations and repositories) and Configuration
 * (configuration files, `@ConfigurationProperties` classes, metadata sources and the
 * `jakarta.*` / `javax.*` APIs in use). Everything is computed in Rust (`spring_model`).
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, Database, FileCog, KeyRound, Leaf, Link2, RefreshCw, Search, Table2, X } from "lucide-solid";
import type { BeanRow, Entity } from "@/ipc/spring";
import { openFile } from "@/state/editor";
import { loadSpring, loading, overview, reveal, shortName } from "./store";
import { t } from "@/i18n";

type Tab = "beans" | "persistence" | "configuration";
const [tab, setTab] = createSignal<Tab>("beans");
const [query, setQuery] = createSignal("");
const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set());

export { setTab as setSpringTab };

const KIND_LABELS: Record<string, string> = {
  component: "Component",
  service: "Service",
  repository: "Repository",
  controller: "Controller",
  restController: "REST Controller",
  configuration: "Configuration",
  configurationProperties: "@ConfigurationProperties",
  application: "Application",
  bean: "@Bean",
  dataRepository: "Spring Data",
  cdi: "CDI",
  producer: "@Produces",
  ejb: "EJB",
  resource: "JAX-RS Resource",
  runtime: "Runtime",
};

const toggle = (id: string) =>
  setOpen((s) => {
    const n = new Set(s);
    if (n.has(id)) n.delete(id);
    else n.add(id);
    return n;
  });

function matches(text: string): boolean {
  const q = query().trim().toLowerCase();
  return !q || q.split(/\s+/).every((w) => text.toLowerCase().includes(w));
}

function Beans() {
  const groups = createMemo(() => {
    const out = new Map<string, BeanRow[]>();
    for (const b of overview()?.beans ?? []) {
      if (!matches(`${b.name} ${b.type} ${KIND_LABELS[b.kind] ?? b.kind}`)) continue;
      const key = b.kind === "runtime" ? "runtime" : b.framework === "cdi" ? "cdi" : "spring";
      out.set(key, [...(out.get(key) ?? []), b]);
    }
    return [...out.entries()].sort((a, b) => ["spring", "cdi", "runtime"].indexOf(a[0]) - ["spring", "cdi", "runtime"].indexOf(b[0]));
  });
  const title: Record<string, string> = { spring: "Spring beans", cdi: "CDI beans", runtime: "Beans of the running application" };
  return (
    <div class="sp-tree" tabIndex={0}>
      <For each={groups()}>
        {([group, beans]) => (
          <>
            <div class="sp-tree-row sp-group" onClick={() => toggle(`g:${group}`)}>
              {open().has(`g:${group}`) ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
              <span class="sp-group-title">{title[group]}</span>
              <span class="sp-count">{beans.length}</span>
            </div>
            <Show when={!open().has(`g:${group}`)}>
              <For each={beans}>
                {(b) => (
                  <div class="sp-tree-row sp-bean" classList={{ runtime: b.kind === "runtime" }} title={b.type} data-bean={b.name} onDblClick={() => reveal(b.location)} onClick={() => reveal(b.location)}>
                    <Leaf size={13} class={`sp-bean-icon kind-${b.kind}`} />
                    <span class="sp-bean-name">{b.name}</span>
                    <span class="sp-bean-type">{shortName(b.type)}</span>
                    <span class="sp-badge">{KIND_LABELS[b.kind] ?? b.kind}</span>
                    <Show when={b.scope}>
                      <span class="sp-hint">{b.scope}</span>
                    </Show>
                    <Show when={b.profiles?.length}>
                      <span class="sp-hint">{t("langs.spring.profile", { profiles: b.profiles!.join(", ") })}</span>
                    </Show>
                    <span class="sp-spacer" />
                    <Show when={b.usages}>
                      <span class="sp-usages" title={t("langs.spring.injectedIntoPlaceS", { usages: b.usages })}>
                        <Link2 size={11} /> {b.usages}
                      </span>
                    </Show>
                    <Show when={b.live}>
                      <span class="sp-live-dot" title={t("langs.spring.existsInTheRunningApplication")} />
                    </Show>
                  </div>
                )}
              </For>
            </Show>
          </>
        )}
      </For>
      <Show when={!loading() && !groups().length}>
        <div class="sp-empty">
          <Leaf size={22} />
          <div>{overview()?.beans.length ? t("langs.spring.noBeansMatchTheSearch") : t("langs.spring.noBeansFound")}</div>
        </div>
      </Show>
    </div>
  );
}

function EntityNode(props: { e: Entity }) {
  const id = () => `e:${props.e.class}`;
  return (
    <>
      <div class="sp-tree-row sp-entity" data-entity={props.e.name} onClick={() => toggle(id())} onDblClick={() => reveal(props.e.location)}>
        {open().has(id()) ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <Table2 size={13} class="sp-entity-icon" />
        <span class="sp-bean-name">{props.e.name}</span>
        <Show when={props.e.table}>
          <span class="sp-hint">table {props.e.table}</span>
        </Show>
        <Show when={props.e.kind !== "entity"}>
          <span class="sp-badge">{props.e.kind === "embeddable" ? t("langs.spring.embeddable") : t("langs.spring.mappedSuperclass")}</span>
        </Show>
        <span class="sp-spacer" />
        <Show when={props.e.repositories?.length}>
          <span class="sp-hint" title={props.e.repositories!.join(", ")}>
            <Database size={11} /> {props.e.repositories!.map(shortName).join(", ")}
          </span>
        </Show>
      </div>
      <Show when={open().has(id())}>
        <For each={props.e.fields}>
          {(f) => (
            <div class="sp-tree-row sp-field" onClick={() => reveal({ ...props.e.location, line: f.line })}>
              <Show when={f.id} fallback={<span class="sp-field-dot" />}>
                <KeyRound size={11} class="sp-key" />
              </Show>
              <span class="sp-field-name">{f.name}</span>
              <span class="sp-field-type">{f.type}</span>
              <Show when={f.relation}>
                <span class="sp-badge">
                  {f.relation} → {f.target}
                </span>
              </Show>
              <Show when={f.column}>
                <span class="sp-hint">column {f.column}</span>
              </Show>
              <Show when={f.generated}>
                <span class="sp-hint">generated</span>
              </Show>
            </div>
          )}
        </For>
      </Show>
    </>
  );
}

function Persistence() {
  const list = createMemo(() => (overview()?.entities ?? []).filter((e) => matches(`${e.name} ${e.class} ${e.table ?? ""}`)));
  return (
    <div class="sp-tree" tabIndex={0}>
      <For each={list()}>{(e) => <EntityNode e={e} />}</For>
      <Show when={!loading() && !list().length}>
        <div class="sp-empty">
          <Table2 size={22} />
          <div>{t("langs.spring.noJpaEntitiesFoundEntity")}</div>
        </div>
      </Show>
    </div>
  );
}

function Configuration() {
  const o = () => overview();
  return (
    <div class="sp-config" tabIndex={0}>
      <section>
        <h4>{t("langs.spring.configurationFiles")}</h4>
        <For each={o()?.configFiles ?? []} fallback={<div class="muted">{t("langs.spring.noApplicationPropertiesApplicationYml")}</div>}>
          {(f) => (
            <div class="sp-tree-row" onClick={() => void openFile(f.path)}>
              <FileCog size={13} class="sp-file-icon" />
              <span class="sp-bean-name">{f.path.split("/").pop()}</span>
              <Show when={f.profile}>
                <span class="sp-badge">profile {f.profile}</span>
              </Show>
              <span class="sp-hint">{f.keys} keys</span>
              <span class="sp-spacer" />
              <span class="sp-hint sp-dir">{f.path.split("/").slice(0, -1).join("/")}</span>
            </div>
          )}
        </For>
      </section>
      <section>
        <h4>{t("langs.spring.configurationproperties")}</h4>
        <For each={o()?.configClasses ?? []} fallback={<div class="muted">{t("langs.spring.none")}</div>}>
          {(c) => (
            <div class="sp-tree-row" title={c.keys.join("\n")} onClick={() => reveal(c.location)}>
              <Leaf size={13} class="sp-bean-icon kind-configuration" />
              <span class="sp-bean-name">{c.prefix}</span>
              <span class="sp-bean-type">{shortName(c.class)}</span>
              <span class="sp-hint">{c.keys.length} properties</span>
            </div>
          )}
        </For>
      </section>
      <section>
        <h4>{t("langs.spring.profiles")}</h4>
        <div class="sp-chips">
          <For each={o()?.profiles ?? []}>{(p) => <span class="sp-badge">{p}</span>}</For>
        </div>
      </section>
      <section>
        <h4>{t("langs.spring.configurationMetadata")}</h4>
        <div class="sp-hint-block">
          {o()?.metadata.properties ?? 0} properties
          <Show when={o()?.metadata.sources.length} fallback={<span class="muted"> {t("langs.spring.noMetadataFoundReloadThe")}</span>}>
            {" "}from {o()!.metadata.sources.join(", ")}
          </Show>
        </div>
        <For each={o()?.metadata.problems ?? []}>{(p) => <div class="sp-problem">{p}</div>}</For>
      </section>
      <section>
        <h4>{t("langs.spring.apis")}</h4>
        <For each={o()?.apis ?? []} fallback={<div class="muted">{t("langs.spring.noSpringOrJakartaEe")}</div>}>
          {(a) => (
            <div class="sp-tree-row sp-api" data-api={a.id}>
              <span class={`sp-ns ns-${a.namespace}`}>{a.namespace}</span>
              <span class="sp-bean-name">{a.name}</span>
              <span class="sp-hint">{t("langs.spring.fileS", { files: a.files })}</span>
            </div>
          )}
        </For>
      </section>
    </div>
  );
}

export function SpringView() {
  onMount(() => {
    if (!overview() && !loading()) void loadSpring();
  });
  const counts = () => ({
    beans: overview()?.beans.length ?? 0,
    persistence: overview()?.entities.length ?? 0,
  });
  return (
    <div class="sp-view" data-testid="spring">
      <div class="sp-tabs" role="tablist">
        <button class="sp-tab" classList={{ active: tab() === "beans" }} onClick={() => setTab("beans")}>
          {t("langs.spring.beans")} <span class="sp-count">{counts().beans}</span>
        </button>
        <button class="sp-tab" classList={{ active: tab() === "persistence" }} onClick={() => setTab("persistence")}>
          {t("langs.spring.persistence")} <span class="sp-count">{counts().persistence}</span>
        </button>
        <button class="sp-tab" classList={{ active: tab() === "configuration" }} onClick={() => setTab("configuration")}>
          {t("langs.spring.configuration")}
        </button>
      </div>
      <Show when={tab() !== "configuration"}>
        <div class="sp-toolbar">
          <div class="sp-search sp-search-wide">
            <Search size={13} />
            <input class="sp-search-input" placeholder={tab() === "beans" ? t("langs.spring.searchBeans") : t("langs.spring.searchEntities")} value={query()} onInput={(e) => setQuery(e.currentTarget.value)} spellcheck={false} />
            <Show when={query()}>
              <button class="icon-btn sp-clear" title={t("common.clear")} onClick={() => setQuery("")}>
                <X size={12} />
              </button>
            </Show>
          </div>
        </div>
      </Show>
      <div class="sp-body">
        <Show when={tab() === "beans"}>
          <Beans />
        </Show>
        <Show when={tab() === "persistence"}>
          <Persistence />
        </Show>
        <Show when={tab() === "configuration"}>
          <Configuration />
        </Show>
      </div>
    </div>
  );
}

export function SpringActions() {
  return (
    <button class="icon-btn" title={t("langs.spring.refreshSpringModel")} onClick={() => void loadSpring(true)} disabled={loading()}>
      <RefreshCw size={14} class={loading() ? "sp-spin" : undefined} />
    </button>
  );
}
