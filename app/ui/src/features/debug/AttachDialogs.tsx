/**
 * Run | Attach to Process… (Ctrl+Alt+F5), Attach to DAP Server… and Debug Core Dump…:
 * start a debug adapter session without a run configuration.
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { RefreshCw } from "lucide-solid";
import { showToolWindow } from "@/state/layout";
import { DialogFrame } from "./DialogFrame";
import { debugAdapters, type DebugAdapter } from "./adapters";
import { dapApi, type LaunchRequest, type ProcessInfo } from "./dapApi";

export type AttachKind = "process" | "server" | "coreDump";

const [attachDialog, setAttachDialog] = createSignal<AttachKind | null>(null);
export { attachDialog, setAttachDialog };

const errorText = (e: unknown) => String(e).replace(/^Error: /, "");

/** Start the session; on success close the dialog and show the Debug tool window. */
async function start(request: LaunchRequest, setError: (e: string | null) => void, setBusy: (b: boolean) => void) {
  setBusy(true);
  setError(null);
  try {
    await dapApi.start(request);
    setAttachDialog(null);
    showToolWindow("debug");
  } catch (e) {
    setError(errorText(e));
  } finally {
    setBusy(false);
  }
}

function AdapterSelect(props: { id: string; adapters: DebugAdapter[]; value: string; onChange: (v: string) => void }) {
  return (
    <select id={props.id} class="dbg-input dbg-adapter-select" value={props.value} onChange={(e) => props.onChange(e.currentTarget.value)}>
      <For each={props.adapters}>{(a) => <option value={a.type}>{a.label}</option>}</For>
    </select>
  );
}

function NoAdapters(props: { what: string }) {
  return (
    <p class="muted dbg-pad">
      No installed debug adapter can {props.what}. Enable a language pack with a debugger, or set the LLDB or GDB path in Settings | Build,
      Execution, Deployment | Debugger.
    </p>
  );
}

function ProcessDialog() {
  const adapters = createMemo(() => debugAdapters().filter((a) => a.attachProcess));
  const [adapter, setAdapter] = createSignal(adapters()[0]?.type ?? "");
  const [filter, setFilter] = createSignal("");
  const [selected, setSelected] = createSignal<ProcessInfo | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [processes, { refetch }] = createResource(() => dapApi.processes().catch((e) => (setError(errorText(e)), [] as ProcessInfo[])));
  const shown = createMemo(() => {
    const f = filter().trim().toLowerCase();
    return (processes() ?? []).filter((p) => !f || p.name.toLowerCase().includes(f) || p.commandLine.toLowerCase().includes(f) || String(p.pid).includes(f));
  });
  const attach = (p: ProcessInfo | null = selected()) => {
    const type = adapter() || adapters()[0]?.type;
    if (!p || !type) return;
    void start({ mode: "attachProcess", pid: p.pid, adapter: type, program: p.commandLine.split(" ")[0] || undefined, name: `${p.name} (${p.pid})` }, setError, setBusy);
  };
  return (
    <DialogFrame
      title="Attach to Process"
      class="dbg-attach"
      onClose={() => setAttachDialog(null)}
      footer={
        <>
          <span class="muted dbg-left">{processes.loading ? "Collecting processes…" : `${shown().length} processes`}</span>
          <button class="btn-secondary" onClick={() => setAttachDialog(null)}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!selected() || !adapter() || busy()} onClick={() => attach()}>
            {busy() ? "Attaching…" : "Attach"}
          </button>
        </>
      }
    >
      <Show when={adapters().length} fallback={<NoAdapters what="attach to processes" />}>
        <div class="dbg-form-row">
          <label for="dbg-attach-adapter">Debugger:</label>
          <AdapterSelect id="dbg-attach-adapter" adapters={adapters()} value={adapter()} onChange={setAdapter} />
          <input
            class="dbg-input dbg-attach-filter"
            placeholder="Filter by name, command line or PID"
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
            ref={(el) => queueMicrotask(() => el.focus())}
          />
          <button class="icon-btn" title="Refresh" onClick={() => void refetch()}>
            <RefreshCw size={13} />
          </button>
        </div>
        <div class="dbg-process-list" role="listbox" aria-label="Processes">
          <div class="dbg-process-row dbg-process-head">
            <span>PID</span>
            <span>Name</span>
            <span>User</span>
            <span>Command line</span>
          </div>
          <For each={shown()}>
            {(p) => (
              <div
                class="dbg-process-row"
                classList={{ selected: selected()?.pid === p.pid }}
                role="option"
                data-pid={p.pid}
                onClick={() => setSelected(p)}
                onDblClick={() => attach(p)}
                title={p.commandLine}
              >
                <span class="dbg-mono">{p.pid}</span>
                <span>{p.name}</span>
                <span class="muted">{p.user ?? ""}</span>
                <span class="muted dbg-mono">{p.commandLine}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
      <Show when={error()}>
        <div class="dbg-error dbg-pad">{error()}</div>
      </Show>
    </DialogFrame>
  );
}

function ServerDialog() {
  const adapters = createMemo(() => debugAdapters());
  const [adapter, setAdapter] = createSignal(adapters()[0]?.type ?? "");
  const [host, setHost] = createSignal("127.0.0.1");
  const [port, setPort] = createSignal("4711");
  const [options, setOptions] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const attach = () => {
    const p = Number(port());
    if (!Number.isInteger(p) || p <= 0 || p > 65535) return setError("Enter a port between 1 and 65535.");
    let configuration: Record<string, unknown> | undefined;
    if (options().trim()) {
      try {
        configuration = JSON.parse(options()) as Record<string, unknown>;
      } catch {
        return setError("The attach options are not valid JSON.");
      }
    }
    void start({ mode: "attachServer", host: host().trim() || "127.0.0.1", port: p, adapter: adapter(), configuration }, setError, setBusy);
  };
  return (
    <DialogFrame
      title="Attach to DAP Server"
      class="dbg-attach-server"
      onClose={() => setAttachDialog(null)}
      footer={
        <>
          <button class="btn-secondary" onClick={() => setAttachDialog(null)}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!adapter() || busy()} onClick={attach}>
            {busy() ? "Connecting…" : "Attach"}
          </button>
        </>
      }
    >
      <p class="muted dbg-hint">Connect to a debug adapter that listens on a TCP port, e.g. dlv dap --listen 127.0.0.1:4711 or debugpy --listen 5678.</p>
      <div class="dbg-form-grid">
        <label for="dbg-srv-adapter">Debugger:</label>
        <AdapterSelect id="dbg-srv-adapter" adapters={adapters()} value={adapter()} onChange={setAdapter} />
        <label for="dbg-srv-host">Host:</label>
        <input id="dbg-srv-host" class="dbg-input" value={host()} onInput={(e) => setHost(e.currentTarget.value)} />
        <label for="dbg-srv-port">Port:</label>
        <input id="dbg-srv-port" class="dbg-input dbg-port" value={port()} onInput={(e) => setPort(e.currentTarget.value)} />
        <label for="dbg-srv-options">Attach options:</label>
        <textarea
          id="dbg-srv-options"
          class="dbg-input dbg-mono dbg-json"
          placeholder={'{ "justMyCode": false }'}
          value={options()}
          onInput={(e) => setOptions(e.currentTarget.value)}
        />
      </div>
      <Show when={error()}>
        <div class="dbg-error dbg-pad">{error()}</div>
      </Show>
    </DialogFrame>
  );
}

