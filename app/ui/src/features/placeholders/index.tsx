/**
 * Tool windows whose features are not implemented yet. Each real feature module replaces
 * its placeholder by registering a tool window with the same id.
 */
import { Bug, Database, GitCommitHorizontal, GitPullRequest, Layers, ListTree, Play, Sparkles, SquareTerminal, TriangleAlert, Container } from "lucide-solid";
import { registerToolWindow, type ToolWindow } from "@/features/registry";

const placeholder = (text: string) => () => <div class="placeholder-panel">{text}</div>;

const items: Omit<ToolWindow, "component">[] = [
  { id: "commit", title: "Commit", icon: GitCommitHorizontal, anchor: "left", order: 10, shortcut: "Alt+0" },
  { id: "pull-requests", title: "Pull Requests", icon: GitPullRequest, anchor: "left", order: 20 },
  { id: "structure", title: "Structure", icon: ListTree, anchor: "left", order: 30, shortcut: "Alt+7" },
  { id: "run", title: "Run", icon: Play, anchor: "bottom", order: 10, shortcut: "Alt+4" },
  { id: "debug", title: "Debug", icon: Bug, anchor: "bottom", order: 20, shortcut: "Alt+5" },
  { id: "services", title: "Services", icon: Container, anchor: "bottom", order: 30, shortcut: "Alt+8" },
  { id: "terminal", title: "Terminal", icon: SquareTerminal, anchor: "bottom", order: 40, shortcut: "Alt+F12" },
  { id: "problems", title: "Problems", icon: TriangleAlert, anchor: "bottom", order: 50, shortcut: "Alt+6" },
  { id: "git", title: "Git", icon: Layers, anchor: "bottom", order: 60, shortcut: "Alt+9" },
  { id: "ai-chat", title: "AI Chat", icon: Sparkles, anchor: "right", order: 0 },
  { id: "database", title: "Database", icon: Database, anchor: "right", order: 10 },
];

for (const item of items) registerToolWindow({ ...item, placeholder: true, component: placeholder(`${item.title} is coming soon.`) });
