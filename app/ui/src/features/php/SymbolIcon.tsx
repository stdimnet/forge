import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { CodeXml, FoldVertical, Hash, KeyRound, Lock, LockOpen } from "lucide-solid";
import { iconStyle, themedSymbol } from "@/features/iconThemes/state";
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

/**
 * PHPStorm-style symbol icon: the active icon theme's icon for the kind (its `symbols`
 * section, with marker overlays for abstract, final and static), else Forge's letter badge;
 * plus Forge's visibility marker either way. Used by Structure, the navigation bar,
 * breadcrumbs, the File Structure popup, Hierarchy and Search Everywhere.
 */
export function SymbolIcon(props: { kind: SymbolKind; visibility?: Visibility; modifiers?: Modifiers }) {
  const glyph = () => glyphs[props.kind];
  const themed = () => themedSymbol(props.kind, props.modifiers);
  const Visibility = () => (
    <Show when={props.visibility}>
      {(v) => (
        <span class={`php-visibility php-visibility-${v()}`} title={v()}>
          {v() === "private" ? <Lock size={10} stroke-width={2.4} /> : v() === "protected" ? <KeyRound size={10} stroke-width={2.4} /> : <LockOpen size={10} stroke-width={2.4} />}
        </span>
      )}
    </Show>
  );
  return (
    <Show
      when={!themed()}
      fallback={
        <span class="php-symbol-icon themed-symbol" classList={{ "with-visibility": !!props.visibility }} data-symbol-kind={props.kind}>
          <span class="themed-symbol-icon" aria-hidden="true">
            <span style={iconStyle(themed()!.icon, "symbol", 14)} />
            <For each={themed()!.markers}>{(m) => <span class="themed-symbol-marker" style={iconStyle(m, "symbol", 14)} />}</For>
          </span>
          <Visibility />
        </span>
      }
    >
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
          <Visibility />
        </span>
      </Show>
    </Show>
  );
}
