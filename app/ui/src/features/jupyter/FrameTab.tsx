/** View as DataFrame: a variable's rows in an editor tab (PyCharm's data view). */
import { Sheet } from "lucide-solid";
import { jupyterApi } from "@/ipc/jupyter";
import { openCustomTab } from "@/state/editor";
import { DataFrameView } from "./DataFrameView";
import { showJupyterBalloon, type NotebookController } from "./store";

export async function openFrameTab(ctl: NotebookController, expr: string): Promise<void> {
  const id = ctl.state.kernel.id;
  if (!id) return;
  try {
    const frame = await jupyterApi.frame(id, expr);
    const name = ctl.path.split("/").pop() ?? ctl.path;
    openCustomTab({
      id: `forge:dataframe/${ctl.path}#${expr}`,
      title: expr,
      tooltip: `${expr} (${name})`,
      icon: (p) => <Sheet size={p.size ?? 14} />,
      component: () => (
        <div class="nb-frame-tab">
          <DataFrameView frame={frame} fill title={expr} />
        </div>
      ),
    });
  } catch (e) {
    showJupyterBalloon(`Cannot view ${expr} as a DataFrame`, String(e).replace(/^Error: /, ""), 8000, undefined, { type: "error" });
  }
}
