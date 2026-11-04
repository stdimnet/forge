/**
 * The Rename dialog (Shift+F6 twice) and the Refactoring Preview, which lists PHPStorm's
 * "Problems Detected" first when there are conflicts. Both render in-page over the
 * workbench, like the other small modal dialogs.
 */
import { createSignal, For, Show } from "solid-js";
import { FilePen, FilePlus, FileX, TriangleAlert } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import type { FilePreview, PreviewRow, RenameConflict, UsagesAnswer } from "@/ipc/codeintel";
import { openFileAt } from "@/editor/views";
import { countLabel, openInFindWindow } from "./usages";
import type { PreviewRequest, RenameDialogRequest } from "./rename";
import { t } from "@/i18n";

const fileName = (p: string) => p.split("/").pop() ?? p;
const dirName = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");

export function RenameDialog(props: { req: RenameDialogRequest }) {
  const [name, setName] = createSignal(props.req.name);
  const [renameFile, setRenameFile] = createSignal(props.req.renameFile);
  const close = (preview: boolean | null) =>
    props.req.done(preview === null ? null : { name: name().trim(), renameFile: renameFile(), preview });
  return (
    <div class="popup-backdrop modal refactoring-backdrop">
      <div
        class="dialog rename-dialog"
        role="dialog"
        aria-label={t("rename.title")}
        onKeyDown={(e) => {
          if (e.key === "Escape") close(null);
          else if (e.key === "Enter" && name().trim()) close(false);
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{t("rename.title")}</div>
        <div class="dialog-body">
          <label class="form-label">{props.req.label}</label>
          <input
            ref={(el) =>
              queueMicrotask(() => {
                el.focus();
                el.select();
              })
            }
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
            spellcheck={false}
          />
          <Show when={props.req.classFile}>
            {(file) => (
              <label class="rename-option">
                <input type="checkbox" checked={renameFile()} onChange={(e) => setRenameFile(e.currentTarget.checked)} />
                {t("rename.renameFile", { name: fileName(file()) })}
              </label>
            )}
          </Show>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={() => close(true)} disabled={!name().trim()}>
            {t("rename.preview")}
          </button>
          <span class="dialog-buttons-gap" />
          <button class="btn-secondary" onClick={() => close(null)}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!name().trim()} onClick={() => close(false)}>
            {t("rename.refactor")}
          </button>
        </div>
      </div>
    </div>
  );
}

function RowText(props: { row: PreviewRow }) {
  const parts = () => {
    const t = props.row.text;
    const out: { text: string; hit: boolean }[] = [];
    let at = 0;
    for (const [s, e] of props.row.ranges) {
      if (s > at) out.push({ text: t.slice(at, s), hit: false });
      out.push({ text: t.slice(s, e), hit: true });
      at = e;
    }
    out.push({ text: t.slice(at), hit: false });
    return out;
  };
  return (
    <span class="usage-code">
      <For each={parts()}>{(p) => (p.hit ? <mark class="usage-match">{p.text}</mark> : p.text)}</For>
    </span>
  );
}

function FileBlock(props: { file: FilePreview }) {
  return (
    <div class="preview-file">
      <div class="preview-file-head">
        <FileIcon name={fileName(props.file.path)} size={14} />
        <span class="preview-file-name">{fileName(props.file.path)}</span>
        <span class="preview-file-dir">{dirName(props.file.path)}</span>
        <Show when={props.file.operation === "rename" && props.file.newPath}>
          {(to) => (
            <span class="preview-file-op">
              <FilePen size={12} />{" "}
              {dirName(to()) === dirName(props.file.path) ? t("rename.fileTo", { name: fileName(to()) }) : t("rename.moveTo", { path: to() })}
            </span>
          )}
        </Show>
        <Show when={props.file.operation === "create"}>
          <span class="preview-file-op">
            <FilePlus size={12} /> {t("rename.createFile")}
          </span>
        </Show>
        <Show when={props.file.operation === "delete"}>
          <span class="preview-file-op">
            <FileX size={12} /> {t("rename.deleteFile")}
          </span>
        </Show>
        <Show when={props.file.rows.length}>
          <span class="preview-file-count">{countLabel(props.file.rows.length, "occurrence")}</span>
        </Show>
      </div>
      <For each={props.file.rows}>
        {(row) => (
          <button class="preview-row" onDblClick={() => void openFileAt(props.file.path, row.line + 1, row.column + 1)}>
            <span class="preview-line">{row.line + 1}</span>
            <RowText row={row} />
          </button>
        )}
      </For>
    </div>
  );
}

/** The conflicts of a refactoring as a Find tool window tree (one row per located conflict). */
export function conflictsAnswer(title: string, conflicts: RenameConflict[]): UsagesAnswer {
  const byPath = new Map<string, RenameConflict[]>();
  for (const c of conflicts) if (c.path) byPath.set(c.path, [...(byPath.get(c.path) ?? []), c]);
  const files = [...byPath].map(([path, list]) => ({
    path,
    usages: list.map((c) => ({
      line: c.line ?? 0,
      column: 0,
      endLine: c.line ?? 0,
      endColumn: 0,
      text: c.message,
      ranges: [] as [number, number][],
      kind: "unclassified" as const,
    })),
  }));
  const count = files.reduce((n, f) => n + f.usages.length, 0);
  const tree = { total: count, fileCount: files.length, groups: [{ label: "Conflicts", count, files }] };
  return { title, source: "none", byType: tree, byFile: tree };
}

export function RefactoringPreviewDialog(props: { req: PreviewRequest }) {
  const a = () => props.req.answer;
  const total = () => a().preview.reduce((n, f) => n + f.rows.length, 0);
  const files = () => a().preview.filter((f) => f.operation === "edit").length;
  return (
    <div class="popup-backdrop modal refactoring-backdrop">
      <div
        class="dialog refactoring-preview"
        role="dialog"
        aria-label="Refactoring Preview"
        tabIndex={-1}
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => {
          if (e.key === "Escape") props.req.done(false);
          else if (e.key === "Enter") props.req.done(true);
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{a().conflicts.length ? "Problems Detected" : "Refactoring Preview"}</div>
        <div class="dialog-body">
          <Show when={a().conflicts.length}>
            <div class="preview-conflicts">
              <For each={a().conflicts}>
                {(c) => (
                  <div class="preview-conflict">
                    <TriangleAlert size={14} />
                    <span>{c.message}</span>
                    <Show when={c.path}>
                      {(p) => (
                        <button class="preview-conflict-link" onClick={() => void openFileAt(p(), (c.line ?? 0) + 1)}>
                          {fileName(p())}
                          {c.line != null ? `:${c.line + 1}` : ""}
                        </button>
                      )}
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>
          <div class="preview-summary">
            {props.req.title}: {countLabel(total(), "occurrence")} in {countLabel(files(), "file")}
            <Show when={a().skipped}>, {countLabel(a().skipped, "dynamic usage")} not renamed</Show>
          </div>
          <div class="preview-files">
            <For each={a().preview}>{(f) => <FileBlock file={f} />}</For>
          </div>
        </div>
        <div class="dialog-buttons">
          <Show when={a().conflicts.some((c) => c.path)}>
            <button
              class="btn-secondary"
              title="Cancel the refactoring and list the conflicts in the Find tool window"
              onClick={() => {
                openInFindWindow(conflictsAnswer(props.req.title, a().conflicts), {
                  kind: "text",
                  title: `Conflicts: ${props.req.title}`,
                  header: `Conflicts of ${props.req.title}`,
                });
                props.req.done(false);
              }}
            >
              Show in View
            </button>
            <span class="dialog-buttons-gap" />
          </Show>
          <button class="btn-secondary" onClick={() => props.req.done(false)}>
            Cancel
          </button>
          <button class="btn-primary" onClick={() => props.req.done(true)}>
            {a().conflicts.length ? "Continue" : "Do Refactor"}
          </button>
        </div>
      </div>
    </div>
  );
}
