/**
 * Themed UI and symbol icons: the active icon theme's icon for a stable id
 * (`forge_icons::ids`), else Forge's own lucide icon (the children).
 */
import { For, Show, type JSX } from "solid-js";
import { iconStyle, themedSymbol, themedUiIcon } from "./state";

/**
 * The icon of a UI id (`toolwindow.project`, `action.run`, `runconfig.<type>`,
 * `vcs.modified`, …): the theme's if it has one, else `children`. Decorative: the button or
 * row around it carries the name.
 */
export function UiIcon(props: { id: string; size: number; class?: string; children?: JSX.Element }) {
  return (
    <Show when={themedUiIcon(props.id)} fallback={props.children}>
      {(key) => <span class={`themed-ui-icon ${props.class ?? ""}`} aria-hidden="true" data-ui-icon={props.id} style={iconStyle(key(), "ui", props.size)} />}
    </Show>
  );
}

/** A themed code symbol icon with its marker overlays, or `fallback` (Forge's badge). */
export function ThemedSymbolIcon(props: {
  kind: string;
  modifiers?: { isAbstract?: boolean; isFinal?: boolean; isStatic?: boolean };
  size?: number;
  fallback: JSX.Element;
}) {
  const sym = () => themedSymbol(props.kind, props.modifiers);
  return (
    <Show when={sym()} fallback={props.fallback}>
      {(s) => (
        <span class="themed-symbol-icon" aria-hidden="true" data-symbol-kind={props.kind}>
          <span class="themed-symbol-main" style={iconStyle(s().icon, "symbol", props.size ?? 14)} />
          <For each={s().markers}>{(m) => <span class="themed-symbol-marker" style={iconStyle(m, "symbol", props.size ?? 14)} />}</For>
        </span>
      )}
    </Show>
  );
}
