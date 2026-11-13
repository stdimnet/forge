/**
 * ER diagram tab (Diagrams | Show Diagram, Ctrl+Alt+Shift+U): the tables of a schema, or a
 * table with the tables it references and that reference it, laid out by
 * `forge_dbschema::diagram` and drawn as SVG — title bars, columns with primary and foreign
 * key markers, and foreign keys from column row to column row (dashed when nullable or
 * virtual). Scroll pans, Ctrl+wheel zooms, dragging the background pans; a click selects a
 * table and highlights its relations, a double click opens its data. Export as SVG or PNG.
 */
import { createEffect, createMemo, createSignal, For, on, onMount, Show } from "solid-js";
import { Download, Eye, Fullscreen, KeyRound, Link2, RefreshCw, Table2, ZoomIn, ZoomOut, Scan } from "lucide-solid";
import type { EditorTab } from "@/state/editor";
import { DIAGRAM_HEADER, DIAGRAM_ROW, dbSchemaApi, type ColumnsMode, type Diagram, type DiagramNode } from "@/ipc/dbSchema";
import { ContextMenu, type MenuItem } from "../ContextMenu";
import { defaultSchema, introspect, openTable, sourceById, sourceColor, states } from "../store";
import { exportDiagram } from "./exportImage";
import { loadSchema, parseDiagramPath } from "./model";
import { t } from "@/i18n";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));
const MIN_ZOOM = 0.2;
const MAX_ZOOM = 3;

