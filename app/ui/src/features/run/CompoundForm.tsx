/**
 * The form of Compound configurations in the Run/Debug Configurations dialog: the
 * configurations it starts together (+ − ↑ ↓) and "Allow multiple instances", like PHPStorm.
 */
import { createSignal, For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ArrowDown, ArrowUp, Minus, Plus, Settings2 } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { openMenu } from "./PopupMenu";
import { COMPOUND, compoundMembers, findType } from "./types";
import { t } from "@/i18n";
import { listboxKeyDown, optionTabIndex } from "@/features/project/treeNavigation";

export function CompoundForm(props: {
  cfg: RunConfiguration;
  update: (fn: (cfg: RunConfiguration) => void) => void;
  configurations: RunConfiguration[];
}) {
  const [sel, setSel] = createSignal<number | null>(null);
  const members = () => compoundMembers(props.cfg);
  const setMembers = (names: string[]) =>
    props.update((c) => (c.options = { ...(c.options ?? {}), configurations: names.join("\n") }));
  const typeOf = (name: string) => props.configurations.find((c) => c.name === name)?.type;
  const move = (delta: number) => {
    const i = sel();
    const to = i === null ? -1 : i + delta;
    if (i === null || to < 0 || to >= members().length) return;
    const next = [...members()];
    [next[i], next[to]] = [next[to], next[i]];
    setMembers(next);
    setSel(to);
  };
  const addMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const candidates = props.configurations.filter((c) => c.name !== props.cfg.name && !members().includes(c.name));
    openMenu({
      x: r.left,
      y: r.bottom + 4,
      items: candidates.length
        ? candidates.map((c) => ({
            label: c.name,
            icon: findType(c.type)?.icon,
            iconColor: findType(c.type)?.color,
            run: () => setMembers([...members(), c.name]),
          }))
        : [{ label: t("run.compoundForm.noOtherConfigurations"), disabled: true }],
    });
  };
  return (
    <>
      <div class="rc-group-title">{t("run.compoundForm.configurations")}</div>
      <div class="rc-bl rc-compound" data-testid="rc-compound">
        <div class="rc-bl-tools">
          <button class="icon-btn" title={t("common.add")} onClick={addMenu}>
            <Plus size={14} />
          </button>
          <button
            class="icon-btn"
            title={t("common.remove")}
            disabled={sel() === null}
            onClick={() => {
              setMembers(members().filter((_, i) => i !== sel()));
              setSel(null);
            }}
          >
            <Minus size={14} />
          </button>
          <button class="icon-btn" title={t("common.moveUp")} disabled={!sel()} onClick={() => move(-1)}>
            <ArrowUp size={14} />
          </button>
          <button class="icon-btn" title={t("common.moveDown")} disabled={sel() === null || sel() === members().length - 1} onClick={() => move(1)}>
            <ArrowDown size={14} />
          </button>
        </div>
        <div
          class="rc-bl-list"
          role="listbox"
          aria-label={t("run.compoundForm.configurations")}
          onKeyDown={(e) => listboxKeyDown(e, members().map((_, n) => String(n)), sel() === null ? null : String(sel()), (id) => setSel(Number(id)))}
        >
          <For each={members()} fallback={<div class="rc-bl-empty muted">{t("run.compoundForm.clickToAddTheConfigurations")}</div>}>
            {(name, i) => {
              const type = () => findType(typeOf(name) ?? "");
              return (
                <div
                  class="rc-bl-item"
                  classList={{ selected: sel() === i(), invalid: !typeOf(name) }}
                  role="option"
                  aria-selected={sel() === i()}
                  tabIndex={optionTabIndex(String(i()), sel() === null ? null : String(sel()), "0")}
                  data-option-id={String(i())}
                  onClick={() => setSel(i())}
                >
                  <span style={{ color: type()?.color }}>
                    <Dynamic component={type()?.icon ?? Settings2} size={13} />
                  </span>
                  <span>{name}</span>
                  <Show when={!typeOf(name)}>
                    <span class="rc-compound-missing">{t("run.compoundForm.notFound")}</span>
                  </Show>
                  <Show when={typeOf(name) === COMPOUND}>
                    <span class="muted">{t("run.compoundForm.compound")}</span>
                  </Show>
                </div>
              );
            }}
          </For>
        </div>
      </div>
      <label class="rc-check rc-compound-multiple">
        <input
          type="checkbox"
          checked={props.cfg.options?.allowMultipleInstances === "true"}
          onChange={(e) => props.update((c) => (c.options = { ...(c.options ?? {}), allowMultipleInstances: String(e.currentTarget.checked) }))}
        />
        <span>{t("run.compoundForm.allowMultipleInstances")}</span>
      </label>
    </>
  );
}
