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

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Open a popup above a status bar button. */
function popupAbove(el: HTMLElement, entries: ResolvedEntry[]): void {
  const r = el.getBoundingClientRect();
  showResolvedMenu(entries, r.left, r.top - 2, { above: true, class: "status-popup" });
}

const SEPARATORS: { eol: Eol; label: string }[] = [
  { eol: "\r\n", label: "CRLF - Windows (\\r\\n)" },
  { eol: "\n", label: "LF - Unix and macOS (\\n)" },
  { eol: "\r", label: "CR - Classic Mac OS (\\r)" },
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
          void changeLineSeparator(props.tab.path, eol).catch((e) => showBalloon("Cannot change the line separator", message(e), 6000));
        },
      })),
    );
  };
  return (
    <button
      class="status-item"
      data-status="line-separator"
      title={`Line Separator: ${eolLabel(effectiveEol(props.tab))}`}
      onClick={(e) => open(e.currentTarget)}
    >
      {eolLabel(effectiveEol(props.tab))}
    </button>
  );
}

async function pickEncoding(tab: EditorTab, encoding: EncodingId): Promise<void> {
  const dirty = isDirty(tab);
  const choice = await askChoice({
    title: `${tab.name}: Reload or Convert to ${encoding}`,
    message: `The encoding you've chosen ('${encoding}') may change the contents of '${tab.name}'. Do you want to reload the file from disk or convert the text and save in the new encoding?`,
    choices: [
      {
        id: "reload",
        label: "Reload",
        disabled: dirty,
        hint: dirty ? "Save or revert the changes first" : "Read the file again in the new encoding",
      },
      { id: "convert", label: "Convert", hint: "Save the text in the new encoding" },
    ],
  });
  try {
    if (choice === "reload") await reloadWithEncoding(tab.path, encoding);
    else if (choice === "convert") await convertEncoding(tab.path, encoding, encoding.startsWith("UTF-16") || (encoding === "UTF-8" && tab.bom));
  } catch (e) {
    showBalloon(`Cannot convert to ${encoding}`, message(e), 8000);
  }
}

export function EncodingStatus(props: { tab: EditorTab }) {
  const open = (el: HTMLElement) => {
    const t = props.tab;
    const entries: ResolvedEntry[] = ENCODINGS.map((encoding) => ({
      kind: "action" as const,
      label: encoding,
      checkable: true,
      checked: encoding === t.encoding,
      run: () => {
        if (encoding !== t.encoding) void pickEncoding(t, encoding);
      },
    }));
    if (t.encoding === "UTF-8") {
      entries.push("separator", {
        kind: "action",
        label: t.bom ? "Remove BOM" : "Add BOM",
        run: () => {
          void convertEncoding(t.path, "UTF-8", !t.bom).catch((e) => showBalloon("Cannot change the BOM", message(e), 6000));
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
        `File Encoding: ${encodingLabel(props.tab.encoding, props.tab.bom)}` +
        (charsetMismatch() ? ` (EditorConfig asks for ${charsetMismatch()})` : "")
      }
      onClick={(e) => open(e.currentTarget)}
    >
      {encodingLabel(props.tab.encoding, props.tab.bom)}
    </button>
  );
}

export function IndentStatus(props: { tab: EditorTab }) {
  const format = () => props.tab.format;
  const tooltip = () => {
    const f = format();
    if (!f) return "Indent";
    const source = {
      codeStyle: "from the code style",
      fileType: "default of the file type",
      editorConfig: "from EditorConfig",
      detected: `detected in the file; the code style says ${f.configuredIndent.useTabs ? "Tab" : `${f.configuredIndent.size} spaces`}`,
    }[f.indentSource];
    return `Indent: ${f.indentLabel.replace(/\*$/, "")} (${source})`;
  };
  const open = (el: HTMLElement) => {
    const f = format();
    const detect = getSetting<boolean>("editor.codeStyle.detectIndents", true);
    const entries: ResolvedEntry[] = [];
    entries.push({
      kind: "action",
      label: `Configure Indents for ${f?.codeStylePage ? (f.languageName ?? "the language") : "All Files"}…`,
      run: () => openSettings(f?.codeStylePage ?? "editor.code-style"),
    });
    entries.push({
      kind: "action",
      label: detect ? "Disable Indents Detection for Project" : "Enable Indents Detection for Project",
      run: () => void saveSetting("editor.codeStyle.detectIndents", !detect, "project"),
    });
    if (f?.editorConfig) {
      entries.push("separator", {
        kind: "action",
        label: "Indents are set by .editorconfig",
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
