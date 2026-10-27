/**
 * The Endpoints tool window (bottom stripe), shared by the packs that contribute endpoint
 * views (`registerEndpointsView`): Python web frameworks (PyForge) and Spring MVC / JAX-RS
 * (JavaForge). Shown while one of them applies to the open project.
 */
import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Route } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { activeEndpointsViews, currentEndpointsView, selectEndpointsView } from "./registry";
import "./endpoints.css";

function EndpointsToolWindow() {
  return (
    <div class="endpoints-window">
      <Show when={activeEndpointsViews().length > 1}>
        <div class="endpoints-views" role="tablist">
          <For each={activeEndpointsViews()}>
            {(v) => (
              <button
                class="endpoints-view-tab"
                classList={{ active: currentEndpointsView()?.id === v.id }}
                role="tab"
                data-endpoints-view={v.id}
                onClick={() => selectEndpointsView(v.id)}
              >
                {v.title}
              </button>
            )}
          </For>
        </div>
      </Show>
      <div class="endpoints-body">
        <Show when={currentEndpointsView()} keyed>
          {(v) => <Dynamic component={v.component} />}
        </Show>
      </div>
    </div>
  );
}

function EndpointsToolWindowActions() {
  return (
    <Show when={currentEndpointsView()?.actions} keyed>
      {(actions) => <Dynamic component={actions} />}
    </Show>
  );
}

registerToolWindow({
  id: "endpoints",
  title: "Endpoints",
  icon: Route,
  anchor: "bottom",
  order: 48,
  component: EndpointsToolWindow,
  actions: EndpointsToolWindowActions,
  when: () => activeEndpointsViews().length > 0,
});
