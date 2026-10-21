/**
 * TODO tool window (PHPStorm: View | Tool Windows | TODO): `TODO` / `FIXME` comments of the
 * project (patterns in Settings | Editor | TODO), scanned in Rust through the project's
 * backend and updated live when files change on disk or in the editor.
 */
import { ListTodo } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { TodoTabs, TodoView } from "./TodoView";
import "./store";
import "./todo.css";

registerToolWindow({
  id: "todo",
  title: "TODO",
  icon: ListTodo,
  anchor: "bottom",
  order: 55,
  component: TodoView,
  headerContent: TodoTabs,
});
