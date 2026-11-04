/**
 * Spring Boot and Jakarta EE state: whether the plugin applies to the open project and the
 * model the tool windows render (computed in Rust, `spring_model`), reloaded after source
 * and configuration changes and when a Spring Boot run changes.
 */
import { createRoot, createSignal } from "solid-js";
import { springApi, type EndpointRow, type SpringLocation, type SpringOverview } from "@/ipc/spring";
import { balloonsFor } from "@/keymap";
import { isPluginActive, usesFramework } from "@/state/project";
import { onFsChanged } from "@/state/fsWatch";
import { revealSymbol } from "@/features/php/editorViews";
import { openHttpRequest } from "@/features/scratches/store";
import { t } from "@/i18n";

export const PLUGIN_ID = "forge.spring";

const showBalloon = balloonsFor("Spring");

/** The plugin is enabled and the project uses Spring Boot or Jakarta EE. */
export const springActive = () => isPluginActive(PLUGIN_ID) && (usesFramework("spring-boot") || usesFramework("jakarta-ee"));

const [overview, setOverview] = createSignal<SpringOverview | null>(null);
const [loading, setLoading] = createSignal(false);
export { overview, loading };

let seq = 0;

/** Load (or with `refresh`, rescan) the model. */
export async function loadSpring(refresh = false): Promise<void> {
  if (!springActive()) {
    setOverview(null);
    return;
  }
  const id = ++seq;
  setLoading(true);
  try {
    const o = await springApi.model(refresh);
    if (id === seq) setOverview(o);
  } catch (err) {
    if (refresh) showBalloon(t("langs.spring.cannotReadTheSpringModel"), String(err).replace(/^Error: /, ""));
  } finally {
    if (id === seq) setLoading(false);
  }
}

let timer: ReturnType<typeof setTimeout> | undefined;

/** Reload soon (several changes in a row load once). */
export function scheduleReload(delay = 800): void {
  clearTimeout(timer);
  timer = setTimeout(() => void loadSpring(), delay);
}

/** Files the model reads. */
export const isSpringSource = (path: string) =>
  /\.(java|kt)$/.test(path) || /(^|\/)(application|bootstrap)(-[\w.-]+)?\.(properties|ya?ml)$/.test(path) || /spring-configuration-metadata\.json$/.test(path);

if (typeof document !== "undefined") {
  createRoot(() => {
    onFsChanged((c) => {
      if (springActive() && [...c.changed, ...c.removed].some(isSpringSource)) scheduleReload();
    });
    void springApi.onDashboard(() => {
      if (springActive()) scheduleReload(300);
    });
  });
}

/** Open a declaration. */
export function reveal(l: SpringLocation | undefined): void {
  if (l) void revealSymbol(l.path, { line: l.line });
}

/** Open in HTTP Client: an `.http` scratch with the endpoint's request. */
export async function openInHttpClient(e: EndpointRow): Promise<void> {
  try {
    await openHttpRequest(await springApi.httpRequest(e));
  } catch (err) {
    showBalloon(t("langs.spring.cannotOpenTheHttpClient"), String(err).replace(/^Error: /, ""));
  }
}

/** The URL of an endpoint at the running application (else the configured port). */
export function endpointUrl(e: EndpointRow): string {
  const base = overview()?.runtime?.url ?? "http://localhost:8080";
  return `${base.replace(/\/$/, "")}${e.path}`;
}

/** `com.acme.shop.OrderService` → `OrderService`. */
export const shortName = (q: string) => q.split(/[.$]/).pop() ?? q;
