/**
 * Changelist and patch dialogs (see `changesDialogs.ts`), laid out like PHPStorm's: New /
 * Edit Changelist, Move to Another Changelist, Create Patch and Apply Patch with a preview of
 * every file.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { FileDiff, FolderOpen, TriangleAlert } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { isTauri } from "@/ipc";
import { getSetting } from "@/state/settings";
import type { ChangelistView, PatchPreviewFile } from "./changesApi";
import { changesApi } from "./changesApi";
import { activeChangelist, changelists } from "./changelists";
import {
  applyPatch,
  chooseSavePath,
  createPatch,
  moveToChangelist,
  saveChangelist,
  showPatchFileDiff,
} from "./changesDialogs";
import { closeGitDialog } from "./gitDialogs";
import { Check, Frame, Row } from "./DialogFrame";

const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
const plural = (n: number, what: string) => `${n} ${what}${n === 1 ? "" : "s"}`;
const statusClass = (s: string) => (s === "added" ? "added" : s === "deleted" ? "deleted" : "modified");

/** New Changelist / Edit Changelist. */
export function ChangelistDialog(props: { edit: ChangelistView | null; movePaths: string[] }) {
  const [name, setName] = createSignal(props.edit?.name ?? "");
  const [comment, setComment] = createSignal(props.edit?.comment ?? "");
  const [active, setActive] = createSignal(props.edit?.active ?? false);
  const taken = () => changelists().some((l) => l.id !== props.edit?.id && l.name.trim().toLowerCase() === name().trim().toLowerCase());
  return (
    <Frame
      title={props.edit ? "Edit Changelist" : "New Changelist"}
      class="gd-wide"
      okLabel={props.edit ? "OK" : "Create"}
      okDisabled={!name().trim() || taken()}
      onOk={async () => {
        const ok = await saveChangelist(props.edit, name(), comment(), active(), props.movePaths);
        if (ok) closeGitDialog();
      }}
    >
      <div class="gd-grid">
        <Row label="Name:">
          <input class="gd-input" classList={{ "gd-invalid": taken() }} data-autofocus value={name()} spellcheck={false} onInput={(e) => setName(e.currentTarget.value)} />
        </Row>
      </div>
      <Show when={taken()}>
        <span class="gd-note gd-error">A changelist with this name already exists</span>
      </Show>
      <label class="git-dialog-label">Comment:</label>
      <textarea class="gd-textarea" value={comment()} spellcheck={false} onInput={(e) => setComment(e.currentTarget.value)} />
      <Check label="Set active" checked={active()} disabled={!!props.edit?.active} onChange={setActive} />
    </Frame>
  );
}

