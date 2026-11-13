/**
 * Forward Ports (PHPStorm's Kubernetes "Forward Ports…"): local:pod port pairs for a pod,
 * with the ports its containers declare as suggestions. The backend parses the pairs and
 * starts one listener per pair on 127.0.0.1.
 */
import { createSignal, For, onCleanup, Show } from "solid-js";
import { Layer } from "@/shell/layers";
import { Plus } from "lucide-solid";
import { suspendKeymap } from "@/keymap";
import type { ContainerPort, ServiceDetails } from "@/ipc/services";
import { forwardDialog, forwardPorts, setForwardDialog } from "./store";
import { t } from "@/i18n";

/** Privileged pod ports get a local port above 1024 (80 → 8080), like most guides use. */
const localFor = (p: number) => (p < 1024 ? p + 8000 : p);
const pairOf = (p: ContainerPort) => `${localFor(p.port)}:${p.port}`;

function Dialog(props: { d: ServiceDetails }) {
  const ports = () => (props.d.ports ?? []).filter((p) => p.protocol.toUpperCase() === "TCP");
  const first = () => {
    const def = props.d.containers?.find((c) => c.default)?.name;
    return ports().find((p) => p.container === def) ?? ports()[0];
  };
  const [spec, setSpec] = createSignal(first() ? pairOf(first()!) : "");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const close = () => setForwardDialog(null);
  const add = (p: ContainerPort) => {
    const pair = pairOf(p);
    const parts = spec()
      .split(/[,\s;]+/)
      .filter(Boolean);
    if (!parts.includes(pair)) setSpec([...parts, pair].join(", "));
  };
  const ok = async () => {
    if (!spec().trim() || busy()) return;
    setBusy(true);
    try {
      await forwardPorts(props.d.nodeId, spec());
      close();
    } catch (err) {
      setError(String(err).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dialog svc-forward-dialog"
        style={{ width: "min(480px, calc(100vw - 32px))" }}
        role="dialog"
        data-testid="forward-ports-dialog"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          else if (e.key === "Enter") void ok();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{t("tools.services.forwardPorts")}</div>
        <div class="dialog-body">
          <div class="svc-forward-pod">
            {t("tools.services.pod")} <b>{props.d.title}</b>
            <span class="muted"> {t("tools.services.inNamespace", { namespace: props.d.target.namespace })}</span>
          </div>
          <label class="form-label">{t("tools.services.localPortPodPort")}</label>
          <input
            ref={(el) => queueMicrotask(() => el.focus())}
            value={spec()}
            placeholder="8080:80, 9000"
            spellcheck={false}
            data-testid="forward-spec"
            onInput={(e) => {
              setSpec(e.currentTarget.value);
              setError(null);
            }}
          />
          <div class="svc-forward-hint muted">{t("tools.services.separateSeveralPairsWithCommas")}</div>
          <Show when={ports().length}>
            <div class="svc-forward-suggest">
              <span class="muted">{t("tools.services.containerPortsLabel")}</span>
              <For each={ports()}>
                {(p) => (
                  <button class="svc-chip" title={`Add ${pairOf(p)}`} onClick={() => add(p)}>
                    <Plus size={11} />
                    {p.port}
                    <Show when={p.name}>
                      <span class="muted"> {p.name}</span>
                    </Show>
                    <span class="muted"> ({p.container})</span>
                  </button>
                )}
              </For>
            </div>
          </Show>
          <Show when={error()}>
            <div class="dialog-error">{error()}</div>
          </Show>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={close}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!spec().trim() || busy()} onClick={() => void ok()}>
            {t("tools.services.forward")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function ForwardPortsDialog() {
  return (
    <Show when={forwardDialog()} keyed>
      {(d) => {
        const release = suspendKeymap();
        onCleanup(release);
        return (
          <Layer kind="modal">
            <Dialog d={d} />
          </Layer>
        );
      }}
    </Show>
  );
}
