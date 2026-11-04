/**
 * Native debugger views of DAP sessions, shown as tabs of the Debug tool window when the
 * adapter supports them: Memory (hex/ASCII, go to address, edit bytes), Disassembly (with
 * instruction stepping) and Registers. Also the thread selector above the frames.
 */
import { createEffect, createSignal, For, on, Show } from "solid-js";
import { ArrowDownToLine, ChevronLeft, ChevronRight, RedoDot, RefreshCw } from "lucide-solid";
import { runAction, shortcutLabel } from "@/keymap";
import { formatForKeymap } from "@/keymap/store";
import type { Property } from "./api";
import { dapApi, type Instruction, type MemoryBlock } from "./dapApi";
import {
  activeSession,
  frameIndex,
  invalidations,
  pausedSession,
  selectedFrame,
  selectThread,
  sessionFeatures,
  threadId,
  threads,
  variables,
} from "./store";
import { VariableNode } from "./VariablesTree";
import { t as tr } from "@/i18n";

/** The built-in tabs, or the id of a registered one (`registerDebugTab`). */
export type DebugTab = "frames" | "console" | "memory" | "disassembly" | "registers" | (string & {});

const [debugTab, setDebugTab] = createSignal<DebugTab>("frames");
/** Address (memory reference) the Memory view shows. */
const [memoryAddress, setMemoryAddress] = createSignal("");
const [memoryOffset, setMemoryOffset] = createSignal(0);

export { debugTab, setDebugTab, memoryAddress };

const PAGE = 256;

/** View Memory: show `reference` in the Memory tab. */
export function openMemoryView(reference: string): void {
  setMemoryAddress(reference);
  setMemoryOffset(0);
  setDebugTab("memory");
}

const tip = (title: string, action: string) => {
  const keys = shortcutLabel(action);
  return keys ? `${title} (${formatForKeymap(keys)})` : title;
};

const errorText = (e: unknown) => String(e).replace(/^Error: /, "");

/** Bumps when the paused location, the frame or an invalidation of `area` changes. */
function reloadKey(area: string) {
  return () => [pausedSession()?.id, pausedSession()?.location?.line, pausedSession()?.threadId, frameIndex(), invalidations()[area] ?? 0] as const;
}

// ---- threads ------------------------------------------------------------------------------

export function ThreadSelector() {
  return (
    <Show when={activeSession()?.adapter && threads().length > 0}>
      <div class="dbg-threads">
        <select
          class="dbg-thread-select"
          aria-label={tr("debug.nativeViews.thread")}
          value={String(threadId() ?? "")}
          disabled={!pausedSession()}
          onChange={(e) => void selectThread(Number(e.currentTarget.value))}
        >
          <For each={threads()}>
            {(t) => (
              <option value={t.id}>
                {`"${t.name}"@${t.id}${t.id === activeSession()?.threadId ? " (stopped)" : ""}`}
              </option>
            )}
          </For>
        </select>
      </div>
    </Show>
  );
}

// ---- memory -------------------------------------------------------------------------------

/** First variable with a memory location, the default address of the Memory view. */
function defaultAddress(list: Property[]): string {
  return list.find((v) => v.memoryReference)?.memoryReference ?? "";
}

