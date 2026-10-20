/**
 * Asked once before Forge installs Intelephense: what it is, that Forge downloads it onto
 * this computer instead of shipping it, and that installing means agreeing to its licence.
 */
import { onMount } from "solid-js";
import { Portal } from "solid-js/web";
import { ExternalLink, ShieldCheck, X } from "lucide-solid";

export const INTELEPHENSE_HOMEPAGE = "https://intelephense.com";
export const INTELEPHENSE_PACKAGE = "https://www.npmjs.com/package/intelephense";

export function IntelephenseConsent(props: { onAnswer: (accepted: boolean) => void }) {
  let accept!: HTMLButtonElement;
  onMount(() => accept.focus());
  return (
    <Portal>
      <div class="ls-overlay" onMouseDown={(e) => e.target === e.currentTarget && props.onAnswer(false)}>
        <div
          class="ls-dialog ls-consent-dialog"
          role="dialog"
          aria-label="Install Intelephense"
          data-testid="intelephense-consent"
          onKeyDown={(e) => {
            if (e.key === "Escape") props.onAnswer(false);
          }}
        >
          <header class="ls-dialog-title">
            <span>Install Intelephense</span>
            <button class="icon-btn" title="Close" onClick={() => props.onAnswer(false)}>
              <X size={15} />
            </button>
          </header>
          <div class="ls-dialog-body">
            <p class="ls-dialog-text">
              Intelephense is the PHP language server behind completion, navigation and diagnostics. It is proprietary
              software by Ben Mewburn, free to use; premium features need a licence key of your own.
            </p>
            <ul class="ls-consent-points">
              <li>
                Its licence does not allow passing it on, so Forge does not ship it. Forge downloads it for you from the
                official npm package onto this computer.
              </li>
              <li>The download is checked against the registry's sha512 hash and kept in Forge's server directory.</li>
              <li>If no Node.js 18 or newer is installed, Forge also downloads the official Node.js LTS build (SHA-256 checked).</li>
            </ul>
            <div class="ls-consent-note">
              <ShieldCheck size={15} />
              <span>
                By installing Intelephense you agree to the Intelephense licence (LICENSE.txt in the package).
              </span>
            </div>
            <div class="ls-consent-links">
              <a class="ls-link" href={INTELEPHENSE_HOMEPAGE} target="_blank" rel="noreferrer noopener">
                Licence and terms at intelephense.com <ExternalLink size={12} />
              </a>
              <a class="ls-link" href={INTELEPHENSE_PACKAGE} target="_blank" rel="noreferrer noopener">
                npm package <ExternalLink size={12} />
              </a>
            </div>
          </div>
          <footer class="ls-dialog-footer">
            <span class="ls-spacer" />
            <button class="btn-secondary" onClick={() => props.onAnswer(false)}>
              Cancel
            </button>
            <button class="btn-primary" ref={accept} onClick={() => props.onAnswer(true)}>
              Accept and Install
            </button>
          </footer>
        </div>
      </div>
    </Portal>
  );
}
