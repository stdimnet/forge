import { Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { CodeXml, FoldVertical, Hash, KeyRound, Lock, LockOpen } from "lucide-solid";
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
  module: "M",
  alias: "T",
  variable: "v",
  tag: "",
  rule: "",
  region: "",
};

/** Kinds drawn as an outline icon instead of a letter badge. */
const glyphs: Partial<Record<SymbolKind, typeof Hash>> = { tag: CodeXml, rule: Hash, region: FoldVertical };

/** PHPStorm-style symbol badge: a letter in a circle, plus the visibility marker. */
export function SymbolIcon(props: { kind: SymbolKind; visibility?: Visibility; modifiers?: Modifiers }) {
  const glyph = () => glyphs[props.kind];
  return (
    <Show
      when={!glyph()}
      fallback={
        <span class={`php-symbol-icon php-symbol-glyph php-glyph-${props.kind}`}>
          <Dynamic component={glyph()} size={14} stroke-width={2} />
        </span>
      }
    >
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
    </Show>
  );
}