export function DiagramView(props: { tab: EditorTab }) {
  const loc = parseDiagramPath(props.tab.path) ?? { id: "", schema: "" };
  const [diagram, setDiagram] = createSignal<Diagram | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [columns, setColumns] = createSignal<ColumnsMode>("all");
  const [hideViews, setHideViews] = createSignal(false);
  const [zoom, setZoom] = createSignal(1);
  const [pan, setPan] = createSignal({ x: 0, y: 0 });
  const [selected, setSelected] = createSignal<string | null>(loc.table ?? null);
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let host!: HTMLDivElement;
  let svg!: SVGSVGElement;
  let fitted = false;

  const schemaName = () => loc.schema || defaultSchema(loc.id);
  const schemaInfo = () => states[loc.id]?.introspected[schemaName()];

  async function layout(): Promise<void> {
    try {
      const schema = schemaInfo() ?? (await loadSchema(loc.id, schemaName()));
      const d = await dbSchemaApi.diagram({
        schema,
        tables: loc.table ? [loc.table] : [],
        neighbours: !!loc.table,
        columns: columns(),
        hideViews: hideViews(),
      });
      setDiagram(d);
      setError(null);
      if (!fitted) {
        fitted = true;
        queueMicrotask(fit);
      }
    } catch (e) {
      setError(errorText(e));
    }
  }

  onMount(() => void layout());
  createEffect(on([schemaInfo, columns, hideViews], () => void layout(), { defer: true }));

  /** Fit the whole diagram into the view (never above 100%). */
  function fit(): void {
    const d = diagram();
    if (!d || !host) return;
    const r = host.getBoundingClientRect();
    const z = Math.max(MIN_ZOOM, Math.min(1, (r.width - 16) / d.width, (r.height - 16) / d.height));
    setZoom(z);
    setPan({ x: Math.max(8, (r.width - d.width * z) / 2), y: Math.max(8, (r.height - d.height * z) / 2) });
  }

  /** Zoom around a point of the view (its centre by default). */
  function zoomTo(z: number, at?: { x: number; y: number }): void {
    const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
    const r = host.getBoundingClientRect();
    const p = at ?? { x: r.width / 2, y: r.height / 2 };
    const cur = zoom();
    const o = pan();
    setPan({ x: p.x - ((p.x - o.x) * next) / cur, y: p.y - ((p.y - o.y) * next) / cur });
    setZoom(next);
  }

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      const r = host.getBoundingClientRect();
      zoomTo(zoom() * (e.deltaY < 0 ? 1.1 : 1 / 1.1), { x: e.clientX - r.left, y: e.clientY - r.top });
    } else {
      const o = pan();
      setPan({ x: o.x - (e.shiftKey ? e.deltaY : e.deltaX), y: o.y - (e.shiftKey ? 0 : e.deltaY) });
    }
  };

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0 || (e.target as Element).closest(".erd-node")) return;
    setSelected(null);
    const start = { x: e.clientX, y: e.clientY, pan: pan() };
    const move = (ev: PointerEvent) => setPan({ x: start.pan.x + ev.clientX - start.x, y: start.pan.y + ev.clientY - start.y });
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      host.classList.remove("panning");
    };
    host.classList.add("panning");
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const related = createMemo(() => {
    const s = selected();
    const d = diagram();
    if (!s || !d) return new Set<string>();
    return new Set(d.edges.filter((e) => e.from === s || e.to === s).flatMap((e) => [e.from, e.to]));
  });

  const exportMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const d = diagram();
    if (!d) return;
    const base = loc.table ?? (schemaName() || "diagram");
    setMenu({
      x: r.left,
      y: r.bottom + 2,
      items: [
        { label: t("database.schema.diagramView.exportToSvg"), run: () => void exportDiagram(svg, d, base, "svg") },
        { label: t("database.schema.diagramView.exportToPng"), run: () => void exportDiagram(svg, d, base, "png") },
      ],
    });
  };

  const color = () => sourceColor(loc.id);
  const title = () => `${sourceById(loc.id)?.name ?? loc.id} › ${schemaName()}${loc.table ? ` › ${loc.table}` : ""}`;

  return (
    <div class="erd" data-testid="db-diagram">
      <div class="db-toolbar">
        <button class="icon-btn db-tool" title={t("database.schema.diagramView.zoomIn")} onClick={() => zoomTo(zoom() * 1.2)}>
          <ZoomIn size={15} />
        </button>
        <button class="icon-btn db-tool" title={t("database.schema.diagramView.zoomOut")} onClick={() => zoomTo(zoom() / 1.2)}>
          <ZoomOut size={15} />
        </button>
        <button class="icon-btn db-tool" title={t("database.schema.diagramView.actualSize")} onClick={() => zoomTo(1)}>
          <Scan size={14} />
        </button>
        <button class="icon-btn db-tool" title={t("database.schema.diagramView.fitContent")} onClick={fit}>
          <Fullscreen size={14} />
        </button>
        <span class="db-sep" />
        <div class="erd-segmented" role="group" aria-label={t("database.schema.diagramView.columns")}>
          <For each={[["all", "All Columns"], ["keys", "Keys"], ["none", "No Columns"]] as [ColumnsMode, string][]}>
            {([mode, label]) => (
              <button classList={{ active: columns() === mode }} onClick={() => setColumns(mode)}>
                {label}
              </button>
            )}
          </For>
        </div>
        <button class="db-chip-btn" classList={{ open: !hideViews() }} title={t("database.schema.diagramView.showViews")} onClick={() => setHideViews(!hideViews())}>
          <Eye size={12} /> {t("database.schema.diagramView.views")}
        </button>
        <span class="db-sep" />
        <button class="icon-btn db-tool" title={t("database.schema.diagramView.exportDiagram")} data-testid="erd-export" disabled={!diagram()} onClick={exportMenu}>
          <Download size={14} />
        </button>
        <button class="icon-btn db-tool" title={t("database.schema.diagramView.refresh")} onClick={() => void introspect(loc.id, schemaName())}>
          <RefreshCw size={14} />
        </button>
        <span class="db-spacer" />
        <Show when={color()}>{(c) => <span class="db-badge colored" style={{ "--db-accent": c().accent }}>{sourceById(loc.id)?.name}</span>}</Show>
        <span class="db-status muted">{title()}</span>
      </div>
      <div class="erd-canvas" ref={host} onWheel={onWheel} onPointerDown={onPointerDown}>
        <Show when={error()}>{(e) => <div class="erd-empty">{e()}</div>}</Show>
        <Show when={!error() && !diagram()}>
          <div class="erd-empty">
            <span class="db-spinner" /> {t("tools.database.loadingEllipsis2", { schemaName: schemaName() })}
          </div>
        </Show>
        <Show when={diagram()}>
          {(d) => (
            <svg ref={svg} class="erd-svg" width="100%" height="100%">
              <defs>
                <marker id="erd-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
                  <path d="M 0 0 L 10 5 L 0 10 z" class="erd-marker" />
                </marker>
                <marker id="erd-dot" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" markerHeight="6">
                  <circle cx="5" cy="5" r="4" class="erd-marker" />
                </marker>
              </defs>
              <g data-erd-viewport transform={`translate(${pan().x} ${pan().y}) scale(${zoom()})`}>
                <For each={d().edges}>
                  {(e) => (
                    <path
                      class="erd-edge"
                      classList={{ dashed: e.optional || e.virtual, active: selected() === e.from || selected() === e.to }}
                      d={e.points.map((p, i) => `${i ? "L" : "M"} ${p.x} ${p.y}`).join(" ")}
                      marker-start="url(#erd-dot)"
                      marker-end="url(#erd-arrow)"
                    >
                      <title>
                        {e.name}: {e.from}({e.columns.join(", ")}) → {e.to}({e.refColumns.join(", ")})
                        {e.virtual ? t("tools.database.virtual") : ""}
                      </title>
                    </path>
                  )}
                </For>
                <For each={d().nodes}>
                  {(n) => (
                    <TableNode
                      node={n}
                      selected={selected() === n.id}
                      dim={!!selected() && selected() !== n.id && !related().has(n.id)}
                      onSelect={() => setSelected(n.id)}
                      onOpen={() => openTable(loc.id, schemaName(), n.id)}
                    />
                  )}
                </For>
              </g>
            </svg>
          )}
        </Show>
      </div>
      <div class="erd-status">
        <Show when={diagram()}>
          {(d) => (
            <>
              <span>
                {t("tools.database.tableRelation", { count: d().nodes.length, value: d().nodes.length, count2: d().edges.length, value2: d().edges.length })}
              </span>
              <Show when={d().externalEdges}>
                <span class="muted"> {t("tools.database.toTablesNotShown", { externalEdges: d().externalEdges })}</span>
              </Show>
            </>
          )}
        </Show>
        <span class="db-spacer" />
        <span class="muted">{Math.round(zoom() * 100)}%</span>
      </div>
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </div>
  );
}

