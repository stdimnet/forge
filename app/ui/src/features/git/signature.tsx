/** Signature status icons of the Git log (`%G?`): verified, unverified, bad. */
import { Show } from "solid-js";
import { ShieldAlert, ShieldCheck, ShieldQuestion, ShieldX } from "lucide-solid";
import type { SignatureStatus } from "./api";
import { t } from "@/i18n";

export function signatureLabel(s: SignatureStatus): string {
  switch (s) {
    case "good":
      return t("git.signature.good");
    case "untrusted":
      return t("git.signature.untrusted");
    case "expired":
      return t("git.signature.expired");
    case "expiredKey":
      return t("git.signature.expiredKey");
    case "revoked":
      return t("git.signature.revoked");
    case "bad":
      return t("git.signature.bad");
    case "unverified":
      return t("git.signature.unverified");
    default:
      return t("git.signature.none");
  }
}

/** A small shield in the log row; nothing for unsigned commits. */
export function SignatureIcon(props: { status?: SignatureStatus }) {
  const kind = () => {
    const s = props.status;
    if (s === "good") return "good";
    if (s === "bad" || s === "revoked") return "bad";
    return "unverified";
  };
  return (
    <Show when={props.status && props.status !== "none"}>
      <span class={`log-signature ${kind()}`} title={signatureLabel(props.status!)} data-signature={props.status}>
        <Show when={kind() === "good"} fallback={<Show when={kind() === "bad"} fallback={props.status === "unverified" ? <ShieldQuestion size={13} /> : <ShieldAlert size={13} />}><ShieldX size={13} /></Show>}>
          <ShieldCheck size={13} />
        </Show>
      </span>
    </Show>
  );
}
