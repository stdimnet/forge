/** Settings > Languages & Frameworks > PHP > Servers: hosts and their path mappings. */
import { createResource, createSignal, For, Show } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import { Copy, Minus, Plus, Server } from "lucide-solid";
import { debugApi, type ServerConfig } from "./api";
import { slug } from "./mapping";
import { MappingsTree } from "./MappingsTree";
import { t } from "@/i18n";

export function ServersPage() {
  const [servers, setServers] = createStore<ServerConfig[]>([]);
  const [selected, setSelected] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loaded] = createResource(async () => {
    const list = await debugApi.servers().catch(() => []);
    setServers(reconcile(list));
    setSelected(list[0]?.id ?? null);
    return true;
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  const persist = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      debugApi
        .saveServers(JSON.parse(JSON.stringify(servers)))
        .then(() => setError(null))
        .catch((e) => setError(String(e)));
    }, 300);
  };
  const index = () => servers.findIndex((s) => s.id === selected());
  const edit = (fn: (s: ServerConfig) => void) => {
    const i = index();
    if (i < 0) return;
    setServers(i, produce(fn));
    persist();
  };
  const uniqueId = (name: string) => {
    let id = slug(name);
    for (let n = 2; servers.some((s) => s.id === id); n++) id = `${slug(name)}-${n}`;
    return id;
  };
  const add = (from?: ServerConfig) => {
    const name = from ? `${from.name} (copy)` : servers.length ? `server-${servers.length + 1}` : "localhost";
    const s: ServerConfig = from
      ? { ...JSON.parse(JSON.stringify(from)), id: uniqueId(name), name }
      : { id: uniqueId(name), name, host: "localhost", port: 80, debugger: "Xdebug", usePathMappings: false, mappings: [] };
    setServers(servers.length, s);
    setSelected(s.id);
    persist();
  };
  const remove = () => {
    const i = index();
    if (i < 0) return;
    setServers((list) => list.filter((_, j) => j !== i));
    setSelected(servers[Math.min(i, servers.length - 1)]?.id ?? null);
    persist();
  };
  const current = () => servers[index()];

  return (
    <div class="dbg-servers">
      <Show when={loaded()}>
        <div class="dbg-servers-list">
          <div class="dbg-bp-toolbar">
            <button class="icon-btn" title={t("common.add")} onClick={() => add()}>
              <Plus size={14} />
            </button>
            <button class="icon-btn" title={t("common.remove")} disabled={!current()} onClick={remove}>
              <Minus size={14} />
            </button>
            <button class="icon-btn" title={t("debug.serversPage.copy")} disabled={!current()} onClick={() => current() && add(current())}>
              <Copy size={13} />
            </button>
          </div>
          <div class="dbg-servers-items">
            <For each={servers} fallback={<div class="muted dbg-pad dbg-center">{t("debug.serversPage.noServersConfigured")}</div>}>
              {(s) => (
                <div class="tree-row dbg-server-row" classList={{ selected: s.id === selected() }} onClick={() => setSelected(s.id)} data-server={s.id}>
                  <Server size={13} class="muted" />
                  <span>{s.name}</span>
                </div>
              )}
            </For>
          </div>
        </div>
        <div class="dbg-server-form">
          <Show when={current()} fallback={<div class="muted dbg-pad">{t("debug.serversPage.addAServerToMap")}</div>}>
            {(s) => (
              <>
                <div class="dbg-form-row">
                  <label for="dbg-srv-name">{t("debug.serversPage.name")}</label>
                  <input id="dbg-srv-name" class="dbg-input" value={s().name} onChange={(e) => edit((x) => (x.name = e.currentTarget.value.trim() || x.name))} />
                </div>
                <div class="dbg-form-row dbg-form-host">
                  <label for="dbg-srv-host">{t("debug.serversPage.host")}</label>
                  <input id="dbg-srv-host" class="dbg-input" value={s().host} onChange={(e) => edit((x) => (x.host = e.currentTarget.value.trim()))} />
                  <label for="dbg-srv-port">{t("debug.serversPage.port")}</label>
                  <input
                    id="dbg-srv-port"
                    class="dbg-input dbg-port"
                    type="number"
                    min={1}
                    max={65535}
                    value={s().port}
                    onChange={(e) => edit((x) => (x.port = Number(e.currentTarget.value) || 80))}
                  />
                </div>
                <div class="dbg-form-row">
                  <label for="dbg-srv-dbg">{t("debug.serversPage.debugger")}</label>
                  <select id="dbg-srv-dbg" class="dbg-input" value={s().debugger} onChange={(e) => edit((x) => (x.debugger = e.currentTarget.value))}>
                    <option value="Xdebug">Xdebug</option>
                  </select>
                </div>
                <label class="dbg-check dbg-form-check">
                  <input type="checkbox" checked={s().usePathMappings} onChange={(e) => edit((x) => (x.usePathMappings = e.currentTarget.checked))} />
                  {t("tools.debug.usePathMappingsSelectIf")}
                </label>
                <MappingsTree
                  mappings={s().mappings}
                  disabled={!s().usePathMappings}
                  onChange={(local, remote) =>
                    edit((x) => {
                      const i = x.mappings.findIndex((m) => m.local === local);
                      if (!remote) {
                        if (i >= 0) x.mappings.splice(i, 1);
                      } else if (i >= 0) x.mappings[i].remote = remote;
                      else x.mappings.push({ local, remote });
                    })
                  }
                />
                <p class="muted dbg-hint">
                  {t("debug.serversPage.matchHint", { config: `PHP_IDE_CONFIG=serverName=${s().name}` })}
                </p>
              </>
            )}
          </Show>
          <Show when={error()}>
            <p class="dbg-error-text">{error()}</p>
          </Show>
        </div>
      </Show>
    </div>
  );
}