/** Move to Another Changelist (F6): an existing list or a new one. */
export function MoveChangelistDialog(props: { paths: string[]; hunks: string[] | null; from: string | null }) {
  const others = () => changelists().filter((l) => l.id !== props.from);
  const [mode, setMode] = createSignal<"existing" | "new">(others().length ? "existing" : "new");
  const [target, setTarget] = createSignal(others().find((l) => l.active)?.id ?? others()[0]?.id ?? "");
  const [name, setName] = createSignal("");
  const [active, setActive] = createSignal(false);
  const taken = () => changelists().some((l) => l.name.trim().toLowerCase() === name().trim().toLowerCase());
  const what = () => (props.hunks ? plural(props.hunks.length, "change") + ` of ${nameOf(props.paths[0])}` : props.paths.length === 1 ? nameOf(props.paths[0]) : plural(props.paths.length, "file"));
  return (
    <Frame
      title="Move to Another Changelist"
      class="gd-wide gd-move-changelist"
      okLabel="OK"
      okDisabled={mode() === "existing" ? !target() : !name().trim() || taken()}
      onOk={async () => {
        const ok = await moveToChangelist(props.paths, props.hunks, mode() === "existing" ? { id: target() } : { newName: name(), active: active() });
        if (ok) closeGitDialog();
      }}
    >
      <span class="gd-note">Move {what()} to:</span>
      <div class="gd-radios gd-changelist-choice" role="radiogroup">
        <label class="gd-radio-row">
          <input type="radio" name="cl-mode" checked={mode() === "existing"} disabled={!others().length} onChange={() => setMode("existing")} />
          Existing changelist:
          <select class="gd-select" value={target()} disabled={mode() !== "existing"} onChange={(e) => setTarget(e.currentTarget.value)}>
            <For each={others()}>{(l) => <option value={l.id}>{l.name + (l.active ? " (active)" : "")}</option>}</For>
          </select>
        </label>
        <label class="gd-radio-row">
          <input type="radio" name="cl-mode" checked={mode() === "new"} onChange={() => setMode("new")} />
          New changelist:
          <input
            class="gd-input"
            classList={{ "gd-invalid": mode() === "new" && taken() }}
            value={name()}
            placeholder="Name"
            spellcheck={false}
            data-autofocus={mode() === "new" ? "" : undefined}
            onFocus={() => setMode("new")}
            onInput={(e) => setName(e.currentTarget.value)}
          />
        </label>
      </div>
      <Check label="Set active" checked={active()} disabled={mode() !== "new"} onChange={setActive} />
    </Frame>
  );
}

