import { describe, expect, it } from "vitest";
import type { KernelEvent, NotebookDiff, NotebookView } from "@/ipc/jupyter";

history.replaceState(null, "", "/?project=python");
const { invoke, listen } = await import("@/ipc");
const { salesNotebook, salesNotebookHead } = await import("./demoNotebooks");
await import("./jupyterMock");

describe("Jupyter mock (pyforge-jupyter through WebAssembly)", () => {
  it("opens, saves and diffs the demo notebook", async () => {
    const view = await invoke<NotebookView>("jupyter_open", { text: salesNotebook });
    expect(view.notebook.cells.map((c) => c.cellType)).toEqual(["markdown", "code", "code", "markdown", "code", "code", "code"]);
    expect(view.rendered[2][0].kind).toBe("dataFrame");
    expect(view.rendered[4][0].kind).toBe("image");
    expect(await invoke<string>("jupyter_serialize", { notebook: view.notebook, original: salesNotebook })).toBe(salesNotebook);
    const diff = await invoke<NotebookDiff>("jupyter_diff", { left: salesNotebookHead, right: salesNotebook });
    expect(diff.stats).toEqual({ added: 1, removed: 0, modified: 3, unchanged: 3 });
  });

  it("runs cells on the simulated kernel", async () => {
    const { id } = await invoke<{ id: string }>("jupyter_kernel_start", { request: { path: "notebooks/sales.ipynb" } });
    const events: KernelEvent[] = [];
    await listen<KernelEvent>(`jupyter://${id}`, (e) => events.push(e));
    await invoke("jupyter_execute", { id, cellId: "a", code: 'print("hi")' });
    await new Promise((r) => setTimeout(r, 600));
    expect(events.filter((e) => e.type !== "status").map((e) => e.type)).toEqual(["executeInput", "output", "done"]);
    const out = events.find((e) => e.type === "output");
    expect(out && out.type === "output" && out.output).toEqual({ outputType: "stream", name: "stdout", text: "hi\n" });
    expect(events.at(-1)).toEqual({ type: "status", state: "idle" });
  });
});
