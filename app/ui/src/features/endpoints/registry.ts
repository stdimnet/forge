/**
 * The Endpoints tool window is shared by the language packs (PyCharm's / IntelliJ IDEA's
 * Endpoints): each pack contributes a view of its endpoints (Python web frameworks, Spring and
 * JAX-RS). With one active view the window shows it directly; with several (a polyglot
 * project) it shows a tab per view.
 *
 *   registerEndpointsView({ id: "spring", title: "Spring", order: 20, when: springActive, component, actions });
 */
import { createSignal, type Component, type JSX } from "solid-js";
import { showToolWindow } from "@/state/layout";

export interface EndpointsView {
  id: string;
  /** The tab title when several views are active. */
  title: string;
  order: number;
  /** The view applies to the open project (its plugin is active). */
  when: () => boolean;
  component: Component;
  /** Toolbar buttons of the view (Refresh, Open in HTTP Client). */
  actions?: () => JSX.Element;
}

const [views, setViews] = createSignal<EndpointsView[]>([]);
const [selected, setSelected] = createSignal<string | null>(null);

/** Add (or replace, by id) a view of the Endpoints tool window. */
export function registerEndpointsView(v: EndpointsView): void {
  setViews((prev) => [...prev.filter((x) => x.id !== v.id), v].sort((a, b) => a.order - b.order));
}

/** The views that apply to the open project. */
export const activeEndpointsViews = () => views().filter((v) => v.when());

/** The view shown: the chosen one while it is active, else the first active one. */
export const currentEndpointsView = (): EndpointsView | undefined => {
  const list = activeEndpointsViews();
  return list.find((v) => v.id === selected()) ?? list[0];
};

export const selectEndpointsView = (id: string) => setSelected(id);

/** Open the Endpoints tool window on a view. */
export function showEndpointsView(id: string): void {
  setSelected(id);
  showToolWindow("endpoints");
}