function CoreDumpDialog() {
  const adapters = createMemo(() => debugAdapters().filter((a) => a.coreDump));
  const [adapter, setAdapter] = createSignal(adapters()[0]?.type ?? "");
  const [core, setCore] = createSignal("");
  const [program, setProgram] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const open = () => {
    if (!core().trim()) return setError("Choose the core dump file.");
    void start(
      { mode: "coreDump", core: core().trim(), program: program().trim() || undefined, adapter: adapter() || adapters()[0]?.type },
      setError,
      setBusy,
    );
  };
  return (
    <DialogFrame
      title="Debug Core Dump"
      class="dbg-core-dump"
      onClose={() => setAttachDialog(null)}
      footer={
        <>
          <button class="btn-secondary" onClick={() => setAttachDialog(null)}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!adapters().length || busy()} onClick={open}>
            {busy() ? "Opening…" : "OK"}
          </button>
        </>
      }
    >
      <Show when={adapters().length} fallback={<NoAdapters what="open core dumps" />}>
        <div class="dbg-form-grid">
          <label for="dbg-core-adapter">Debugger:</label>
          <AdapterSelect id="dbg-core-adapter" adapters={adapters()} value={adapter()} onChange={setAdapter} />
          <label for="dbg-core-file">Core file:</label>
          <input id="dbg-core-file" class="dbg-input dbg-mono" placeholder="/var/crash/core.4242" value={core()} onInput={(e) => setCore(e.currentTarget.value)} />
          <label for="dbg-core-program">Symbol file:</label>
          <input
            id="dbg-core-program"
            class="dbg-input dbg-mono"
            placeholder="The executable that crashed, e.g. build/demo"
            value={program()}
            onInput={(e) => setProgram(e.currentTarget.value)}
          />
        </div>
        <p class="muted dbg-hint">The program does not run: frames, variables, memory and registers show the state at the crash.</p>
      </Show>
      <Show when={error()}>
        <div class="dbg-error dbg-pad">{error()}</div>
      </Show>
    </DialogFrame>
  );
}

export function AttachDialogs() {
  return (
    <Show when={attachDialog()} keyed>
      {(kind) => (kind === "process" ? <ProcessDialog /> : kind === "server" ? <ServerDialog /> : <CoreDumpDialog />)}
    </Show>
  );
}
