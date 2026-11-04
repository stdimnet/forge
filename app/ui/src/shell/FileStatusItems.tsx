/**
 * Status bar widgets of the active file, as in PHPStorm: line separator, encoding and
 * indent. Each opens a popup: change the separator (the file is converted and saved), reload
 * or convert the file in another encoding, configure indents or turn indent detection off.
 * The values come from Rust (`fs_read_text` detection, `editor_file_format`).
 */
import { Show } from "solid-js";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import { askChoice } from "@/features/contextMenu/dialogs";
import { openSettings } from "@/features/settings/dialogState";
import { showBalloon } from "@/keymap";
import { ENCODINGS, encodingLabel, eolLabel, type EncodingId, type Eol } from "@/ipc/text";
import {
  changeLineSeparator,
  convertEncoding,
  isDirty,
  reloadWithEncoding,
  type EditorTab,
} from "@/state/editor";
import { getSetting, saveSetting } from "@/state/settings";
import { t } from "@/i18n";

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Open a popup above a status bar button. */
function popupAbove(el: HTMLElement, entries: ResolvedEntry[]): void {
  const r = el.getBoundingClientRect();
  showResolvedMenu(entries, r.left, r.top - 2, { above: true, class: "status-popup" });
}

const SEPARATORS: { eol: Eol; label: string }[] = [
  { eol: "\r\n", label: t("statusbar.eol.crlf") },
  { eol: "\n", label: t("statusbar.eol.lf") },
  { eol: "\r", label: t("statusbar.eol.cr") },
];

/** The separator the file is written with (a new file gets the code style's one). */
export const effectiveEol = (tab: EditorTab): Eol => (tab.eolKnown ? tab.eol : (tab.format?.lineSeparator ?? tab.eol));

export function LineSeparatorStatus(props: { tab: EditorTab }) {
  const open = (el: HTMLElement) => {
    const current = effectiveEol(props.tab);
    popupAbove(
      el,
      SEPARATORS.map(({ eol, label }) => ({
        kind: "action" as const,
        label,
        checkable: true,
        checked: eol === current,
        run: () => {
          void changeLineSeparator(props.tab.path, eol).catch((e) => showBalloon(t("statusbar.eol.failed"), message(e), 6000));
        },
      })),
    );
  };
  return (
    <button
      class="status-item"
      data-status="line-separator"
      title={t("statusbar.eol.tooltip", { eol: eolLabel(effectiveEol(props.tab)) })}
      onClick={(e) => open(e.currentTarget)}
    >
      {eolLabel(effectiveEol(props.tab))}
    </button>
  );
}

async function pickEncoding(tab: EditorTab, encoding: EncodingId): Promise<void> {
  const dirty = isDirty(tab);
  const choice = await askChoice({
    title: t("statusbar.encoding.reloadOrConvert", { file: tab.name, encoding }),
    message: t("statusbar.encoding.question", { file: tab.name, encoding }),
    choices: [
      {
        id: "reload",
        label: t("statusbar.encoding.reload"),
        disabled: dirty,
        hint: dirty ? t("statusbar.encoding.saveFirst") : t("statusbar.encoding.reloadHint"),
      },
      { id: "convert", label: t("statusbar.encoding.convert"), hint: t("statusbar.encoding.convertHint") },
    ],
  });
  try {
    if (choice === "reload") await reloadWithEncoding(tab.path, encoding);
    else if (choice === "convert") await convertEncoding(tab.path, encoding, encoding.startsWith("UTF-16") || (encoding === "UTF-8" && tab.bom));
  } catch (e) {
    showBalloon(t("statusbar.encoding.convertFailed", { encoding }), message(e), 8000);
  }
}

export function EncodingStatus(props: { tab: EditorTab }) {
  const open = (el: HTMLElement) => {
    const tab = props.tab;
    const entries: ResolvedEntry[] = ENCODINGS.map((encoding) => ({
      kind: "action" as const,
      label: encoding,
      checkable: true,
      checked: encoding === tab.encoding,
      run: () => {
        if (encoding !== tab.encoding) void pickEncoding(tab, encoding);
      },
    }));
    if (tab.encoding === "UTF-8") {
      entries.push("separator", {
        kind: "action",
        label: tab.bom ? t("statusbar.bom.remove") : t("statusbar.bom.add"),
        run: () => {
          void convertEncoding(tab.path, "UTF-8", !tab.bom).catch((e) => showBalloon(t("statusbar.bom.failed"), message(e), 6000));
        },
      });
    }
    popupAbove(el, entries);
  };
  const charsetMismatch = () => {
    const c = props.tab.format?.charset;
    return c && (c.encoding !== props.tab.encoding || c.bom !== props.tab.bom) ? encodingLabel(c.encoding, c.bom) : null;
  };
  return (
    <button
      class="status-item"
      data-status="encoding"
      title={
        t("statusbar.encoding.tooltip", { encoding: encodingLabel(props.tab.encoding, props.tab.bom) }) +
        (charsetMismatch() ? t("statusbar.encoding.editorConfig", { encoding: charsetMismatch() }) : "")
      }
      onClick={(e) => open(e.currentTarget)}
    >
      {encodingLabel(props.tab.encoding, props.tab.bom)}
    </button>
  );
}

export function IndentStatus(props: { tab: EditorTab }) {
  // The status bar may still read this while the active tab switches to a custom tab.
  const format = () => props.tab?.format;
  const tooltip = () => {
    const f = format();
    if (!f) return t("statusbar.indent");
    const source = {
      codeStyle: t("statusbar.indent.codeStyle"),
      fileType: t("statusbar.indent.fileType"),
      editorConfig: t("statusbar.indent.editorConfig"),
      detected: t("statusbar.indent.detected", {
        indent: f.configuredIndent.useTabs ? t("statusbar.indent.tab") : t("statusbar.indent.spaces", { count: f.configuredIndent.size }),
      }),
    }[f.indentSource];
    return t("statusbar.indent.tooltip", { indent: f.indentLabel.replace(/\*$/, ""), source });
  };
  const open = (el: HTMLElement) => {
    const f = format();
    const detect = getSetting<boolean>("editor.codeStyle.detectIndents", true);
    const entries: ResolvedEntry[] = [];
    entries.push({
      kind: "action",
      label: t("statusbar.indent.configureFor", { language: f?.codeStylePage ? (f.languageName ?? t("statusbar.indent.theLanguage")) : t("statusbar.indent.allFiles") }),
      run: () => openSettings(f?.codeStylePage ?? "editor.code-style"),
    });
    entries.push({
      kind: "action",
      label: detect ? t("statusbar.indent.disableDetection") : t("statusbar.indent.enableDetection"),
      run: () => void saveSetting("editor.codeStyle.detectIndents", !detect, "project"),
    });
    if (f?.editorConfig) {
      entries.push("separator", {
        kind: "action",
        label: t("statusbar.indent.byEditorConfig"),
        disabled: true,
      });
    }
    popupAbove(el, entries);
  };
  return (
    <Show when={format()}>
      {(f) => (
        <button class="status-item" data-status="indent" title={tooltip()} onClick={(e) => open(e.currentTarget)}>
          {f().indentLabel}
        </button>
      )}
    </Show>
  );
}