export function MemoryPane() {
  const [block, setBlock] = createSignal<MemoryBlock | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [input, setInput] = createSignal(memoryAddress());
  const [editing, setEditing] = createSignal<{ row: number; col: number } | null>(null);
  const canWrite = () => !!sessionFeatures()?.writeMemory && !!pausedSession();

  const load = async () => {
    const s = pausedSession();
    let address = memoryAddress();
    if (!address) {
      address = defaultAddress(variables());
      if (address) setMemoryAddress(address);
    }
    if (!s || !address) {
      setBlock(null);
      return;
    }
    setInput(address);
    try {
      setBlock(await dapApi.readMemory(s.id, address, PAGE, memoryOffset()));
      setError(null);
    } catch (e) {
      setBlock(null);
      setError(errorText(e));
    }
  };
  createEffect(on([reloadKey("memory"), memoryAddress, memoryOffset, () => variables().length], () => void load()));

  const go = () => {
    const a = input().trim();
    if (!a) return;
    setMemoryOffset(0);
    if (a === memoryAddress()) void load();
    else setMemoryAddress(a);
  };
  const write = async (row: number, col: number, text: string) => {
    setEditing(null);
    const s = pausedSession();
    const perRow = block()?.rows[0]?.hex.length ?? 16;
    if (!s || !/^[0-9a-f]{1,2}$/i.test(text.trim())) return;
    try {
      await dapApi.writeMemory(s.id, memoryAddress(), text.trim(), memoryOffset() + row * perRow + col);
      await load();
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <div class="dbg-native">
      <div class="dbg-native-tools">
        <label class="muted" for="dbg-mem-addr">
          {tr("debug.nativeViews.address")}
        </label>
        <input
          id="dbg-mem-addr"
          class="dbg-input dbg-mono dbg-mem-addr"
          value={input()}
          placeholder={tr("debug.nativeViews.n0x7ffe0000OrVariable")}
          spellcheck={false}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              go();
            }
          }}
        />
        <button class="icon-btn" title={tr("debug.nativeViews.previousPage")} disabled={!block()} onClick={() => setMemoryOffset((o) => o - PAGE)}>
          <ChevronLeft size={14} />
        </button>
        <button class="icon-btn" title={tr("debug.nativeViews.nextPage")} disabled={!block()} onClick={() => setMemoryOffset((o) => o + PAGE)}>
          <ChevronRight size={14} />
        </button>
        <button class="icon-btn" title={tr("debug.nativeViews.refresh")} disabled={!pausedSession()} onClick={() => void load()}>
          <RefreshCw size={13} />
        </button>
        <Show when={block()?.unreadable}>
          <span class="muted dbg-native-note">{tr("tools.debug.bytesNotReadable", { unreadable: block()!.unreadable })}</span>
        </Show>
      </div>
      <div class="dbg-memory dbg-mono" role="grid" aria-label={tr("debug.nativeViews.memory")}>
        <Show when={error()}>
          <div class="dbg-error dbg-pad">{error()}</div>
        </Show>
        <Show
          when={block()}
          fallback={
            <Show when={!error()}>
              <div class="dbg-empty muted">
                {pausedSession() ? tr("debug.nativeViews.enterAnAddressOrChoose") : tr("debug.nativeViews.memoryIsAvailableWhileThe")}
              </div>
            </Show>
          }
        >
          {(b) => (
            <For each={b().rows}>
              {(row, r) => (
                <div class="dbg-mem-row" role="row">
                  <span class="dbg-mem-address">{row.address}</span>
                  <span class="dbg-mem-hex">
                    <For each={row.hex}>
                      {(byte, c) => (
                        <Show
                          when={editing()?.row === r() && editing()?.col === c()}
                          fallback={
                            <span
                              class="dbg-mem-byte"
                              classList={{ unreadable: byte === "??", zero: byte === "00", gap: c() > 0 && c() % 8 === 0 }}
                              title={canWrite() ? tr("debug.nativeViews.doubleClickToChange") : undefined}
                              onDblClick={() => canWrite() && byte !== "??" && setEditing({ row: r(), col: c() })}
                            >
                              {byte}
                            </span>
                          }
                        >
                          <input
                            class="dbg-mem-edit"
                            value={byte}
                            maxLength={2}
                            ref={(el) => queueMicrotask(() => (el.focus(), el.select()))}
                            onBlur={() => setEditing(null)}
                            onKeyDown={(e) => {
                              if (e.key === "Enter") void write(r(), c(), e.currentTarget.value);
                              if (e.key === "Escape") setEditing(null);
                            }}
                          />
                        </Show>
                      )}
                    </For>
                  </span>
                  <span class="dbg-mem-ascii">{row.ascii}</span>
                </div>
              )}
            </For>
          )}
        </Show>
      </div>
    </div>
  );
}

// ---- disassembly --------------------------------------------------------------------------