/** Create Patch: to a file (relative paths are in the project) or to the clipboard. */
export function CreatePatchDialog(props: { paths: string[]; shas: string[]; name: string }) {
  const safe = props.name.replace(/[\\/:*?"<>|\n]+/g, "_").trim() || "Changes";
  const [file, setFile] = createSignal(`${safe}.patch`);
  const [toClipboard, setToClipboard] = createSignal(false);
  const [reverse, setReverse] = createSignal(false);
  const source = () => (props.shas.length ? plural(props.shas.length, "commit") : plural(props.paths.length, "file"));
  return (
    <Frame
      title="Create Patch"
      class="gd-wide"
      okLabel="Create"
      okDisabled={!toClipboard() && !file().trim()}
      onOk={async () => {
        const ok = await createPatch({ paths: props.paths, shas: props.shas, file: toClipboard() ? null : file().trim(), reverse: reverse() });
        if (ok) closeGitDialog();
      }}
    >
      <span class="gd-note">Changes: {source()}</span>
      <div class="gd-radios" role="radiogroup">
        <label class="gd-radio-row">
          <input type="radio" name="patch-target" checked={!toClipboard()} onChange={() => setToClipboard(false)} />
          Save to file:
          <span class="gd-inline gd-grow">
            <input class="gd-input" data-autofocus value={file()} disabled={toClipboard()} spellcheck={false} onInput={(e) => setFile(e.currentTarget.value)} />
            <Show when={isTauri()}>
              <button
                class="icon-btn"
                title="Browse…"
                disabled={toClipboard()}
                onClick={async () => {
                  const p = await chooseSavePath(file());
                  if (p) setFile(p);
                }}
              >
                <FolderOpen size={15} />
              </button>
            </Show>
          </span>
        </label>
        <label class="gd-radio-row">
          <input type="radio" name="patch-target" checked={toClipboard()} onChange={() => setToClipboard(true)} />
          Copy patch to clipboard
        </label>
      </div>
      <span class="gd-note">Paths are relative to the project root; a relative file name is saved in the project.</span>
      <Show when={!props.shas.length}>
        <Check label="Reverse patch" checked={reverse()} onChange={setReverse} />
      </Show>
    </Frame>
  );
}

/** Apply Patch: the files with their state against the base directory, a diff per file. */
export function ApplyPatchDialog(props: { patch: string; name: string; files: PatchPreviewFile[] }) {
  const [base, setBase] = createSignal("");
  const [files, setFiles] = createSignal(props.files);
  const [checked, setChecked] = createSignal(new Set(props.files.map((f) => f.path)));
  const auto = getSetting<boolean>("vcs.changelists.createAutomatically", true);
  const [target, setTarget] = createSignal<string>(auto ? "__new" : (activeChangelist()?.id ?? ""));
  let timer: ReturnType<typeof setTimeout> | undefined;
  const rebase = (b: string) => {
    setBase(b);
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const next = await changesApi.patchPreview(props.patch, b.trim()).catch(() => files());
      setFiles(next);
      setChecked(new Set(next.map((f) => f.path)));
    }, 250);
  };
  const failing = createMemo(() => files().filter((f) => !f.applies).length);
  return (
    <Frame
      title="Apply Patch"
      class="gd-wide gd-apply-patch"
      okLabel="OK"
      okDisabled={!checked().size}
      onOk={async () => {
        const all = checked().size === files().length;
        const paths = all ? null : files().map((f) => f.path).filter((p) => checked().has(p));
        closeGitDialog();
        const t = target();
        await applyPatch(props.patch, base().trim(), paths, t === "__new" ? { newName: props.name } : t ? { id: t } : {});
      }}
    >
      <div class="gd-grid">
        <Row label="Patch:">
          <span class="gd-static">{props.name}</span>
        </Row>
        <Row label="Base directory:">
          <input class="gd-input" value={base()} placeholder="Project root" spellcheck={false} onInput={(e) => rebase(e.currentTarget.value)} />
        </Row>
        <Row label="Changelist:">
          <select class="gd-select" value={target()} onChange={(e) => setTarget(e.currentTarget.value)}>
            <option value="__new">New: {props.name}</option>
            <For each={changelists()}>{(l) => <option value={l.id}>{l.name + (l.active ? " (active)" : "")}</option>}</For>
          </select>
        </Row>
      </div>
      <div class="git-chooser-list gd-list gd-file-checks gd-patch-files">
        <label class="gd-file gd-file-all">
          <input
            type="checkbox"
            checked={checked().size === files().length}
            onChange={(e) => setChecked(e.currentTarget.checked ? new Set(files().map((f) => f.path)) : new Set<string>())}
          />
          <span class="strong">{props.name}</span>
          <span class="tree-hint">
            {checked().size} of {plural(files().length, "file")}
          </span>
        </label>
        <For each={files()}>
          {(f) => (
            <label class="gd-file gd-file-indent" data-path={f.path} title={f.message ?? f.path} onDblClick={() => showPatchFileDiff(f)}>
              <input
                type="checkbox"
                checked={checked().has(f.path)}
                onChange={(e) => {
                  const next = new Set(checked());
                  e.currentTarget.checked ? next.add(f.path) : next.delete(f.path);
                  setChecked(next);
                }}
              />
              <FileIcon name={nameOf(f.path)} />
              <span class={`vcs-${statusClass(f.status)}`}>{nameOf(f.path)}</span>
              <span class="tree-hint">{dirOf(f.path)}</span>
              <span class="gd-patch-counts">
                <Show when={f.binary} fallback={<><span class="gd-plus">+{f.additions}</span> <span class="gd-minus">−{f.deletions}</span></>}>
                  binary
                </Show>
              </span>
              <Show when={!f.applies}>
                <span class="gd-patch-warn" title={`${f.message ?? "Does not apply"}: will be merged three-way`}>
                  <TriangleAlert size={13} />
                </span>
              </Show>
              <button class="icon-btn gd-file-diff" title="Show Diff" onClick={(e) => (e.preventDefault(), showPatchFileDiff(f))}>
                <FileDiff size={14} />
              </button>
            </label>
          )}
        </For>
      </div>
      <Show when={failing() > 0}>
        <span class="gd-note gd-warn">
          {plural(failing(), "file")} cannot be patched as they are now; they will be merged three-way and conflicts open the merge dialog.
        </span>
      </Show>
    </Frame>
  );
}
