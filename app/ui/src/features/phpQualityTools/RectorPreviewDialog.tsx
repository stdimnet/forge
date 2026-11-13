/**
 * Rector's changes before they are applied: the rules that apply and the diff Rector
 * printed (`rector process --dry-run`). Apply puts the changes into the editor as one edit.
 */
import { For, Show } from "solid-js";
import { closeDialog, type DialogComponentProps } from "@/shell/window/dialogs";
import { t } from "@/i18n";

export const RECTOR_DIALOG = "rector-preview";

export interface RectorPreviewProps {
  path: string;
  diff: string;
  rules: string[];
}

/** `Rector\Php80\Rector\Class_\ClassPropertyAssignToConstructorPromotionRector` → the class name. */
const shortRule = (rule: string) => rule.split("\\").pop() ?? rule;

function lineClass(line: string): string {
  if (line.startsWith("@@")) return "qt-diff-hunk";
  if (line.startsWith("+++") || line.startsWith("---")) return "qt-diff-file";
  if (line.startsWith("+")) return "qt-diff-add";
  if (line.startsWith("-")) return "qt-diff-del";
  return "";
}

export function RectorPreviewDialog(props: DialogComponentProps<RectorPreviewProps>) {
  const close = (apply: boolean) => closeDialog(RECTOR_DIALOG, apply ? { apply: true } : null);
  const lines = () => props.props.diff.replace(/\n$/, "").split("\n");
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") close(false);
    else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "BUTTON") close(true);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div class="qt-dialog" onKeyDown={onKeyDown} tabIndex={-1} ref={(el) => queueMicrotask(() => el.focus())}>
      <div class="qt-dialog-body">
        <div class="qt-dialog-file">{props.props.path}</div>
        <Show when={props.props.rules.length}>
          <div class="qt-dialog-label">{t("langs.phpQualityTools.appliedRulesLabel")}</div>
          <ul class="qt-rules">
            <For each={props.props.rules}>{(r) => <li title={r}>{shortRule(r)}</li>}</For>
          </ul>
        </Show>
        <pre class="qt-diff" data-testid="rector-diff">
          <For each={lines()}>{(l) => <div class={lineClass(l)}>{l || " "}</div>}</For>
        </pre>
      </div>
      <footer class="qt-dialog-footer">
        <span class="qt-spacer" />
        <button class="btn-secondary set-btn" onClick={() => close(false)}>
          {t("common.cancel")}
        </button>
        <button class="btn-primary set-btn" data-testid="rector-apply" onClick={() => close(true)}>
          {t("common.apply")}
        </button>
      </footer>
    </div>
  );
}
