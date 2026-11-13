import type { Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Asterisk, Hexagon, Route, Cpu, FlaskConical, SquareTerminal, Server } from "lucide-solid";
import type { ProviderKind } from "@/ipc/ai";

type Icon = Component<{ size?: number; "stroke-width"?: number }>;

/** Icon + theme color per provider kind (colors are existing theme tokens). */
export const kindStyle: Record<ProviderKind, { icon: Icon; color: string }> = {
  anthropic: { icon: Asterisk, color: "var(--accent)" },
  openai: { icon: Hexagon, color: "var(--success)" },
  openrouter: { icon: Route, color: "var(--info)" },
  ollama: { icon: Cpu, color: "var(--syn-type)" },
  lmstudio: { icon: FlaskConical, color: "var(--syn-variable)" },
  llamacpp: { icon: SquareTerminal, color: "var(--warning)" },
  custom: { icon: Server, color: "var(--fg-muted)" },
};

export function KindIcon(props: { kind: ProviderKind; size?: number }) {
  const style = () => kindStyle[props.kind];
  const size = () => props.size ?? 24;
  return (
    <span
      class="ai-kind-icon"
      style={{ "--kind-color": style().color, width: `${size()}px`, height: `${size()}px` }}
    >
      <Dynamic component={style().icon} size={Math.round(size() * 0.6)} stroke-width={2.2} />
    </span>
  );
}
