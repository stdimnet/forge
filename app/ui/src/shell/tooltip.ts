/**
 * Native tooltips (`title`) that behave like PHPStorm's: a press hides the tooltip until the
 * pointer leaves the control, and the tooltip never shows while the control's popup is open.
 *
 * WebKitGTK keeps a native tooltip on screen after a click, so a tooltip would otherwise stay
 * on top of the popup the click opened. The tooltip is hidden by dropping the `title`
 * attribute; `aria-label` keeps the control's accessible name meanwhile.
 *
 * Spread the result onto the control: `<button {...tooltip(() => text, { open })}>`.
 */
import { createSignal } from "solid-js";

export interface TooltipOptions {
  /** Whether the control's popup is open: no tooltip meanwhile. */
  open?: () => boolean;
  /**
   * Whether the text names the control (`aria-label`). Use `false` for controls that show
   * their name as text, where the tooltip only adds detail.
   */
  label?: boolean;
}

export interface TooltipProps {
  readonly title: string | undefined;
  readonly "aria-label": string | undefined;
  onPointerDown: () => void;
  onPointerLeave: () => void;
}

export function tooltip(text: () => string | undefined, options: TooltipOptions = {}): TooltipProps {
  const [pressed, setPressed] = createSignal(false);
  return {
    get title() {
      return pressed() || options.open?.() ? undefined : text();
    },
    get "aria-label"() {
      return options.label === false ? undefined : text();
    },
    onPointerDown: () => setPressed(true),
    onPointerLeave: () => setPressed(false),
  };
}
