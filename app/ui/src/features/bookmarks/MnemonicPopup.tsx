/**
 * "Toggle Bookmark with Mnemonic" (Ctrl+F11): PHPStorm's grid of 0–9 and A–Z. Pressing a
 * digit or letter (or clicking it) assigns it to the line; a mnemonic that is already used
 * elsewhere is shown and moves here. Delete removes the bookmark.
 */
import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { suspendKeymap } from "@/keymap";
import { allBookmarks } from "./store";
import { t } from "@/i18n";

export interface MnemonicRequest {
  path: string;
  /** 0-based line. */
  line: number;
  current?: string;
  done: (choice: string | null | "delete") => void;
}

const [request, setRequest] = createSignal<MnemonicRequest | null>(null);

export function askMnemonic(path: string, line: number, current?: string): Promise<string | null | "delete"> {
  return new Promise((resolve) => setRequest({ path, line, current, done: resolve }));
}

const DIGITS = "1234567890".split("");
const LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("");

function Popup(props: { req: MnemonicRequest }) {
  let root!: HTMLDivElement;
  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => root.focus());
  const used = (m: string) => allBookmarks().find((b) => b.mnemonic === m);
  const finish = (choice: string | null | "delete") => {
    setRequest(null);
    props.req.done(choice);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") finish(null);
    else if (e.key === "Delete" || e.key === "Backspace") finish("delete");
    else if (/^[0-9a-zA-Z]$/.test(e.key)) finish(e.key.toUpperCase());
  };
  const Cell = (p: { m: string }) => (
    <button
      class="mnemonic-cell"
      classList={{ current: props.req.current === p.m, used: !!used(p.m) && props.req.current !== p.m }}
      title={used(p.m) ? `${used(p.m)!.path}:${used(p.m)!.line + 1}` : t("tools.bookmarks.bookmarkWithMnemonic", { mnemonic: p.m })}
      onClick={() => finish(p.m)}
    >
      {p.m}
    </button>
  );
  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && finish(null)}>
      <div class="popup mnemonic-popup" tabIndex={-1} ref={root} role="dialog" aria-label={t("tools.bookmarks.bookmarkMnemonic")} onKeyDown={onKeyDown}>
        <div class="mnemonic-title">{t("tools.bookmarks.bookmarkMnemonic")}</div>
        <div class="mnemonic-grid digits">
          <For each={DIGITS}>{(m) => <Cell m={m} />}</For>
        </div>
        <div class="mnemonic-grid letters">
          <For each={LETTERS}>{(m) => <Cell m={m} />}</For>
        </div>
        <div class="mnemonic-hint">
          {t("tools.bookmarks.pressADigitOrLetter")}
          <Show when={props.req.current}> {t("tools.bookmarks.deleteRemovesTheBookmark")}</Show>
        </div>
      </div>
    </div>
  );
}

export function MnemonicHost() {
  return <Show when={request()}>{(r) => <Popup req={r()} />}</Show>;
}
