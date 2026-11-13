/**
 * Chart mode of a result set (DataGrip's "Show Chart"): line, bar, area, pie and scatter
 * charts with X/Y column pickers, grouping and aggregation. The series are computed in Rust
 * (`forge_dataedit::chart`); this draws them as plain SVG with the theme's graph colours.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { AreaChart, BarChart3, ChartScatter, LineChart, PieChart } from "lucide-solid";
import type { Cell } from "@/ipc/db";
import { dataEditApi, type ChartAgg, type ChartData, type ChartKind, type ChartSpec } from "@/ipc/dataedit";
import type { GridColumn } from "./grid";
import { t as tr } from "@/i18n";

const color = (i: number) => `var(--graph-${i % 7})`;

const KINDS: { kind: ChartKind; title: string; icon: typeof BarChart3 }[] = [
  { kind: "bar", title: tr("database.chartView.bar"), icon: BarChart3 },
  { kind: "line", title: tr("database.chartView.line"), icon: LineChart },
  { kind: "area", title: tr("database.chartView.area"), icon: AreaChart },
  { kind: "pie", title: tr("database.chartView.pie"), icon: PieChart },
  { kind: "scatter", title: tr("database.chartView.scatter"), icon: ChartScatter },
];

/** 4-6 round tick values covering [lo, hi]. */
function ticks(lo: number, hi: number): number[] {
  if (lo === hi) {
    hi = lo + 1;
    lo = Math.min(0, lo);
  }
  const span = hi - lo;
  const step0 = Math.pow(10, Math.floor(Math.log10(span / 5)));
  const step = [1, 2, 5, 10].map((m) => m * step0).find((s) => span / s <= 6) ?? step0 * 10;
  const out: number[] = [];
  for (let v = Math.floor(lo / step) * step; v <= hi + step * 0.001; v += step) out.push(Number(v.toPrecision(12)));
  if (out[out.length - 1] < hi) out.push(out[out.length - 1] + step);
  return out;
}

const fmt = (v: number) => (Math.abs(v) >= 1e6 ? `${(v / 1e6).toFixed(1)}M` : Math.abs(v) >= 1e4 ? `${(v / 1e3).toFixed(0)}k` : String(Number(v.toPrecision(6))));

export function ChartView(props: { columns: GridColumn[]; rows: Cell[][] }) {
  const numeric = () => props.columns.filter((c) => c.kind === "number");
  const [spec, setSpec] = createSignal<ChartSpec>({ kind: "bar", x: "", y: [], group: null, agg: "sum" });

  // Defaults: X is the first text or date column, Y the first number column.
  createEffect(
    on(
      () => props.columns.map((c) => c.name).join("\u0000"),
      () => {
        const cols = props.columns;
        const x = cols.find((c) => c.kind === "date") ?? cols.find((c) => c.kind === "text" && !c.primary) ?? cols[0];
        const y = numeric().find((c) => c.name !== x?.name && !c.primary && !c.foreign) ?? numeric().find((c) => c.name !== x?.name);
        setSpec((s) => ({ ...s, x: x?.name ?? "", y: y ? [y.name] : [], agg: y ? "sum" : "count" }));
      },
    ),
  );

  const [data] = createResource(
    () => ({ spec: spec(), rows: props.rows, columns: props.columns.map((c) => c.name) }),
    async ({ spec: s, rows, columns }): Promise<ChartData | { error: string }> => {
      if (!s.x) return { error: "Pick an X column" };
      try {
        return await dataEditApi.chart(columns, rows, s);
      } catch (e) {
        return { error: e instanceof Error ? e.message : String(e) };
      }
    },
  );

  let box!: HTMLDivElement;
  const [size, setSize] = createSignal({ w: 800, h: 360 });
  onMount(() => {
    const ro = new ResizeObserver(() => setSize({ w: Math.max(320, box.clientWidth), h: Math.max(220, box.clientHeight) }));
    ro.observe(box);
    onCleanup(() => ro.disconnect());
  });

  const update = (patch: Partial<ChartSpec>) => setSpec((s) => ({ ...s, ...patch }));
  const toggleY = (name: string) => {
    const s = spec();
    const y = s.y.includes(name) ? s.y.filter((v) => v !== name) : [...s.y, name];
    update({ y, agg: !y.length ? "count" : s.agg === "count" ? "sum" : s.agg });
  };

  return (
    <div class="db-chart" data-testid="db-chart">
      <div class="db-chart-bar">
        <div class="db-seg">
          <For each={KINDS}>
            {(k) => (
              <button class="db-seg-btn" classList={{ active: spec().kind === k.kind }} title={k.title} onClick={() => update({ kind: k.kind })}>
                <k.icon size={14} />
              </button>
            )}
          </For>
        </div>
        <label class="db-chart-field">
          <span class="muted">X</span>
          <select value={spec().x} onChange={(e) => update({ x: e.currentTarget.value })}>
            <For each={props.columns}>{(c) => <option value={c.name}>{c.name}</option>}</For>
          </select>
        </label>
        <div class="db-chart-field db-chart-ys">
          <span class="muted">Y</span>
          <For each={numeric()}>
            {(c) => (
              <button class="db-ychip" classList={{ active: spec().y.includes(c.name) }} onClick={() => toggleY(c.name)}>
                {c.name}
              </button>
            )}
          </For>
        </div>
        <label class="db-chart-field">
          <span class="muted">{tr("database.chartView.groupBy")}</span>
          <select value={spec().group ?? ""} onChange={(e) => update({ group: e.currentTarget.value || null })}>
            <option value="">{tr("tools.database.none")}</option>
            <For each={props.columns.filter((c) => c.name !== spec().x)}>{(c) => <option value={c.name}>{c.name}</option>}</For>
          </select>
        </label>
        <label class="db-chart-field">
          <span class="muted">{tr("database.chartView.aggregate")}</span>
          <select value={spec().agg} onChange={(e) => update({ agg: e.currentTarget.value as ChartAgg })}>
            <For each={["sum", "avg", "min", "max", "count"] as ChartAgg[]}>{(a) => <option value={a}>{a.toUpperCase()}</option>}</For>
          </select>
        </label>
      </div>
      <div class="db-chart-box" ref={box}>
        <Show when={data()}>
          {(d) => (
            <Show when={!("error" in d())} fallback={<div class="db-side-empty muted">{(d() as { error: string }).error}</div>}>
              <Chart data={d() as ChartData} w={size().w} h={size().h} />
            </Show>
          )}
        </Show>
      </div>
    </div>
  );
}