function TableNode(props: { node: DiagramNode; selected: boolean; dim: boolean; onSelect: () => void; onOpen: () => void }) {
  const n = () => props.node;
  const isView = () => n().kind === "view" || n().kind === "matview";
  const rowY = (i: number) => DIAGRAM_HEADER + i * DIAGRAM_ROW;
  return (
    <g
      class="erd-node"
      classList={{ selected: props.selected, dim: props.dim, view: isView(), focus: n().selected }}
      transform={`translate(${n().x} ${n().y})`}
      data-table={n().id}
      onPointerDown={(e) => {
        e.stopPropagation();
        props.onSelect();
      }}
      onDblClick={props.onOpen}
    >
      <title>{n().comment ? `${n().id}: ${n().comment}` : n().id}</title>
      <rect class="erd-box" width={n().width} height={n().height} rx="5" />
      <path class="erd-head" d={`M 0 5 a 5 5 0 0 1 5 -5 h ${n().width - 10} a 5 5 0 0 1 5 5 v ${DIAGRAM_HEADER - 5} h ${-n().width} z`} />
      <g transform="translate(8 7)" class="erd-head-icon">
        <Show when={isView()} fallback={<Table2 size={14} />}>
          <Eye size={14} />
        </Show>
      </g>
      <text class="erd-title" x="28" y={DIAGRAM_HEADER / 2 + 1}>
        {n().id}
      </text>
      <Show when={n().columns.length || n().hiddenColumns}>
        <line class="erd-rule" x1="0" x2={n().width} y1={DIAGRAM_HEADER} y2={DIAGRAM_HEADER} />
      </Show>
      <For each={n().columns}>
        {(c, i) => (
          <g transform={`translate(0 ${rowY(i())})`} class="erd-col" classList={{ virtual: c.virtual }}>
            <Show when={c.primary}>
              <g transform="translate(8 4)" class="erd-pk">
                <KeyRound size={12} />
              </g>
            </Show>
            <Show when={c.foreign && !c.primary}>
              <g transform="translate(8 4)" class="erd-fk">
                <Link2 size={12} />
              </g>
            </Show>
            <Show when={c.foreign && c.primary}>
              <g transform="translate(20 4)" class="erd-fk">
                <Link2 size={12} />
              </g>
            </Show>
            <text class="erd-name" classList={{ key: c.primary, unique: c.unique, nullable: c.nullable }} x={c.foreign && c.primary ? 36 : 26} y={DIAGRAM_ROW / 2 + 1}>
              {c.name}
            </text>
            <text class="erd-type" x={n().width - 8} y={DIAGRAM_ROW / 2 + 1}>
              {c.typeLabel}
              <title>
                {c.name} {c.dataType}
                {c.nullable ? "" : t("tools.database.notNull")}
              </title>
            </text>
          </g>
        )}
      </For>
      <Show when={n().hiddenColumns}>
        <text class="erd-more" x="26" y={rowY(n().columns.length) + DIAGRAM_ROW / 2 + 1}>
          {t("tools.database.more", { hiddenColumns: n().hiddenColumns })}
        </text>
      </Show>
    </g>
  );
}