export function DisassemblyPane() {
  const [code, setCode] = createSignal<Instruction[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  let list: HTMLDivElement | undefined;
  const load = async () => {
    const s = pausedSession();
    const ip = selectedFrame()?.address;
    if (!s || !ip) {
      setCode([]);
      setError(s ? tr("debug.nativeViews.theSelectedFrameHasNo") : null);
      return;
    }
    try {
      setCode(await dapApi.disassemble(s.id, ip, -12, 48, ip));
      setError(null);
      queueMicrotask(() => list?.querySelector(".dbg-asm-row.current")?.scrollIntoView({ block: "center" }));
    } catch (e) {
      setCode([]);
      setError(errorText(e));
    }
  };
  createEffect(on([reloadKey("memory"), () => selectedFrame()?.address], () => void load()));
  const stepping = () => !!pausedSession() && !!sessionFeatures()?.instructionStepping;
  const sourceOf = (i: Instruction, n: number) => {
    const prev = code()[n - 1];
    return i.line && (!prev || prev.line !== i.line || prev.file !== i.file) ? `${(i.file ?? "").split("/").pop()}:${i.line}` : "";
  };
  return (
    <div class="dbg-native">
      <div class="dbg-native-tools">
        <button class="icon-btn" title={tip("Step Over Instruction", "Forge.Debugger.StepOverInstruction")} disabled={!stepping()} onClick={() => void runAction("Forge.Debugger.StepOverInstruction")}>
          <RedoDot size={14} />
        </button>
        <button class="icon-btn" title={tip("Step Into Instruction", "Forge.Debugger.StepIntoInstruction")} disabled={!stepping()} onClick={() => void runAction("Forge.Debugger.StepIntoInstruction")}>
          <ArrowDownToLine size={14} />
        </button>
        <span class="muted dbg-native-note">{selectedFrame() ? `${selectedFrame()!.where} @ ${selectedFrame()!.address ?? "?"}` : ""}</span>
      </div>
      <div class="dbg-asm dbg-mono" ref={list} role="list" aria-label={tr("debug.nativeViews.disassembly")}>
        <Show when={error()}>
          <div class="dbg-error dbg-pad">{error()}</div>
        </Show>
        <Show when={!pausedSession()}>
          <div class="dbg-empty muted">{tr("debug.nativeViews.disassemblyIsAvailableWhileThe")}</div>
        </Show>
        <For each={code()}>
          {(i, n) => (
            <div class="dbg-asm-row" classList={{ current: i.current }} data-address={i.address}>
              <span class="dbg-asm-marker">{i.current ? "▶" : ""}</span>
              <span class="dbg-asm-address">{i.address}</span>
              <span class="dbg-asm-bytes">{i.bytes ?? ""}</span>
              <span class="dbg-asm-text">{i.instruction}</span>
              <span class="dbg-asm-symbol">{i.symbol ? `<${i.symbol}>` : ""}</span>
              <span class="dbg-asm-source">{sourceOf(i, n())}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

// ---- registers ----------------------------------------------------------------------------

export function RegistersPane() {
  const [regs, setRegs] = createSignal<Property[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const load = async () => {
    const s = pausedSession();
    if (!s) {
      setRegs([]);
      return;
    }
    try {
      setRegs(await dapApi.registers(s.id, selectedFrame()?.level ?? 0));
      setError(null);
    } catch (e) {
      setRegs([]);
      setError(errorText(e));
    }
  };
  createEffect(on(reloadKey("variables"), () => void load()));
  return (
    <div class="dbg-native">
      <div class="dbg-tree dbg-registers" role="tree" aria-label={tr("debug.nativeViews.registers")}>
        <Show when={error()}>
          <div class="dbg-error dbg-pad">{error()}</div>
        </Show>
        <For
          each={regs()}
          fallback={
            <Show when={!error()}>
              <div class="dbg-empty muted">{pausedSession() ? tr("debug.nativeViews.theDebugAdapterReportsNo") : tr("debug.nativeViews.registersAreAvailableWhileThe")}</div>
            </Show>
          }
        >
          {(r) => <VariableNode p={r} depth={0} />}
        </For>
      </div>
    </div>
  );
}
