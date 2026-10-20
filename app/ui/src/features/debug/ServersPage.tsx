/** Settings > Languages & Frameworks > PHP > Servers: hosts and their path mappings. */
import { createResource, createSignal, For, Show } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import { Copy, Minus, Plus, Server } from "lucide-solid";
import { workspace } from "@/state/workspace";
import { debugApi, type ServerConfig } from "./api";
import { slug } from "./mapping";

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
  const root = () => workspace()?.root ?? "";

  return (
    <div class="dbg-servers">
      <Show when={loaded()}>
        <div class="dbg-servers-list">
          <div class="dbg-bp-toolbar">
            <button class="icon-btn" title="Add" onClick={() => add()}>
              <Plus size={14} />
            </button>
            <button class="icon-btn" title="Remove" disabled={!current()} onClick={remove}>
              <Minus size={14} />
            </button>
            <button class="icon-btn" title="Copy" disabled={!current()} onClick={() => current() && add(current())}>
              <Copy size={13} />
            </button>
          </div>
          <div class="dbg-servers-items">
            <For each={servers} fallback={<div class="muted dbg-pad dbg-center">No servers configured</div>}>
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
          <Show when={current()} fallback={<div class="muted dbg-pad">Add a server to map paths of a remote or containerized PHP to the project.</div>}>
            {(s) => (
              <>
                <div class="dbg-form-row">
                  <label for="dbg-srv-name">Name:</label>
                  <input id="dbg-srv-name" class="dbg-input" value={s().name} onChange={(e) => edit((x) => (x.name = e.currentTarget.value.trim() || x.name))} />
                </div>
                <div class="dbg-form-row dbg-form-host">
                  <label for="dbg-srv-host">Host:</label>
                  <input id="dbg-srv-host" class="dbg-input" value={s().host} onChange={(e) => edit((x) => (x.host = e.currentTarget.value.trim()))} />
                  <label for="dbg-srv-port">Port:</label>
                  <input
                    id="dbg-srv-port"
                    class="dbg-input dbg-port"
                    type="number"
                    min={1}
                    max={65535}
                    value={s().port}
                    onChange={(e) => edit((x) => (x.port = Number(e.currentTarget.value) || 80))}
                  />
                  <label for="dbg-srv-dbg">Debugger:</label>
                  <select id="dbg-srv-dbg" class="dbg-input" value={s().debugger} onChange={(e) => edit((x) => (x.debugger = e.currentTarget.value))}>
                    <option value="Xdebug">Xdebug</option>
                  </select>
                </div>
                <label class="dbg-check dbg-form-check">
                  <input type="checkbox" checked={s().usePathMappings} onChange={(e) => edit((x) => (x.usePathMappings = e.currentTarget.checked))} />
                  Use path mappings (select if the server is remote or symlinks are used)
                </label>
                <div class="dbg-mappings" classList={{ disabled: !s().usePathMappings }}>
                  <div class="dbg-mappings-head">
                    <span>File/Directory</span>
                    <span>Absolute path on the server</span>
                  </div>
                  <div class="dbg-mapping-row dbg-mapping-root">
                    <span class="dbg-mono" title={root()}>
                      <span class="muted">Project files › </span>
                      {root() || "<project root>"}
                    </span>
                    <span class="dbg-mono">
                      {s().mappings.find((m) => m.local === "")?.remote ?? <span class="muted">not mapped</span>}
                    </span>
                  </div>
                  <For each={s().mappings}>
                    {(m, i) => (
                      <div class="dbg-mapping-row" data-mapping={i()}>
                        <input
                          class="dbg-input dbg-mono"
                          placeholder="project relative path (empty = project root)"
                          value={m.local}
                          disabled={!s().usePathMappings}
                          onChange={(e) => edit((x) => (x.mappings[i()].local = e.currentTarget.value.trim()))}
                        />
                        <input
                          class="dbg-input dbg-mono"
                          placeholder="/var/www/html"
                          value={m.remote}
                          disabled={!s().usePathMappings}
                          onChange={(e) => edit((x) => (x.mappings[i()].remote = e.currentTarget.value.trim()))}
                        />
                        <button class="icon-btn" title="Remove Mapping" disabled={!s().usePathMappings} onClick={() => edit((x) => x.mappings.splice(i(), 1))}>
                          <Minus size={13} />
                        </button>
                      </div>
                    )}
                  </For>
                  <button
                    class="dbg-link dbg-add-mapping"
                    disabled={!s().usePathMappings}
                    onClick={() => edit((x) => x.mappings.push({ local: x.mappings.some((m) => m.local === "") ? "vendor" : "", remote: "" }))}
                  >
                    <Plus size={12} /> Add mapping
                  </button>
                </div>
                <p class="muted dbg-hint">
                  Requests are matched by <code>PHP_IDE_CONFIG=serverName={s().name}</code>, otherwise by <code>$_SERVER['SERVER_NAME']</code> and port.
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
