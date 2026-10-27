/**
 * The Plan result tab of a console (Explain Plan): the plan tree parsed by
 * `forge_dbschema::plan` with operation, object, cost, rows and details, and the flame-style
 * view (DataGrip's "Show Visualization") whose bars are as wide as their share of the cost
 * (or rows, or simply equal shares when the database gives neither).
 */
import { createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, Flame, ListTree } from "lucide-solid";
import type { ExplainResult, Plan, PlanNode } from "@/ipc/dbSchema";

const fmt = (n: number | undefined) =>
  n === undefined ? "" : Math.abs(n) >= 1000 ? Math.round(n).toLocaleString("en-US") : String(Math.round(n * 100) / 100);

interface Row {
  node: PlanNode;
  depth: number;
}

function flatten(n: PlanNode, depth: number, closed: Set<number>, out: Row[] = []): Row[] {
  out.push({ node: n, depth });
  if (!closed.has(n.id)) for (const c of n.children ?? []) flatten(c, depth + 1, closed, out);
  return out;
}

function PlanTree(props: { plan: Plan; selected: number | null; onSelect: (id: number) => void }) {
  const [closed, setClosed] = createSignal<Set<number>>(new Set());
  const total = () => props.plan.totalCost ?? 0;
  /** The node's own share of the total cost, in percent. */
  const share = (n: PlanNode) => (total() ? Math.min(100, ((n.selfCost ?? 0) / total()) * 100) : 0);
  const toggle = (id: number) =>
    setClosed((c) => {
      const n = new Set(c);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  return (
    <div class="plan-tree" data-testid="plan-tree">
      <table>
        <thead>
          <tr>
            <th class="plan-op-col">Operation</th>
            <th>Object</th>
            <th class="num">Cost</th>
            <th class="num">Self</th>
            <th class="num">Rows</th>
            <th>Details</th>
          </tr>
        </thead>
        <tbody>
          <For each={flatten(props.plan.root, 0, closed())}>
            {(r) => (
              <tr classList={{ selected: props.selected === r.node.id }} onClick={() => props.onSelect(r.node.id)}>
                <td class="plan-op" style={{ "padding-left": `${6 + r.depth * 16}px` }}>
                  <span class="plan-chevron" onClick={(e) => (e.stopPropagation(), toggle(r.node.id))}>
                    <Show when={r.node.children?.length}>{closed().has(r.node.id) ? <ChevronRight size={12} /> : <ChevronDown size={12} />}</Show>
                  </span>
                  {r.node.operation}
                </td>
                <td class="plan-object">{r.node.object ?? ""}</td>
                <td class="num">
                  <Show when={r.node.cost !== undefined}>
                    <Show when={share(r.node) >= 2}>
                      <span class="plan-cost-bar" style={{ width: `${share(r.node)}%` }} />
                    </Show>
                    {fmt(r.node.cost)}
                  </Show>
                </td>
                <td class="num">{fmt(r.node.selfCost)}</td>
                <td class="num">{fmt(r.node.rows)}</td>
                <td class="plan-detail" title={r.node.detail}>
                  {r.node.detail ?? ""}
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </div>
  );
}

function PlanFlame(props: { plan: Plan; selected: number | null; onSelect: (id: number) => void }) {
  const depth = () => Math.max(...props.plan.flame.map((b) => b.depth)) + 1;
  return (
    <div class="plan-flame" data-testid="plan-flame">
      <div class="plan-flame-stack" style={{ height: `${depth() * 24}px` }}>
        <For each={props.plan.flame}>
          {(b) => (
            <div
              class="plan-bar"
              classList={{ selected: props.selected === b.id, narrow: b.width < 0.04 }}
              style={{ left: `${b.start * 100}%`, width: `calc(${b.width * 100}% - 2px)`, top: `${b.depth * 24}px`, "--plan-hue": `var(--graph-${b.depth % 7})` }}
              title={`${b.label}${b.cost !== undefined ? `\ncost ${fmt(b.cost)}` : ""}${b.rows !== undefined ? `\nrows ${fmt(b.rows)}` : ""}`}
              onClick={() => props.onSelect(b.id)}
            >
              <span>{b.label}</span>
            </div>
          )}
        </For>
      </div>
      <div class="plan-flame-legend muted">
        Bar width: {props.plan.weight === "cost" ? "estimated cost" : props.plan.weight === "rows" ? "estimated rows" : "operations (the database gives no estimates)"}
      </div>
    </div>
  );
}

export function PlanView(props: { result: ExplainResult }) {
  const [mode, setMode] = createSignal<"tree" | "flame">("tree");
  const [selected, setSelected] = createSignal<number | null>(null);
  return (
    <div class="db-result plan-view" data-testid="plan-view">
      <div class="db-toolbar small">
        <button class="db-chip-btn" classList={{ open: mode() === "tree" }} onClick={() => setMode("tree")}>
          <ListTree size={12} /> Tree
        </button>
        <button class="db-chip-btn" classList={{ open: mode() === "flame" }} data-testid="plan-flame-toggle" onClick={() => setMode("flame")}>
          <Flame size={12} /> Flame Graph
        </button>
        <span class="db-spacer" />
        <span class="db-status muted" title={props.result.sql}>
          <Show when={props.result.plan?.totalCost !== undefined}>total cost {fmt(props.result.plan?.totalCost)} · </Show>
          {props.result.elapsedMs} ms
        </span>
      </div>
      <Show when={props.result.plan} fallback={<div class="plan-error">{props.result.error ?? "No plan"}</div>}>
        {(p) => (
          <Show when={mode() === "tree"} fallback={<PlanFlame plan={p()} selected={selected()} onSelect={setSelected} />}>
            <PlanTree plan={p()} selected={selected()} onSelect={setSelected} />
          </Show>
        )}
      </Show>
    </div>
  );
}
