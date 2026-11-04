/**
 * The Peripherals tab of the Debug tool window (CLion's "Peripherals" for embedded
 * sessions): the peripherals of the configuration's CMSIS-SVD file; expanding one reads its
 * registers through the debugger's `readMemory` and shows them with their decoded fields
 * (`cpp_svd_read`, decoding in `cforge_embedded::svd`). Values are read again whenever the
 * program stops.
 */
import { createEffect, createResource, createSignal, For, on, Show } from "solid-js";
import { ChevronDown, ChevronRight, RefreshCw, Search } from "lucide-solid";
import { cppToolsApi, type RegisterValue } from "@/ipc/cppTools";
import { activeSession, pausedSession } from "@/features/debug/store";
import { t } from "@/i18n";

const [svds, setSvds] = createSignal<Record<string, string>>({});

/** Show the Peripherals tab for a debug session with this SVD file. */
export function setSessionSvd(sessionId: string, path: string): void {
  setSvds((m) => ({ ...m, [sessionId]: path }));
}

export const svdOf = (sessionId: string): string | undefined => svds()[sessionId];

const hex = (n: number, digits = 8) => `0x${n.toString(16).toUpperCase().padStart(digits, "0")}`;

function RegisterRow(props: { reg: RegisterValue; changed: boolean; open: boolean; onToggle: () => void }) {
  const open = () => props.open;
  return (
    <>
      <div class="tree-row periph-row periph-register" data-register={props.reg.name} onClick={() => props.onToggle()} title={props.reg.description}>
        <span class="periph-toggle">{props.reg.fields.length ? open() ? <ChevronDown size={12} /> : <ChevronRight size={12} /> : null}</span>
        <span class="periph-name">{props.reg.name}</span>
        <span class="periph-address muted">{hex(props.reg.address)}</span>
        <span class="periph-value" classList={{ changed: props.changed, muted: props.reg.value == null }}>
          {props.reg.display}
        </span>
        <span class="periph-access muted">{props.reg.access}</span>
      </div>
      <Show when={open()}>
        <For each={props.reg.fields}>
          {(f) => (
            <div class="tree-row periph-row periph-field" title={f.description}>
              <span class="periph-toggle" />
              <span class="periph-name">{f.name}</span>
              <span class="periph-address muted">{f.bits}</span>
              <span class="periph-value">{f.display}</span>
              <span class="periph-access muted">{f.description ?? ""}</span>
            </div>
          )}
        </For>
      </Show>
    </>
  );
}

function PeripheralNode(props: { path: string; name: string; base: number; description?: string; sessionId: string }) {
  const [open, setOpen] = createSignal(false);
  const [values, setValues] = createSignal<RegisterValue[] | null>(null);
  const [previous, setPrevious] = createSignal<Map<string, number | null | undefined>>(new Map());
  const [error, setError] = createSignal<string | null>(null);
  // Expanded registers survive reloads (new values replace the rows).
  const [openRegs, setOpenRegs] = createSignal<Set<string>>(new Set());
  const toggleReg = (name: string) =>
    setOpenRegs((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  const load = async () => {
    try {
      const list = await cppToolsApi.svdRead(props.path, props.name, props.sessionId);
      const old = values();
      if (old) setPrevious(new Map(old.map((r) => [r.name, r.value])));
      setValues(list);
      setError(null);
    } catch (e) {
      setError(String(e).replace(/^Error: /, ""));
    }
  };
  // Read again whenever the program stops while the peripheral is open.
  createEffect(on(() => [open(), pausedSession()?.location?.line, pausedSession()?.id] as const, ([o]) => o && void load()));
  return (
    <>
      <div class="tree-row periph-row periph-peripheral" data-peripheral={props.name} onClick={() => setOpen(!open())} title={props.description}>
        <span class="periph-toggle">{open() ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</span>
        <span class="periph-name">{props.name}</span>
        <span class="periph-address muted">{hex(props.base)}</span>
        <span />
        <span class="periph-access muted">{props.description ?? ""}</span>
      </div>
      <Show when={open()}>
        <Show when={error()}>
          <div class="periph-error">{error()}</div>
        </Show>
        <For each={values() ?? []}>{(r) => (
            <RegisterRow
              reg={r}
              open={openRegs().has(r.name)}
              onToggle={() => toggleReg(r.name)}
              changed={previous().has(r.name) && previous().get(r.name) !== r.value}
            />
          )}</For>
      </Show>
    </>
  );
}

export function PeripheralsView() {
  const session = () => activeSession();
  const path = () => (session() ? svdOf(session()!.id) : undefined);
  const [filter, setFilter] = createSignal("");
  const [device, { refetch }] = createResource(path, (p) => cppToolsApi.svdPeripherals(p));
  const shown = () => {
    const f = filter().trim().toLowerCase();
    return (device()?.peripherals ?? []).filter((p) => !f || p.name.toLowerCase().includes(f) || (p.description ?? "").toLowerCase().includes(f));
  };
  return (
    <div class="periph-view">
      <div class="periph-toolbar">
        <Search size={13} class="muted" />
        <input class="periph-filter" placeholder={t("langs.cpp.filterPeripherals")} value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
        <span class="muted periph-device">{device() ? `${device()!.name}${device()!.vendor ? ` · ${device()!.vendor}` : ""}` : ""}</span>
        <button class="icon-btn" title={t("langs.cpp.reloadSvdFile")} onClick={() => void refetch()}>
          <RefreshCw size={14} />
        </button>
      </div>
      <Show when={!device.error} fallback={<div class="periph-error">{String(device.error).replace(/^Error: /, "")}</div>}>
        <div class="periph-header">
          <span class="periph-toggle" />
          <span class="periph-name">{t("common.name")}</span>
          <span class="periph-address">{t("langs.cpp.address")}</span>
          <span class="periph-value">{t("common.value")}</span>
          <span class="periph-access">{t("langs.cpp.access")}</span>
        </div>
        <div class="periph-tree">
          <For each={shown()}>
            {(p) => <PeripheralNode path={path()!} name={p.name} base={p.baseAddress} description={p.description} sessionId={session()!.id} />}
          </For>
        </div>
      </Show>
    </div>
  );
}
