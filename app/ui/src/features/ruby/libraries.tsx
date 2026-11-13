/**
 * "External Libraries" of a Ruby project in the Project view, like RubyMine's: the
 * project's Ruby SDK (`< rbenv: 3.3.6 >`) and the gems of the bundle with their locked
 * versions (`ruby_bundle`: Gemfile merged with Gemfile.lock in Rust). Gems of the Gemfile
 * come first; gems they pull in are dimmed. Activating a direct gem shows its line in the
 * Gemfile; missing gems (not in Gemfile.lock) offer `bundle install`.
 */
import { createMemo, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, Gem, Library } from "lucide-solid";
import type { BundleGem } from "@/ipc/ruby";
import { projectTree, registerProjectViewSection } from "@/features/project/ProjectView";
import { revealSymbol } from "@/features/php/editorViews";
import { showResolvedMenu } from "@/features/contextMenu/state";
import { openSettings } from "@/features/settings/dialogState";
import { rubyActive, rubyBundle, rubyStatus } from "./state";
import { t } from "@/i18n";
import "./ruby.css";

export const RUBY_LIBRARIES_ID = "ruby-libraries://";
const SDK_ID = `${RUBY_LIBRARIES_ID}sdk`;
const GEMS_ID = `${RUBY_LIBRARIES_ID}gems`;

function Row(props: { id: string; depth: number; dir: boolean; name: string; hint?: string; gem?: BundleGem; onActivate?: () => void; onContextMenu?: (e: MouseEvent) => void }) {
  const { expanded, setOpen, toggle, selected, setSelected, singleClickOpens } = projectTree;
  const isOpen = () => expanded().has(props.id);
  const activate = () => (props.dir ? toggle(props.id) : props.onActivate?.());
  return (
    <div
      class="tree-row library-row"
      classList={{ selected: selected() === props.id, ignored: !!props.gem && !props.gem.direct }}
      style={{ "padding-left": `${8 + props.depth * 16}px` }}
      data-path={props.id}
      data-dir={props.dir ? "" : undefined}
      data-expanded={props.dir && isOpen() ? "" : undefined}
      data-depth={props.depth}
      title={props.gem ? `${props.gem.name} ${props.gem.version}  (${props.gem.groups.join(", ")}${props.gem.requirements?.length ? `; ${props.gem.requirements.join(", ")}` : ""})` : undefined}
      onClick={() => {
        setSelected(props.id);
        if (singleClickOpens()) activate();
      }}
      onDblClick={() => !singleClickOpens() && activate()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setSelected(props.id);
        props.onContextMenu?.(e);
      }}
    >
      <span
        class="tree-chevron"
        onClick={(e) => {
          if (!props.dir) return;
          e.stopPropagation();
          setOpen(props.id, !isOpen());
        }}
      >
        <Show when={props.dir}>{isOpen() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Show>
      </span>
      <span class="library-icon">
        <Show when={props.gem} fallback={<Library size={15} />}>
          <Gem size={14} class="ruby-gem-icon" classList={{ "ruby-gem-indirect": !props.gem!.direct }} />
        </Show>
      </span>
      <span class="tree-label">{props.name}</span>
      <Show when={props.hint}>
        <span class="tree-hint">{props.hint}</span>
      </Show>
    </div>
  );
}

const sdkMenu = (e: MouseEvent) => showResolvedMenu([{ kind: "action", label: t("langs.ruby.configureRubySdkEllipsis"), run: () => openSettings("lang.ruby") }], e.clientX, e.clientY);

/** The Ruby part of "External Libraries". */
export function RubyLibrariesNode() {
  const gems = createMemo(() => {
    const list = rubyBundle()?.gems ?? [];
    return [...list.filter((g) => g.direct).sort((a, b) => a.name.localeCompare(b.name)), ...list.filter((g) => !g.direct)];
  });
  const sdk = () => rubyStatus()?.chosen?.interpreter;
  const open = () => projectTree.expanded().has(RUBY_LIBRARIES_ID);
  return (
    <Show when={rubyActive() && (sdk() || gems().length > 0)}>
      <Row id={RUBY_LIBRARIES_ID} depth={0} dir name="External Libraries" onContextMenu={sdkMenu} />
      <Show when={open()}>
        <Show when={sdk()}>{(i) => <Row id={SDK_ID} depth={1} dir={false} name={`< ${i().name} >`} hint={i().path} onContextMenu={sdkMenu} onActivate={() => openSettings("lang.ruby")} />}</Show>
        <Show when={gems().length > 0}>
          <Row id={GEMS_ID} depth={1} dir name="Bundled Gems" hint={`${gems().length} gems`} />
          <Show when={projectTree.expanded().has(GEMS_ID)}>
            <For each={gems()}>
              {(g) => (
                <Row
                  id={`${GEMS_ID}/${g.name}`}
                  depth={2}
                  dir={false}
                  name={g.name}
                  hint={g.version ? `${g.version}${g.source !== "rubygems" ? ` (${g.source})` : ""}` : t("common.notInstalled2")}
                  gem={g}
                  onActivate={() => g.line !== undefined && void revealSymbol("Gemfile", { line: g.line, name: g.name })}
                />
              )}
            </For>
          </Show>
        </Show>
      </Show>
    </Show>
  );
}

registerProjectViewSection({ id: "ruby-libraries", order: 91, component: RubyLibrariesNode });
