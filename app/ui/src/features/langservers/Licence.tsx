/**
 * Intelephense licence: "Intelephense: Free" or "Premium (key …1234)" plus a dialog to
 * enter or remove the key. The key goes to the OS keychain through the backend and is
 * never shown again; Intelephense restarts when it changes. Licences are bought at
 * intelephense.com (a link, no purchase flow inside Forge).
 */
import { createSignal, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { BadgeCheck, ExternalLink, Eye, EyeOff, KeyRound, X } from "lucide-solid";
import { licence, refreshLicence, saveLicence } from "./store";

const errorText = (e: unknown) => String((e as Error)?.message ?? e);

export function LicenceDialog(props: { onClose: () => void }) {
  const [key, setKey] = createSignal("");
  const [reveal, setReveal] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let input!: HTMLInputElement;
  onMount(() => input.focus());

  const save = async (value: string | null) => {
    setBusy(true);
    setError(null);
    try {
      await saveLicence(value);
      props.onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Portal>
      <div class="ls-overlay" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
        <div
          class="ls-dialog ls-licence-dialog"
          role="dialog"
          aria-label="Intelephense Licence"
          onKeyDown={(e) => {
            if (e.key === "Escape") props.onClose();
          }}
        >
          <header class="ls-dialog-title">
            <span>Intelephense Licence</span>
            <button class="icon-btn" title="Close" onClick={props.onClose}>
              <X size={15} />
            </button>
          </header>
          <div class="ls-dialog-body">
            <p class="ls-dialog-text">
              Intelephense works without a licence. A licence key unlocks its premium features: rename, code actions,
              find implementations, type and call hierarchy, folding and more.
            </p>
            <label class="ls-field-label" for="ls-licence-key">
              Licence key:
            </label>
            <div class="ls-secret">
              <input
                id="ls-licence-key"
                ref={input}
                class="ls-input"
                type={reveal() ? "text" : "password"}
                autocomplete="off"
                spellcheck={false}
                placeholder={licence()?.premium ? `Replace the key ending in ${licence()?.maskedKey?.slice(1)}` : "Paste your licence key"}
                value={key()}
                onInput={(e) => setKey(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && key().trim()) void save(key());
                }}
              />
              <button class="icon-btn" title={reveal() ? "Hide" : "Show"} onClick={() => setReveal((v) => !v)}>
                {reveal() ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
            </div>
            <Show when={error()}>
              <div class="ls-error">{error()}</div>
            </Show>
            <p class="ls-hint">
              Stored in the system keychain, never in settings files. Intelephense restarts to apply it.
            </p>
            <a class="ls-link" href={licence()?.purchaseUrl ?? "https://intelephense.com"} target="_blank" rel="noreferrer noopener">
              Buy a licence at intelephense.com <ExternalLink size={12} />
            </a>
          </div>
          <footer class="ls-dialog-footer">
            <Show when={licence()?.premium}>
              <button class="btn-secondary ls-remove" disabled={busy()} onClick={() => void save(null)}>
                Remove Licence
              </button>
            </Show>
            <span class="ls-spacer" />
            <button class="btn-secondary" onClick={props.onClose}>
              Cancel
            </button>
            <button class="btn-primary" disabled={busy() || !key().trim()} onClick={() => void save(key())}>
              Activate
            </button>
          </footer>
        </div>
      </div>
    </Portal>
  );
}

/** Status line with the Enter/Change button, for the PHP page and the Intelephense row. */
export function IntelephenseLicence(props: { compact?: boolean }) {
  const [open, setOpen] = createSignal(false);
  onMount(() => void refreshLicence());
  return (
    <div class="ls-licence" classList={{ compact: props.compact }} data-testid="intelephense-licence">
      <Show
        when={licence()?.premium}
        fallback={
          <span class="ls-licence-status">
            <span class="ls-licence-label">Intelephense:</span>
            <span class="ls-tier free">Free</span>
          </span>
        }
      >
        <span class="ls-licence-status">
          <span class="ls-licence-label">Intelephense:</span>
          <span class="ls-tier premium">
            <BadgeCheck size={13} /> Premium
          </span>
          <span class="muted">(key {licence()?.maskedKey})</span>
        </span>
      </Show>
      <button class="btn-secondary ls-small-btn" onClick={() => setOpen(true)}>
        <KeyRound size={13} />
        {licence()?.premium ? "Change Licence Key…" : "Enter Licence Key…"}
      </button>
      <Show when={!licence()?.premium}>
        <a class="ls-link" href={licence()?.purchaseUrl ?? "https://intelephense.com"} target="_blank" rel="noreferrer noopener">
          Buy a licence
        </a>
      </Show>
      <Show when={licence()?.error}>
        <span class="ls-error">Keychain: {licence()?.error}</span>
      </Show>
      <Show when={open()}>
        <LicenceDialog onClose={() => setOpen(false)} />
      </Show>
    </div>
  );
}
