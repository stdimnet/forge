/**
 * PHPStorm's "Optimize Imports" dialog for the Project view: what is optimized (a
 * directory, the project or the selected files), "Include subdirectories" and "Only VCS
 * changed files". Keys are handled on the dialog element itself, not globally.
 */
import { createSignal, onCleanup, Show } from "solid-js";
import { suspendKeymap } from "@/keymap";
import { repoAvailable } from "@/features/git/store";
import { workspace } from "@/state/workspace";

export interface OptimizeTarget {
  path: string;
  dir: boolean;
}

export interface OptimizeOptions {
  includeSubdirectories: boolean;
  onlyVcsChanged: boolean;
}

interface Request {
  targets: OptimizeTarget[];
  done: (options: OptimizeOptions | null) => void;
}

const [request, setRequest] = createSignal<Request | null>(null);

/** Ask for the options of Optimize Imports on `targets`; null when cancelled. */
export function askOptimizeImports(targets: OptimizeTarget[]): Promise<OptimizeOptions | null> {
  return new Promise((resolve) => setRequest({ targets, done: resolve }));
}

/** "Optimize imports in directory 'app/Models'", "… in project 'shop'", "… in 3 files". */
export function describeTargets(targets: OptimizeTarget[], project: string): string {
  if (targets.length === 1) {
    const t = targets[0];
    if (t.dir && !t.path) return `Optimize imports in project '${project}'`;
    return t.dir ? `Optimize imports in directory '${t.path}'` : `Optimize imports in file '${t.path.split("/").pop()}'`;
  }
  const dirs = targets.filter((t) => t.dir).length;
  if (!dirs) return `Optimize imports in ${targets.length} files`;
  return `Optimize imports in ${targets.length} selected items`;
}

function Dialog(props: { req: Request }) {
  const [subdirs, setSubdirs] = createSignal(true);
  const [vcs, setVcs] = createSignal(false);
  const hasDirs = () => props.req.targets.some((t) => t.dir);
  const close = (ok: boolean) => {
    setRequest(null);
    props.req.done(ok ? { includeSubdirectories: !hasDirs() || subdirs(), onlyVcsChanged: vcs() } : null);
  };
  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog optimize-imports-dialog"
        style={{ width: "min(460px, calc(100vw - 32px))" }}
        role="dialog"
        aria-label="Optimize Imports"
        tabIndex={-1}
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => {
          if (e.key === "Escape") close(false);
          else if (e.key === "Enter") close(true);
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">Optimize Imports</div>
        <div class="dialog-body">
          <div class="optimize-imports-scope">{describeTargets(props.req.targets, workspace()?.name ?? "project")}</div>
          <Show when={hasDirs()}>
            <label class="form-label check">
              <input type="checkbox" checked={subdirs()} onChange={(e) => setSubdirs(e.currentTarget.checked)} />
              Include subdirectories
            </label>
          </Show>
          <label class="form-label check" classList={{ disabled: !repoAvailable() }} title={repoAvailable() ? undefined : "The project is not under version control"}>
            <input type="checkbox" checked={vcs()} disabled={!repoAvailable()} onChange={(e) => setVcs(e.currentTarget.checked)} />
            Only VCS changed files
          </label>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={() => close(false)}>
            Cancel
          </button>
          <button class="btn-primary" onClick={() => close(true)}>
            Run
          </button>
        </div>
      </div>
    </div>
  );
}

export function OptimizeImportsDialogHost() {
  return (
    <Show when={request()} keyed>
      {(req) => {
        const release = suspendKeymap();
        onCleanup(release);
        return <Dialog req={req} />;
      }}
    </Show>
  );
}
