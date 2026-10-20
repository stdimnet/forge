/**
 * Status bar progress of Forge-managed installs, like PHPStorm's background tasks:
 * "Installing Intelephense: Downloading Node.js v22.11.0 · 45%" with a small bar and a stop
 * button.
 */
import { createMemo, Show } from "solid-js";
import { LoaderCircle, X } from "lucide-solid";
import { progressFraction, progressText } from "./autoInstall";
import { activeInstalls, cancelManaged } from "./managed";

export function InstallStatus() {
  const current = createMemo(() => activeInstalls()[0]);
  return (
    <Show when={current()}>
      {(a) => {
        const fraction = () => progressFraction(a().done, a().total);
        const more = () => activeInstalls().length - 1;
        return (
          <span class="status-item ls-install-status" data-testid="install-status" title={progressText(a().label, a().done, a().total)}>
            <LoaderCircle size={13} class="lsp-spin" />
            <span class="ls-install-status-name">{a().name}:</span>
            <span class="ls-install-status-text">{progressText(a().label, a().done, a().total)}</span>
            <span class="ls-install-bar" classList={{ indeterminate: fraction() === null }}>
              <span style={{ width: `${Math.round((fraction() ?? 0.35) * 100)}%` }} />
            </span>
            <Show when={more() > 0}>
              <span class="muted">+{more()}</span>
            </Show>
            <button class="icon-btn ls-install-stop" title="Cancel" onClick={() => cancelManaged(a().id)}>
              <X size={12} />
            </button>
          </span>
        );
      }}
    </Show>
  );
}
