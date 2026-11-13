/**
 * Doctrine tool window: mapped entities with their table, fields (type, nullable, id) and
 * associations, like PHPStorm's Persistence view. Double-click navigates to the mapping.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { ArrowRight, ChevronDown, ChevronRight, Hash, KeyRound, Layers, Link2, RefreshCw, Search, Table2 } from "lucide-solid";
import type { DoctrineAssociation, DoctrineEntity, DoctrineField } from "@/ipc/doctrine";
import { revealSymbol } from "@/features/php/editorViews";
import { entities, loadEntities, loading } from "./store";
import { t } from "@/i18n";

const [expanded, setExpanded] = createSignal<Set<string>>(new Set(["App\\Entity\\Order"]));
const [selected, setSelected] = createSignal<string | null>(null);
const [filter, setFilter] = createSignal("");

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

const short = (fqn: string) => fqn.split("\\").pop() ?? fqn;

const KIND_LABEL: Record<DoctrineAssociation["kind"], string> = {
  manyToOne: "ManyToOne",
  oneToMany: "OneToMany",
  manyToMany: "ManyToMany",
  oneToOne: "OneToOne",
};

function toggle(key: string) {
  setExpanded((s) => {
    const n = new Set(s);
    if (n.has(key)) n.delete(key);
    else n.add(key);
    return n;
  });
}

function FieldRow(props: { entity: DoctrineEntity; field: DoctrineField }) {
  const key = () => `${props.entity.class}#${props.field.name}`;
  return (
    <div
      class="tree-row dc-row dc-member"
      classList={{ selected: selected() === key() }}
      onClick={() => setSelected(key())}
      onDblClick={() => void revealSymbol(props.entity.path, { line: props.field.line, name: props.field.name })}
    >
      <span class="dc-icon" classList={{ "dc-id": props.field.id, "dc-field": !props.field.id, "dc-embedded": props.field.embedded }}>
        {props.field.id ? <KeyRound size={13} /> : props.field.embedded ? <Layers size={13} /> : <Hash size={13} />}
      </span>
      <span class="dc-name">{props.field.name}</span>
      <span class="dc-type">
        {props.field.embedded ? short(props.field.type) : props.field.type}
        {props.field.length ? `(${props.field.length})` : ""}
      </span>
      <Show when={props.field.nullable}>
        <span class="dc-flag">nullable</span>
      </Show>
      <Show when={props.field.unique}>
        <span class="dc-flag">unique</span>
      </Show>
      <Show when={props.field.generated}>
        <span class="dc-flag">generated</span>
      </Show>
    </div>
  );
}

function AssociationRow(props: { entity: DoctrineEntity; assoc: DoctrineAssociation }) {
  const key = () => `${props.entity.class}#${props.assoc.name}`;
  const many = () => props.assoc.kind === "oneToMany" || props.assoc.kind === "manyToMany";
  return (
    <div
      class="tree-row dc-row dc-member"
      classList={{ selected: selected() === key() }}
      onClick={() => setSelected(key())}
      onDblClick={() => void revealSymbol(props.entity.path, { line: props.assoc.line, name: props.assoc.name })}
      title={props.assoc.mappedBy ? t("langs.doctrine.mappedBy", { mappedBy: props.assoc.mappedBy }) : props.assoc.inversedBy ? t("langs.doctrine.inversedBy", { inversedBy: props.assoc.inversedBy }) : ""}
    >
      <span class="dc-icon dc-assoc">
        <Link2 size={13} />
      </span>
      <span class="dc-name">{props.assoc.name}</span>
      <span class="dc-kind">{KIND_LABEL[props.assoc.kind]}</span>
      <ArrowRight size={11} class="dc-arrow" />
      <span class="dc-type dc-target">
        {short(props.assoc.target)}
        {many() ? "[]" : ""}
      </span>
    </div>
  );
}

function EntityNode(props: { entity: DoctrineEntity }) {
  const open = () => expanded().has(props.entity.class) || !!filter();
  return (
    <>
      <div
        class="tree-row dc-row dc-entity-row"
        classList={{ selected: selected() === props.entity.class }}
        data-entity={props.entity.name}
        onClick={() => {
          setSelected(props.entity.class);
          toggle(props.entity.class);
        }}
        onDblClick={() => void revealSymbol(props.entity.path, { line: props.entity.line, name: props.entity.name })}
        title={props.entity.class}
      >
        <span class="dc-chevron">{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>
        <span class="dc-icon" classList={{ "dc-entity": !props.entity.embeddable, "dc-embeddable": !!props.entity.embeddable }}>
          {props.entity.embeddable ? <Layers size={14} /> : <Table2 size={14} />}
        </span>
        <span class="dc-name dc-entity-name">{props.entity.name}</span>
        <Show when={props.entity.table}>
          <span class="dc-table">{props.entity.table}</span>
        </Show>
        <Show when={props.entity.embeddable}>
          <span class="dc-table">embeddable</span>
        </Show>
        <span class="dc-source">{props.entity.source}</span>
      </div>
      <Show when={open()}>
        <For each={props.entity.fields}>{(f) => <FieldRow entity={props.entity} field={f} />}</For>
        <For each={props.entity.associations}>{(a) => <AssociationRow entity={props.entity} assoc={a} />}</For>
      </Show>
    </>
  );
}

export function EntitiesView() {
  const shown = createMemo(() => {
    const f = filter().trim().toLowerCase();
    return entities().filter(
      (e) => !f || e.class.toLowerCase().includes(f) || e.table?.includes(f) || e.fields.some((x) => x.name.toLowerCase().includes(f)),
    );
  });
  return (
    <div class="dc-panel">
      <div class="dc-toolbar">
        <label class="sf-search dc-search">
          <Search size={13} />
          <input type="text" placeholder={t("langs.doctrine.filterEntitiesAndFields")} value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} spellcheck={false} />
        </label>
      </div>
      <div class="dc-tree" role="tree">
        <Show when={entities().length} fallback={<div class="placeholder-panel">{loading() ? t("langs.doctrine.readingMappingsEllipsis") : t("langs.doctrine.noDoctrineEntitiesFound")}</div>}>
          <For each={shown()}>{(e) => <EntityNode entity={e} />}</For>
        </Show>
      </div>
      <div class="dc-footer muted">
        {plural(entities().filter((e) => !e.embeddable).length, "entity", "entities")} ·{" "}
        {plural(entities().filter((e) => e.embeddable).length, "embeddable", "embeddables")}
      </div>
    </div>
  );
}

export function EntitiesActions() {
  return (
    <button class="icon-btn" classList={{ spinning: loading() }} title={t("common.refresh")} onClick={() => void loadEntities(true)}>
      <RefreshCw size={14} />
    </button>
  );
}
