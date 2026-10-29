/**
 * The Coroutines tab of the Debug tool window (like IntelliJ IDEA's Coroutines view): while
 * a Kotlin program is suspended, the debugger evaluates the dump of kotlinx-coroutines'
 * debug probes and Rust parses it (`kotlin_coroutines`) into coroutines with their state,
 * dispatcher, stack and creation stack. Without the probes it explains how to enable them.
 */
import { createEffect, createSignal, For, on, Show } from "solid-js";
import { ChevronDown, ChevronRight, RefreshCw, Waves } from "lucide-solid";
import { debugApi } from "@/features/debug/api";
import { activeSession, frameIndex, pausedSession, selectedFrame } from "@/features/debug/store";
import { openFileAt } from "@/editor/views";
import { COROUTINE_DUMP_EXPRESSION, COROUTINE_INFO_EXPRESSION, kotlinApi, type Coroutine, type CoroutineDump, type CoroutineFrame } from "@/ipc/kotlin";

const errorText = (e: unknown) => String(e).replace(/^Error: /, "");

/** Evaluate the dump in the selected frame: the Kotlin expression, else the Java one. */
async function dump(sessionId: string, level: number): Promise<CoroutineDump> {
  let failure = "";
  for (const expression of [COROUTINE_DUMP_EXPRESSION, COROUTINE_INFO_EXPRESSION]) {
    try {
      const value = await debugApi.eval(sessionId, expression, level, "repl");
      const parsed = await kotlinApi.coroutines(value.value ?? "");
      if (parsed.kind === "coroutines") return parsed;
      failure = parsed.message;
    } catch (e) {
      failure = errorText(e);
    }
  }
  return kotlinApi.coroutines(failure);
}

/** Open the source of a frame: `com.acme.orders.MainKt$main$1.invokeSuspend` in `Main.kt`. */
async function openFrame(f: CoroutineFrame): Promise<void> {
  if (!f.file) return;
  const pkg = f.method.split("$")[0].split(".").slice(0, -2).join("/");
  const rel = pkg ? `${pkg}/${f.file}` : f.file;
  for (const root of ["src/main/kotlin", "src/test/kotlin", "app/src/main/kotlin", "src/main/java"]) {
    try {
      if (await openFileAt(`${root}/${rel}`, f.line ?? null)) return;
    } catch {
      // Try the next source root.
    }
  }
}

function FrameRow(props: { frame: CoroutineFrame }) {
  const method = () => props.frame.method.split(".").slice(-2).join(".");
  return (
    <div class="kt-frame-row" title={props.frame.method} onClick={() => void openFrame(props.frame)}>
      <span>{method()}</span>
      <Show when={props.frame.file}>
        <span class="loc">
          {props.frame.file}
          {props.frame.line ? `:${props.frame.line}` : ""}
        </span>
      </Show>
    </div>
  );
}

function CoroutineRow(props: { c: Coroutine; open: boolean; toggle: () => void }) {
  const state = () => props.c.state.toLowerCase();
  const hasFrames = () => props.c.frames.length > 0 || props.c.creation.length > 0;
  return (
    <>
      <div class="kt-coroutine-row" data-coroutine={props.c.name} onClick={props.toggle}>
        <span class="chev">
          <Show when={hasFrames()}>{props.open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
        </span>
        <Waves size={13} />
        <span class="kt-coroutine-name">
          "{props.c.name}
          {props.c.id !== undefined ? `#${props.c.id}` : ""}"
        </span>
        <span class={`kt-coroutine-state kt-state-${state()}`}>{props.c.state || "UNKNOWN"}</span>
        <span class="kt-coroutine-meta">
          {[props.c.dispatcher, props.c.job].filter(Boolean).join(" · ")}
        </span>
      </div>
      <Show when={props.open}>
        <For each={props.c.frames}>{(f) => <FrameRow frame={f} />}</For>
        <Show when={props.c.creation.length}>
          <div class="kt-frame-caption">Coroutine creation stack trace</div>
          <For each={props.c.creation}>{(f) => <FrameRow frame={f} />}</For>
        </Show>
      </Show>
    </>
  );
}

export function CoroutinesView() {
  const [result, setResult] = createSignal<CoroutineDump | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [open, setOpen] = createSignal<Record<string, boolean>>({});
  const load = async () => {
    const s = pausedSession();
    if (!s) return;
    setLoading(true);
    try {
      setResult(await dump(s.id, selectedFrame()?.level ?? 0));
    } finally {
      setLoading(false);
    }
  };
  createEffect(on(() => [pausedSession()?.id, pausedSession()?.location?.line, frameIndex()] as const, () => void load()));
  const coroutines = () => {
    const r = result();
    return r?.kind === "coroutines" ? r.coroutines : [];
  };
  const keyOf = (c: Coroutine, i: number) => `${c.name}#${c.id ?? i}`;
  return (
    <div class="kt-coroutines" data-coroutines-view>
      <div class="kt-coroutines-bar">
        <button class="icon-btn" title="Refresh" disabled={!pausedSession() || loading()} onClick={() => void load()}>
          <RefreshCw size={14} />
        </button>
        <span class="grow">
          <Show when={result()?.kind === "coroutines"}>
            {coroutines().length} coroutines · {coroutines().filter((c) => c.state === "SUSPENDED").length} suspended
          </Show>
        </span>
        <span>{activeSession()?.serverName}</span>
      </div>
      <Show
        when={pausedSession()}
        fallback={<div class="kt-coroutines-empty">Coroutines are dumped while the program is suspended: pause it or stop at a breakpoint.</div>}
      >
        <Show when={result()?.kind === "unavailable"}>
          <div class="kt-coroutines-empty">
            <p>{(result() as { message: string }).message}</p>
            <p>
              Gradle: <code>testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-debug")</code>, then <code>DebugProbes.install()</code>.
            </p>
          </div>
        </Show>
        <div class="kt-coroutines-list" role="tree" aria-label="Coroutines">
          <For each={coroutines()}>
            {(c, i) => (
              <CoroutineRow
                c={c}
                open={open()[keyOf(c, i())] ?? i() === 0}
                toggle={() => setOpen((o) => ({ ...o, [keyOf(c, i())]: !(o[keyOf(c, i())] ?? i() === 0) }))}
              />
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
