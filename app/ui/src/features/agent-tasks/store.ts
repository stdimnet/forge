/** State of the Agent Tasks tool window: the project's tasks, the selection and the form. */
import { createSignal } from "solid-js";
import { agentTasksApi, type AgentTask, type ApplyAnswer, type ApplyMode, type FileChange } from "@/ipc/agentTasks";
import { setDiffRequest } from "@/features/git/overlays";
import { logNotification } from "@/state/notifications";
import { t } from "@/i18n";

const [tasks, setTasks] = createSignal<AgentTask[]>([]);
const [selectedId, setSelectedId] = createSignal<string | null>(null);
const [formOpen, setFormOpen] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);
/** Changed files of the selected task (reviewable tasks only). */
const [changes, setChanges] = createSignal<FileChange[] | null>(null);
/** The last Apply's answer, shown above the changes. */
const [applyAnswer, setApplyAnswer] = createSignal<ApplyAnswer | null>(null);
const [busy, setBusy] = createSignal(false);

export { tasks, selectedId, formOpen, setFormOpen, error, setError, changes, applyAnswer, busy };

export const selectedTask = () => tasks().find((t) => t.id === selectedId()) ?? null;

let subscribed = false;

/** Load the tasks and follow their changes. */
export async function loadTasks(): Promise<void> {
  try {
    const list = await agentTasksApi.list();
    setTasks(list);
    if (!selectedId() && list.length) select(list[0].id);
    setError(null);
  } catch (e) {
    setError(String(e));
  }
  if (!subscribed) {
    subscribed = true;
    void agentTasksApi.onChanged((t) => {
      setTasks((list) => {
        const i = list.findIndex((x) => x.id === t.id);
        if (i < 0) return [t, ...list];
        const next = list.slice();
        next[i] = t;
        return next;
      });
      if (t.id === selectedId() && (t.status === "needsReview" || t.status === "failed" || t.status === "cancelled")) {
        void refreshChanges();
      }
    });
  }
}

export function select(id: string | null): void {
  setSelectedId(id);
  setApplyAnswer(null);
  setChanges(null);
  void refreshChanges();
}

export async function refreshChanges(): Promise<void> {
  const t = selectedTask();
  if (!t || !t.sandbox || !(t.status === "needsReview" || t.status === "failed" || t.status === "cancelled")) {
    setChanges(null);
    return;
  }
  try {
    setChanges(await agentTasksApi.changes(t.id));
  } catch {
    setChanges(null);
  }
}

async function run<T>(f: () => Promise<T>): Promise<T | undefined> {
  setBusy(true);
  try {
    const r = await f();
    setError(null);
    return r;
  } catch (e) {
    setError(String(e));
    return undefined;
  } finally {
    setBusy(false);
  }
}

export const createTask = (request: Parameters<typeof agentTasksApi.create>[0]) =>
  run(async () => {
    const t = await agentTasksApi.create(request);
    setTasks((list) => (list.some((x) => x.id === t.id) ? list : [t, ...list]));
    setFormOpen(false);
    select(t.id);
    return t;
  });

export const cancelTask = (id: string) => run(() => agentTasksApi.cancel(id));
export const approve = (id: string, step: number, allow: boolean) => run(() => agentTasksApi.approve(id, step, allow));

export const discardTask = (id: string) =>
  run(async () => {
    await agentTasksApi.discard(id);
    setChanges(null);
  });

export const removeTask = (id: string) =>
  run(async () => {
    await agentTasksApi.remove(id);
    setTasks((list) => list.filter((t) => t.id !== id));
    if (selectedId() === id) select(tasks()[0]?.id ?? null);
  });

export async function applyTask(id: string, mode: ApplyMode, paths?: string[]): Promise<void> {
  const r = await run(() => agentTasksApi.apply(id, mode, paths));
  if (!r) return;
  setApplyAnswer(r);
  const task = tasks().find((t) => t.id === id);
  if (r.applied) {
    logNotification({
      group: "Agent Tasks",
      type: "info",
      title: r.created ? t("tools.agentTasks.notify.appliedCreated", { created: r.created }) : t("tools.agentTasks.notify.applied"),
      content: t("tools.agentTasks.notify.content", { title: task?.title ?? id, count: r.files.length }),
    });
  }
  await refreshChanges();
}

/** Show a changed file in the diff viewer: base commit on the left, the agent's version right. */
export async function showChange(id: string, path: string): Promise<void> {
  const d = await run(() => agentTasksApi.fileDiff(id, path));
  if (!d) return;
  setDiffRequest({
    path,
    left: { title: d.before == null ? "(new file)" : "Base", content: d.before ?? "" },
    right: { title: d.after == null ? "(deleted)" : "Agent's version", content: d.after ?? "" },
    editable: false,
  });
}
