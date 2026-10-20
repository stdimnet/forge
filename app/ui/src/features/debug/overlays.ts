/** Signals for the debugger's floating UI, rendered by `DebugOverlays`. */
import { createSignal } from "solid-js";
import type { Breakpoint } from "./api";

export interface BreakpointPopup {
  id: number;
  x: number;
  y: number;
}

const [breakpointPopup, setBreakpointPopup] = createSignal<BreakpointPopup | null>(null);
/** Expression to open the Evaluate dialog with (null = closed). */
const [evaluateRequest, setEvaluateRequest] = createSignal<{ expression: string } | null>(null);
/** Breakpoints dialog, optionally with a breakpoint preselected. */
const [breakpointsDialog, setBreakpointsDialog] = createSignal<{ select?: number } | null>(null);

export { breakpointPopup, setBreakpointPopup, evaluateRequest, setEvaluateRequest, breakpointsDialog, setBreakpointsDialog };

export function openBreakpointPopup(bp: Breakpoint, x: number, y: number): void {
  setBreakpointPopup({ id: bp.id, x, y });
}
