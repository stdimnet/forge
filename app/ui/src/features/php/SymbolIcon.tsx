import { Show } from "solid-js";
import { KeyRound, Lock, LockOpen } from "lucide-solid";
import type { Modifiers, SymbolKind, Visibility } from "./api";

const letters: Record<SymbolKind, string> = {
  namespace: "n",
  class: "c",
  interface: "I",
  trait: "T",
  enum: "E",
  enumCase: "e",
  method: "m",
  property: "f",
  constant: "c",
  function: "λ",
};

/** PHPStorm-style symbol badge: a letter in a circle, plus the visibility marker. */
export function SymbolIcon(props: { kind: SymbolKind; visibility?: Visibility; modifiers?: Modifiers }) {
  return (
    <span class="php-symbol-icon" classList={{ "with-visibility": !!props.visibility }}>
      <span
        class={`php-badge php-badge-${props.kind}`}
        classList={{ abstract: !!props.modifiers?.isAbstract, static: !!props.modifiers?.isStatic }}
      >
        {letters[props.kind]}
        <Show when={props.modifiers?.isStatic}>
          <span class="php-badge-static" />
        </Show>
      </span>
      <Show when={props.visibility}>
        {(v) => (
          <span class={`php-visibility php-visibility-${v()}`} title={v()}>
            {v() === "private" ? <Lock size={10} stroke-width={2.4} /> : v() === "protected" ? <KeyRound size={10} stroke-width={2.4} /> : <LockOpen size={10} stroke-width={2.4} />}
          </span>
        )}
      </Show>
    </span>
  );
}
