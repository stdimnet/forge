import { File, FileCode, FileJson, FileText, Folder, FolderOpen, Database, Settings2 } from "lucide-solid";
import { Dynamic } from "solid-js/web";

const byExt: Record<string, { icon: typeof File; color: string }> = {
  php: { icon: FileCode, color: "#8f8cf0" },
  json: { icon: FileJson, color: "#e5b449" },
  md: { icon: FileText, color: "#7cc5e8" },
  sql: { icon: Database, color: "#6fd2c4" },
  env: { icon: Settings2, color: "#a0a0aa" },
  js: { icon: FileCode, color: "#e8d44d" },
  ts: { icon: FileCode, color: "#4f8fe0" },
  lua: { icon: FileCode, color: "#5a6ee0" },
  yml: { icon: Settings2, color: "#e07070" },
  yaml: { icon: Settings2, color: "#e07070" },
};

export function FileIcon(props: { name: string; dir?: boolean; open?: boolean; size?: number }) {
  const spec = () => {
    if (props.dir) return { icon: props.open ? FolderOpen : Folder, color: "var(--fg-muted)" };
    if (props.name.endsWith(".blade.php")) return { icon: FileCode, color: "#ff7a50" };
    const ext = props.name.startsWith(".") ? props.name.slice(1).split(".")[0] : props.name.split(".").pop()!;
    return byExt[ext.toLowerCase()] ?? { icon: File, color: "var(--fg-muted)" };
  };
  return (
    <Dynamic
      component={spec().icon}
      size={props.size ?? 15}
      stroke-width={1.75}
      style={{ color: spec().color, "flex-shrink": 0 }}
    />
  );
}