function Chart(props: { data: ChartData; w: number; h: number }) {
  const M = { l: 56, r: 16, t: 34, b: 46 };
  const pw = () => props.w - M.l - M.r;
  const ph = () => props.h - M.t - M.b;
  const yt = createMemo(() => ticks(Math.min(0, props.data.min), Math.max(0, props.data.max)));
  const y0 = () => yt()[0];
  const y1 = () => yt()[yt().length - 1];
  const sy = (v: number) => M.t + ph() - ((v - y0()) / (y1() - y0() || 1)) * ph();
  const n = () => props.data.categories.length;
  const band = () => pw() / Math.max(1, n());
  const cx = (i: number) => M.l + band() * (i + 0.5);
  const labelEvery = () => Math.max(1, Math.ceil(n() / Math.max(1, Math.floor(pw() / 70))));
  const xt = createMemo(() => ticks(props.data.xMin, props.data.xMax));
  const sx = (v: number) => M.l + ((v - xt()[0]) / (xt()[xt().length - 1] - xt()[0] || 1)) * pw();

  const legend = () => (
    <g class="db-chart-legend">
      <For each={props.data.kind === "pie" ? props.data.categories : props.data.series.map((s) => s.name)}>
        {(name, i) => (
          <g transform={`translate(${M.l + i() * 120}, 12)`}>
            <rect width="10" height="10" rx="2" fill={color(i())} />
            <text x="14" y="9">
              {name.length > 14 ? `${name.slice(0, 14)}…` : name}
            </text>
          </g>
        )}
      </For>
    </g>
  );

  const pie = () => {
    const vals = (props.data.series[0]?.values ?? []).map((v) => Math.max(0, v ?? 0));
    const total = vals.reduce((a, b) => a + b, 0) || 1;
    const r = Math.min(pw(), ph()) / 2 - 4;
    const c = { x: M.l + pw() / 2, y: M.t + ph() / 2 };
    let a = -Math.PI / 2;
    return vals.map((v, i) => {
      const da = (v / total) * Math.PI * 2;
      const p = (ang: number) => `${c.x + r * Math.cos(ang)},${c.y + r * Math.sin(ang)}`;
      const mid = a + da / 2;
      const d = da >= Math.PI * 2 - 1e-9 ? `M ${c.x - r},${c.y} a ${r},${r} 0 1,0 ${2 * r},0 a ${r},${r} 0 1,0 ${-2 * r},0` : `M ${c.x},${c.y} L ${p(a)} A ${r},${r} 0 ${da > Math.PI ? 1 : 0},1 ${p(a + da)} Z`;
      a += da;
      return { d, i, pct: (v / total) * 100, lx: c.x + r * 0.68 * Math.cos(mid), ly: c.y + r * 0.68 * Math.sin(mid) };
    });
  };

  return (
    <svg class="db-chart-svg" width={props.w} height={props.h} viewBox={`0 0 ${props.w} ${props.h}`}>
      {legend()}
      <Show
        when={props.data.kind !== "pie"}
        fallback={
          <For each={pie()}>
            {(s) => (
              <g>
                <path d={s.d} fill={color(s.i)} class="db-chart-slice">
                  <title>
                    {props.data.categories[s.i]}: {props.data.series[0]?.values[s.i]} ({s.pct.toFixed(1)}%)
                  </title>
                </path>
                <Show when={s.pct >= 4}>
                  <text x={s.lx} y={s.ly} class="db-chart-pct" text-anchor="middle">
                    {s.pct.toFixed(0)}%
                  </text>
                </Show>
              </g>
            )}
          </For>
        }
      >
        <g class="db-chart-grid">
          <For each={yt()}>
            {(t) => (
              <g>
                <line x1={M.l} x2={M.l + pw()} y1={sy(t)} y2={sy(t)} />
                <text x={M.l - 6} y={sy(t) + 4} text-anchor="end">
                  {fmt(t)}
                </text>
              </g>
            )}
          </For>
        </g>
        <Show
          when={props.data.kind === "scatter"}
          fallback={
            <g class="db-chart-xlabels">
              <For each={props.data.categories}>
                {(c, i) => (
                  <Show when={i() % labelEvery() === 0}>
                    <text x={cx(i())} y={M.t + ph() + 16} text-anchor="middle">
                      {c.length > 12 ? `${c.slice(0, 12)}…` : c}
                    </text>
                  </Show>
                )}
              </For>
            </g>
          }
        >
          <g class="db-chart-xlabels">
            <For each={xt()}>
              {(t) => (
                <text x={sx(t)} y={M.t + ph() + 16} text-anchor="middle">
                  {fmt(t)}
                </text>
              )}
            </For>
          </g>
        </Show>
        <line class="db-chart-axis" x1={M.l} x2={M.l + pw()} y1={sy(Math.max(y0(), 0))} y2={sy(Math.max(y0(), 0))} />
        <For each={props.data.series}>
          {(s, si) => {
            const pts = () =>
              s.values
                .map((v, i) => (v === null ? null : { x: cx(i), y: sy(v), v, i }))
                .filter((p): p is { x: number; y: number; v: number; i: number } => !!p);
            switch (props.data.kind) {
              case "bar": {
                const k = props.data.series.length;
                const bw = () => Math.max(1, (band() * 0.8) / k);
                return (
                  <For each={pts()}>
                    {(p) => (
                      <rect
                        class="db-chart-bar-rect"
                        x={M.l + band() * p.i + band() * 0.1 + bw() * si()}
                        y={Math.min(p.y, sy(0))}
                        width={bw()}
                        height={Math.max(1, Math.abs(sy(0) - p.y))}
                        fill={color(si())}
                      >
                        <title>
                          {s.name} · {props.data.categories[p.i]}: {p.v}
                        </title>
                      </rect>
                    )}
                  </For>
                );
              }
              case "scatter":
                return (
                  <For each={s.values}>
                    {(v, i) => (
                      <circle cx={sx(s.xs?.[i()] ?? 0)} cy={sy(v ?? 0)} r="4" fill={color(si())} class="db-chart-dot">
                        <title>
                          {s.name}: ({s.xs?.[i()]}, {v})
                        </title>
                      </circle>
                    )}
                  </For>
                );
              default: {
                const line = () => pts().map((p) => `${p.x},${p.y}`).join(" ");
                const area = () => {
                  const ps = pts();
                  return ps.length ? `M ${ps[0].x},${sy(Math.max(y0(), 0))} L ${line()} L ${ps[ps.length - 1].x},${sy(Math.max(y0(), 0))} Z` : "";
                };
                return (
                  <g>
                    <Show when={props.data.kind === "area"}>
                      <path d={area()} fill={color(si())} class="db-chart-area" />
                    </Show>
                    <polyline points={line()} fill="none" stroke={color(si())} stroke-width="2" />
                    <For each={pts()}>
                      {(p) => (
                        <circle cx={p.x} cy={p.y} r="3" fill={color(si())}>
                          <title>
                            {s.name} · {props.data.categories[p.i]}: {p.v}
                          </title>
                        </circle>
                      )}
                    </For>
                  </g>
                );
              }
            }
          }}
        </For>
      </Show>
      <Show when={props.data.truncated || props.data.skipped}>
        <text x={props.w - M.r} y={props.h - 6} text-anchor="end" class="db-chart-note">
          {props.data.truncated ? tr("tools.database.categoriesNotShown", { truncated: props.data.truncated }) : ""}
          {props.data.truncated && props.data.skipped ? " · " : ""}
          {props.data.skipped ? tr("tools.database.nonNumericValuesSkipped", { skipped: props.data.skipped }) : ""}
        </text>
      </Show>
    </svg>
  );
}
