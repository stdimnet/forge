/** Signature status icons of the Git log (`%G?`): verified, unverified, bad. */
import { Show } from "solid-js";
import { ShieldAlert, ShieldCheck, ShieldQuestion, ShieldX } from "lucide-solid";
import type { SignatureStatus } from "./api";

export function signatureLabel(s: SignatureStatus): string {
  switch (s) {
    case "good":
      return "Verified signature";
    case "untrusted":
      return "Signature of an untrusted key";
    case "expired":
      return "Expired signature";
    case "expiredKey":
      return "Signed with an expired key";
    case "revoked":
      return "Signed with a revoked key";
    case "bad":
      return "Bad signature";
    case "unverified":
      return "Signature cannot be verified";
    default:
      return "Not signed";
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
