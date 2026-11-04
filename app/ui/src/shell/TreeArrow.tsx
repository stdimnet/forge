import { Match, Switch } from "solid-js";
import { ChevronDown, ChevronRight, Play } from "lucide-solid";
import { iconLook } from "@/features/iconThemes/state";

/**
 * The expand arrow of a tree row, in the style the icon theme or the user picked
 * (`fileIcons.treeArrows`): chevrons (Forge's default), small triangles, or none.
 */
export function TreeArrow(props: { open: boolean }) {
  return (
    <Switch fallback={props.open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}>
      <Match when={iconLook().arrows === "none"}>{null}</Match>
      <Match when={iconLook().arrows === "triangle"}>
        <Play
          size={8}
          fill="currentColor"
          stroke-width={0}
          class="tree-triangle"
          style={{ transform: props.open ? "rotate(90deg)" : undefined, transition: "transform 80ms" }}
        />
      </Match>
    </Switch>
  );
}
