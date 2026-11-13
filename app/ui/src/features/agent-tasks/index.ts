/**
 * Agent Tasks: AI agents (Forge's own tool-calling agent, Claude Code or Gemini CLI) work on a
 * task in a sandbox, run the tests and fix failures, and leave a diff to review and apply
 * (`crates/forge-agent-tasks`).
 */
import { ListChecks } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { AgentTasksActions, AgentTasksPanel } from "./AgentTasksPanel";
import { setFormOpen } from "./store";
import { t } from "@/i18n";

registerToolWindow({
  id: "agent-tasks",
  title: t("toolwindow.agent-tasks.title"),
  icon: ListChecks,
  anchor: "right",
  order: 1,
  component: AgentTasksPanel,
  actions: AgentTasksActions,
});

registerCommand({
  id: "AgentTasks.New",
  title: "New Agent Task…",
  category: "AI",
  run: () => {
    showToolWindow("agent-tasks");
    setFormOpen(true);
  },
});

registerCommand({
  id: "AgentTasks.Show",
  title: "Show Agent Tasks",
  category: "AI",
  run: () => showToolWindow("agent-tasks"),
});
