import { Match, Switch } from "solid-js";
import { ArrowDown, ArrowRight, ChevronDown, ChevronRight, Play, SquareMinus, SquarePlus } from "lucide-solid";
import { iconLook } from "@/features/iconThemes/state";

/**
 * The expand arrow of a tree row, in the style the icon theme or the user picked
 * (`fileIcons.treeArrows`): chevrons (Forge's default), small triangles, boxed plus and
 * minus signs, straight arrows, or none. The row's `aria-expanded` carries the state.
 */
export function TreeArrow(props: { open: boolean }) {
  return (
    <Switch fallback={props.open ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}>
      <Match when={iconLook().arrows === "none"}>{null}</Match>
      <Match when={iconLook().arrows === "triangle"}>
        <Play
          size={8}
          fill="currentColor"
          stroke-width={0}
          class="tree-triangle"
          aria-hidden="true"
          style={{ transform: props.open ? "rotate(90deg)" : undefined, transition: "transform 80ms" }}
        />
      </Match>
      <Match when={iconLook().arrows === "plusMinus"}>
        {props.open ? (
          <SquareMinus size={11} stroke-width={1.75} class="tree-plus-minus" aria-hidden="true" />
        ) : (
          <SquarePlus size={11} stroke-width={1.75} class="tree-plus-minus" aria-hidden="true" />
        )}
      </Match>
      <Match when={iconLook().arrows === "arrow"}>
        {props.open ? <ArrowDown size={12} class="tree-straight-arrow" aria-hidden="true" /> : <ArrowRight size={12} class="tree-straight-arrow" aria-hidden="true" />}
      </Match>
    </Switch>
  );
}
