/**
 * Schema tools state shared by the views: editor tab paths of ER diagrams and Compare
 * Structure, and loading a schema of a data source (connecting and introspecting once).
 */
import { openViewTab } from "@/state/editor";
import type { SchemaInfo } from "@/ipc/db";
import { connect, defaultSchema, introspect, sourceById, states } from "../store";

export const DIAGRAM_VIEW = "db-diagram";
export const COMPARE_VIEW = "db-compare";

export const diagramPath = (id: string, schema: string, table?: string) =>
  `db-diagram://${id}/${schema}${table ? `/${table}` : ""}`;

export function parseDiagramPath(path: string): { id: string; schema: string; table?: string } | null {
  const m = /^db-diagram:\/\/([^/]+)\/([^/]*)(?:\/(.+))?$/.exec(path);
  return m ? { id: m[1], schema: m[2], table: m[3] } : null;
}

/** Open the ER diagram of a schema, or of a table with its related tables. */
export function openDiagram(id: string, schema?: string, table?: string): void {
  const s = schema || defaultSchema(id);
  const name = table ? `${table} (diagram)` : `${s || sourceById(id)?.name || id} (diagram)`;
  openViewTab({ path: diagramPath(id, s, table), name, view: DIAGRAM_VIEW });
}

/** What a Compare Structure tab starts with. */
export interface CompareStart {
  source?: { id: string; schema: string };
  target?: { id: string; schema: string };
}

const compareStarts = new Map<string, CompareStart>();
let compareSeq = 0;

export const comparePath = (n: number) => `db-compare://${n}`;

/** The start of a Compare Structure tab (read once). */
export function takeCompareStart(path: string): CompareStart {
  const s = compareStarts.get(path) ?? {};
  compareStarts.delete(path);
  return s;
}

/** Open Compare Structure (DataGrip: Ctrl+D on two schemas). */
export function openCompare(start: CompareStart = {}): void {
  const path = comparePath(++compareSeq);
  compareStarts.set(path, start);
  openViewTab({ path, name: "Compare Structure", view: COMPARE_VIEW });
}

/** A schema of a data source: connects and introspects it when it is not loaded yet. */
export async function loadSchema(id: string, schema: string): Promise<SchemaInfo> {
  if (!(await connect(id))) throw new Error(states[id]?.error ?? `Cannot connect to ${sourceById(id)?.name ?? id}`);
  const name = schema || defaultSchema(id);
  if (!states[id]?.introspected[name]) await introspect(id, name);
  const info = states[id]?.introspected[name];
  if (!info) throw new Error(`Schema ${name} is not available`);
  return info;
}

/** Unix seconds now (migration file names). */
export const nowSeconds = () => Math.floor(Date.now() / 1000);
