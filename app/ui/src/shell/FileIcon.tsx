import {
  Container,
  EyeOff,
  File,
  FileCode,
  FileJson,
  FileTerminal,
  FileText,
  Folder,
  FolderOpen,
  Database,
  Lock,
  Mail,
  Mailbox,
  Package,
  Settings2,
} from "lucide-solid";
import { Dynamic } from "solid-js/web";

type Spec = { icon: typeof File; color: string };

const byExt: Record<string, Spec> = {
  php: { icon: FileCode, color: "#8f8cf0" },
  json: { icon: FileJson, color: "#e5b449" },
  jsonc: { icon: FileJson, color: "#e5b449" },
  md: { icon: FileText, color: "#7cc5e8" },
  sql: { icon: Database, color: "#6fd2c4" },
  env: { icon: Settings2, color: "#a0a0aa" },
  js: { icon: FileCode, color: "#e8d44d" },
  ts: { icon: FileCode, color: "#4f8fe0" },
  lua: { icon: FileCode, color: "#5a6ee0" },
  kt: { icon: FileCode, color: "#a97bff" },
  kts: { icon: FileCode, color: "#a97bff" },
  yml: { icon: Settings2, color: "#e07070" },
  yaml: { icon: Settings2, color: "#e07070" },
  neon: { icon: Settings2, color: "#e07070" },
  toml: { icon: Settings2, color: "#c9a26b" },
  ini: { icon: Settings2, color: "#a0a0aa" },
  conf: { icon: Settings2, color: "#a0a0aa" },
  cfg: { icon: Settings2, color: "#a0a0aa" },
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
  pyw: { icon: FileCode, color: "#4f8fe0" },
  pyx: { icon: FileCode, color: "#4f8fe0" },
  pxd: { icon: FileCode, color: "#4f8fe0" },
  pyi: { icon: FileCode, color: "#6aa5e8" },
  go: { icon: FileCode, color: "#4fc3e0" },
  rs: { icon: FileCode, color: "#d08050" },
  css: { icon: FileCode, color: "#5aa0e0" },
  scss: { icon: FileCode, color: "#d070a0" },
  sh: { icon: FileTerminal, color: "#73c48a" },
  bash: { icon: FileTerminal, color: "#73c48a" },
  zsh: { icon: FileTerminal, color: "#73c48a" },
  lock: { icon: Lock, color: "#a0a0aa" },
  cf: { icon: Mailbox, color: "#d9a05a" },
  eml: { icon: Mail, color: "#6fb3e0" },
  dockerfile: { icon: Container, color: "#4fa3e0" },
};

// PHP under other extensions (Drupal's hook files).
for (const ext of ["inc", "phtml", "module", "install", "theme", "profile", "engine"]) byExt[ext] = byExt.php;

const ignoreFile = /^\.(?:\w+)?ignore$|^\.gitignore_global$/;
const composeFile = /^(?:docker-compose|compose)(?:\..+)?\.ya?ml$/i;
const dockerfile = /^(?:Dockerfile|Containerfile)(?:\..+)?$/i;
// Python dependency files (pip, Pipenv, Poetry, uv).
const pythonDeps = /^(?:(?:requirements|constraints)[\w.-]*\.(?:txt|in)|Pipfile|Pipfile\.lock|poetry\.lock|uv\.lock)$/;
const shellDotfile = /^\.(?:bashrc|bash_profile|bash_login|bash_logout|bash_aliases|profile|zshrc|zprofile|envrc)$/;

function specFor(name: string): Spec {
  if (name.endsWith(".blade.php")) return { icon: FileCode, color: "#ff7a50" };
  if (ignoreFile.test(name)) return { icon: EyeOff, color: "#a0a0aa" };
  if (composeFile.test(name) || dockerfile.test(name)) return byExt.dockerfile;
  if (shellDotfile.test(name)) return byExt.sh;
  if (pythonDeps.test(name)) return { icon: Package, color: "#e5b449" };
  // Go modules and workspaces.
  if (/^go\.(?:mod|work)$/.test(name)) return { icon: Package, color: "#4fc3e0" };
  if (/^go\.(?:work\.)?sum$/.test(name)) return { icon: Lock, color: "#4fc3e0" };
  if (/^\.env(?:\..+)?$/.test(name)) return byExt.env;
  const ext = name.startsWith(".") ? name.slice(1).split(".")[0] : name.split(".").pop()!;
  return byExt[ext.toLowerCase()] ?? { icon: File, color: "var(--fg-muted)" };
}

export function FileIcon(props: { name: string; dir?: boolean; open?: boolean; size?: number; dimmed?: boolean }) {
  const spec = (): Spec =>
    props.dir ? { icon: props.open ? FolderOpen : Folder, color: "var(--fg-muted)" } : specFor(props.name);
  return (
    <Dynamic
      component={spec().icon}
      class="file-icon"
      size={props.size ?? 15}
      stroke-width={1.75}
      style={{ color: spec().color, "flex-shrink": 0, opacity: props.dimmed ? 0.6 : undefined }}
    />
  );
}
