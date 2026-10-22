/**
 * Spelling and grammar of the commit message (Commit tool window): the problems of the
 * message, green / blue squiggles drawn under the textarea's text, and the problem list
 * entries whose menu offers the same fixes as Alt+Enter in the editor.
 */
import { createEffect, createSignal, For, onCleanup, onMount, type Accessor } from "solid-js";
import { SpellCheck, WholeWord } from "lucide-solid";
import { spellApi, type ChangeToData, type ReplaceData, type SaveToData, type SpellProblem } from "@/ipc/spell";
import { item, separator, showContextMenu, type MenuEntry } from "@/features/contextMenu/state";
import { settingsState } from "@/state/settings";
import { dictionaryWords, inspectionsOn, saveWord, settingsFingerprint, spellOptions } from "./options";

/** The problems of `text`, checked a moment after it stops changing. */
export function createCommitMessageCheck(text: Accessor<string>): Accessor<SpellProblem[]> {
  const [problems, setProblems] = createSignal<SpellProblem[]>([]);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;
  createEffect(() => {
    const message = text();
    settingsFingerprint();
    clearTimeout(timer);
    const n = ++seq;
    if (!message.trim() || !inspectionsOn()) {
      setProblems([]);
      return;
    }
    timer = setTimeout(() => {
      spellApi
        .inspect({ segments: [{ from: 0, text: message, kind: "commitMessage" }], options: spellOptions(), words: dictionaryWords() })
        .then((p) => n === seq && setProblems(p))
        .catch(() => n === seq && setProblems([]));
    }, 250);
  });
  onCleanup(() => clearTimeout(timer));
  return problems;
}

/** The text with the problems' ranges as underlined spans, for the overlay. */
function pieces(text: string, problems: SpellProblem[]): { text: string; kind?: SpellProblem["kind"] }[] {
  const out: { text: string; kind?: SpellProblem["kind"] }[] = [];
  let at = 0;
  for (const p of [...problems].sort((a, b) => a.from - b.from)) {
    if (p.from < at || p.to > text.length) continue;
    if (p.from > at) out.push({ text: text.slice(at, p.from) });
    out.push({ text: text.slice(p.from, p.to), kind: p.kind });
    at = p.to;
  }
  out.push({ text: text.slice(at) + "\n" });
  return out;
}

/** Squiggles under a textarea: a transparent copy of its text that scrolls with it. */
export function SpellOverlay(props: { text: string; problems: SpellProblem[]; textarea: () => HTMLTextAreaElement | undefined }) {
  let el!: HTMLDivElement;
  const sync = () => {
    const ta = props.textarea();
    if (!ta) return;
    const cs = getComputedStyle(ta);
    const borders = parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth);
    const scrollbar = ta.offsetWidth - ta.clientWidth - borders;
    el.style.padding = cs.padding;
    el.style.paddingRight = `${parseFloat(cs.paddingRight) + Math.max(0, scrollbar)}px`;
    el.style.font = cs.font;
    el.style.lineHeight = cs.lineHeight;
    el.scrollTop = ta.scrollTop;
  };
  onMount(() => {
    const ta = props.textarea();
    if (!ta) return;
    ta.addEventListener("scroll", sync);
    const observer = new ResizeObserver(sync);
    observer.observe(ta);
    sync();
    onCleanup(() => {
      ta.removeEventListener("scroll", sync);
      observer.disconnect();
    });
  });
  createEffect(() => {
    props.text;
    props.problems;
    queueMicrotask(sync);
  });
  return (
    <div class="spell-overlay" ref={el} aria-hidden="true">
      <span class="spell-overlay-text">
        <For each={pieces(props.text, props.problems)}>
          {(p) => (p.kind ? <span class={`spell-mark ${p.kind}`}>{p.text}</span> : p.text)}
        </For>
      </span>
    </div>
  );
}

const lineOf = (text: string, offset: number) => text.slice(0, offset).split("\n").length;

/** The fixes of a commit message problem as a menu. */
async function showFixes(problem: SpellProblem, x: number, y: number, replace: (r: ReplaceData) => void): Promise<void> {
  const actions = await spellApi
    .contextActions({ problems: [problem], words: dictionaryWords(), hasProject: settingsState.hasProject, limit: 6 })
    .catch(() => []);
  const entries: MenuEntry[] = [];
  for (const a of actions) {
    if (a.action === "SpellCheck.ChangeTo") {
      const d = a.data as ChangeToData;
      for (const s of d.suggestions) entries.push(item(`Change to '${s}'`, () => replace({ from: d.from, to: d.to, text: s })));
      if (!d.suggestions.length) entries.push(item("No suggestions", () => undefined, undefined, { disabled: true }));
      entries.push(separator);
    } else if (a.action === "SpellCheck.SaveTo") {
      entries.push(item(a.title, () => void saveWord(a.data as SaveToData)));
    } else if (a.action === "SpellCheck.Replace") {
      entries.push(item(a.title, () => replace(a.data as ReplaceData)));
    }
  }
  showContextMenu(entries, x, y);
}

/** Entries of the commit message's problem list. */
export function CommitSpellProblems(props: { text: string; problems: SpellProblem[]; onReplace: (r: ReplaceData) => void }) {
  return (
    <For each={props.problems}>
      {(p) => (
        <li
          class={`spell-problem ${p.kind}`}
          data-spell-problem={p.word ?? p.rule}
          title="Show fixes"
          onClick={(e) => void showFixes(p, e.clientX, e.clientY, props.onReplace)}
        >
          {p.kind === "typo" ? <SpellCheck size={12} /> : <WholeWord size={12} />}
          <span>
            Line {lineOf(props.text, p.from)}: {p.message}
          </span>
        </li>
      )}
    </For>
  );
}
