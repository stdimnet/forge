import { Container, File, FileCode, FileJson, FileTerminal, FileText, Folder, FolderOpen, Database, Lock, Settings2 } from "lucide-solid";
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
  neon: { icon: Settings2, color: "#e07070" },
  toml: { icon: Settings2, color: "#c9a26b" },
  ini: { icon: Settings2, color: "#a0a0aa" },
  xml: { icon: FileCode, color: "#e8a050" },
  html: { icon: FileCode, color: "#e8734a" },
  twig: { icon: FileCode, color: "#9ccc65" },
  tmpl: { icon: FileCode, color: "#4fc3e0" },
  gotmpl: { icon: FileCode, color: "#4fc3e0" },
  gohtml: { icon: FileCode, color: "#4fc3e0" },
  tpl: { icon: FileCode, color: "#e0c050" },
  latte: { icon: FileCode, color: "#f08050" },
  hbs: { icon: FileCode, color: "#e8a040" },
  vue: { icon: FileCode, color: "#41b883" },
  tsx: { icon: FileCode, color: "#4f8fe0" },
  jsx: { icon: FileCode, color: "#e8d44d" },
  py: { icon: FileCode, color: "#4f8fe0" },
  go: { icon: FileCode, color: "#4fc3e0" },
  rs: { icon: FileCode, color: "#d08050" },
  css: { icon: FileCode, color: "#5aa0e0" },
  scss: { icon: FileCode, color: "#d070a0" },
  sh: { icon: FileTerminal, color: "#73c48a" },
  lock: { icon: Lock, color: "#a0a0aa" },
  dockerfile: { icon: Container, color: "#4fa3e0" },
};

export function FileIcon(props: { name: string; dir?: boolean; open?: boolean; size?: number }) {
  const spec = () => {
    if (props.dir) return { icon: props.open ? FolderOpen : Folder, color: "var(--fg-muted)" };
    if (props.name.endsWith(".blade.php")) return { icon: FileCode, color: "#ff7a50" };
    const lower = props.name.toLowerCase();
    if (lower === "dockerfile" || lower.startsWith("dockerfile.") || lower.startsWith("docker-compose") || lower.startsWith("compose.y"))
      return byExt.dockerfile;
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
