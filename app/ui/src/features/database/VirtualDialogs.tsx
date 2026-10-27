/**
 * "Add Virtual Foreign Key" (from a column) and "Add Virtual Column" (from a table) of the
 * Database tool window. Both are stored in the project (`.forge/database/virtual.toml`).
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { suspendKeymap } from "@/keymap";
import { addVirtualColumn, addVirtualForeignKey, states } from "./store";

export type VirtualDialog =
  | { kind: "fk"; ds: string; schema: string; table: string; column: string }
  | { kind: "column"; ds: string; schema: string; table: string };

const [dialog, setDialog] = createSignal<VirtualDialog | null>(null);
export const openVirtualDialog = (d: VirtualDialog) => setDialog(d);
const close = () => setDialog(null);

export function VirtualDialogs() {
  return (
    <Show when={dialog()}>
      {(d) => {
        const v = d();
        return v.kind === "fk" ? <ForeignKeyDialog {...v} /> : <ColumnDialog {...v} />;
      }}
    </Show>
  );
}

function Modal(props: { title: string; ok: string; okDisabled?: boolean; onOk: () => void; children: JSX.Element; testId: string }) {
  let root!: HTMLDivElement;
  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => root.querySelector<HTMLElement>("select, input")?.focus());
  return (
    <div class="db-modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        ref={root}
        class="db-modal"
        role="dialog"
        aria-label={props.title}
        data-testid={props.testId}
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "BUTTON" && !props.okDisabled) props.onOk();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <header class="db-modal-title">{props.title}</header>
        <div class="db-modal-body">{props.children}</div>
        <footer class="db-modal-actions">
          <span class="db-doc-spacer" />
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" disabled={props.okDisabled} onClick={() => props.onOk()}>
            {props.ok}
          </button>
        </footer>
      </div>
    </div>
  );
}

function ForeignKeyDialog(p: { ds: string; schema: string; table: string; column: string }) {
  /** Tables of the loaded schemas as `schema.table`. */
  const targets = createMemo(() =>
    Object.values(states[p.ds]?.introspected ?? {}).flatMap((s) => s.tables.filter((t) => t.kind !== "collection").map((t) => ({ schema: s.name, table: t }))),
  );
  const guess = () => {
    // `user_id` → `users`, `customer` → `customers`.
    const base = p.column.replace(/_?id$/i, "").toLowerCase();
    return targets().find((t) => [base, `${base}s`, `${base}es`].includes(t.table.name.toLowerCase())) ?? targets().find((t) => t.table.name !== p.table) ?? targets()[0];
  };
  const first = guess();
  const [target, setTarget] = createSignal(first ? `${first.schema}.${first.table.name}` : "");
  const selected = () => targets().find((t) => `${t.schema}.${t.table.name}` === target());
  const [refColumn, setRefColumn] = createSignal(first?.table.primaryKey[0] ?? first?.table.columns[0]?.name ?? "");
  const [name, setName] = createSignal("");
  const pick = (value: string) => {
    setTarget(value);
    const t = selected();
    setRefColumn(t?.table.primaryKey[0] ?? t?.table.columns[0]?.name ?? "");
  };
  const ok = async () => {
    const t = selected();
    if (!t || !refColumn()) return;
    const done = await addVirtualForeignKey(p.ds, {
      schema: p.schema,
      table: p.table,
      name: name().trim() || undefined,
      columns: [p.column],
      refSchema: t.schema === p.schema ? undefined : t.schema,
      refTable: t.table.name,
      refColumns: [refColumn()],
    });
    if (done) close();
  };
  return (
    <Modal title="Add Virtual Foreign Key" ok="Add" okDisabled={!selected() || !refColumn()} onOk={() => void ok()} testId="db-virtual-fk-dialog">
      <div class="db-form">
        <span class="db-form-label">Column:</span>
        <code class="db-form-value">
          {p.schema}.{p.table}.{p.column}
        </code>
        <span class="db-form-label">References table:</span>
        <select class="dsd-input" value={target()} onChange={(e) => pick(e.currentTarget.value)} data-testid="db-vfk-table">
          <For each={targets()}>{(t) => <option value={`${t.schema}.${t.table.name}`}>{`${t.schema}.${t.table.name}`}</option>}</For>
        </select>
        <span class="db-form-label">Column:</span>
        <select class="dsd-input" value={refColumn()} onChange={(e) => setRefColumn(e.currentTarget.value)} data-testid="db-vfk-column">
          <For each={selected()?.table.columns ?? []}>
            {(c) => <option value={c.name}>{`${c.name}  ${c.dataType}${selected()?.table.primaryKey.includes(c.name) ? "  (primary key)" : ""}`}</option>}
          </For>
        </select>
        <span class="db-form-label">Name:</span>
        <input class="dsd-input" placeholder={`vfk_${p.table}_${p.column}_${selected()?.table.name ?? ""}`} value={name()} spellcheck={false} onInput={(e) => setName(e.currentTarget.value)} />
      </div>
      <div class="dsd-hint muted">
        Virtual foreign keys are stored in the project (.forge/database/virtual.toml), not in the database. Navigation, Quick Documentation and join completion use them.
      </div>
    </Modal>
  );
}

function ColumnDialog(p: { ds: string; schema: string; table: string }) {
  const [name, setName] = createSignal("");
  const [expression, setExpression] = createSignal("");
  const [type, setType] = createSignal("");
  const ok = async () => {
    const done = await addVirtualColumn(p.ds, { schema: p.schema, table: p.table, name: name().trim(), expression: expression().trim(), type: type().trim() || undefined });
    if (done) close();
  };
  return (
    <Modal title="Add Virtual Column" ok="Add" okDisabled={!name().trim() || !expression().trim()} onOk={() => void ok()} testId="db-virtual-column-dialog">
      <div class="db-form">
        <span class="db-form-label">Table:</span>
        <code class="db-form-value">
          {p.schema}.{p.table}
        </code>
        <span class="db-form-label">Name:</span>
        <input class="dsd-input" value={name()} spellcheck={false} onInput={(e) => setName(e.currentTarget.value)} />
        <span class="db-form-label">Expression:</span>
        <input class="dsd-input" placeholder="price * quantity" value={expression()} spellcheck={false} onInput={(e) => setExpression(e.currentTarget.value)} />
        <span class="db-form-label">Type:</span>
        <input class="dsd-input" placeholder="numeric" value={type()} spellcheck={false} onInput={(e) => setType(e.currentTarget.value)} />
      </div>
    </Modal>
  );
}
