import {
  Clapperboard,
  Container,
  EyeOff,
  File,
  FileCode,
  FileJson,
  FileTerminal,
  FileText,
  Folder,
  FolderDot,
  FolderOpen,
  FolderOpenDot,
  Database,
  Lock,
  Mail,
  Mailbox,
  Package,
  Settings2,
} from "lucide-solid";
import { Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { folderColor, iconStyle, themedIconKey, type IconKind } from "@/features/iconThemes/state";

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
  groovy: { icon: FileCode, color: "#5ea9c9" },
  gvy: { icon: FileCode, color: "#5ea9c9" },
  gy: { icon: FileCode, color: "#5ea9c9" },
  scala: { icon: FileCode, color: "#e0524f" },
  sc: { icon: FileCode, color: "#e0524f" },
  sbt: { icon: Settings2, color: "#e0524f" },
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
  djhtml: { icon: FileCode, color: "#44b78b" },
  j2: { icon: FileCode, color: "#d05050" },
  jinja: { icon: FileCode, color: "#d05050" },
  jinja2: { icon: FileCode, color: "#d05050" },
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
  rb: { icon: FileCode, color: "#e0524f" },
  rake: { icon: FileCode, color: "#e0524f" },
  gemspec: { icon: Package, color: "#e0524f" },
  ru: { icon: FileCode, color: "#e0524f" },
  erb: { icon: FileCode, color: "#e0524f" },
  cs: { icon: FileCode, color: "#9b6fd8" },
  csx: { icon: FileCode, color: "#9b6fd8" },
  fs: { icon: FileCode, color: "#4fa8d8" },
  fsx: { icon: FileCode, color: "#4fa8d8" },
  vb: { icon: FileCode, color: "#6f8fd8" },
  razor: { icon: FileCode, color: "#9b6fd8" },
  cshtml: { icon: FileCode, color: "#9b6fd8" },
  csproj: { icon: Package, color: "#9b6fd8" },
  fsproj: { icon: Package, color: "#4fa8d8" },
  vbproj: { icon: Package, color: "#6f8fd8" },
  sln: { icon: Package, color: "#b088e8" },
  slnx: { icon: Package, color: "#b088e8" },
  // Unity and Godot.
  shader: { icon: FileCode, color: "#d7a64a" },
  hlsl: { icon: FileCode, color: "#d7a64a" },
  cginc: { icon: FileCode, color: "#d7a64a" },
  compute: { icon: FileCode, color: "#d7a64a" },
  asmdef: { icon: Package, color: "#9b6fd8" },
  asmref: { icon: Package, color: "#9b6fd8" },
  uss: { icon: FileCode, color: "#5aa0e0" },
  uxml: { icon: FileCode, color: "#e0a060" },
  gd: { icon: FileCode, color: "#478cbf" },
  gdshader: { icon: FileCode, color: "#d7a64a" },
  tscn: { icon: Clapperboard, color: "#478cbf" },
  tres: { icon: Settings2, color: "#478cbf" },
  godot: { icon: Settings2, color: "#478cbf" },
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
  // Bundler and Rake.
  if (name === "Gemfile" || name === "gems.rb") return { icon: Package, color: "#e0524f" };
  if (name === "Gemfile.lock" || name === "gems.locked") return { icon: Lock, color: "#e0524f" };
  if (/^(?:Rakefile|rakefile|Guardfile|Capfile|\.irbrc|\.pryrc)$/.test(name)) return { icon: FileCode, color: "#e0524f" };
  // Cargo manifests and lock files.
  if (name === "Cargo.toml") return { icon: Package, color: "#dea584" };
  if (name === "Cargo.lock") return { icon: Lock, color: "#dea584" };
  if (/^\.env(?:\..+)?$/.test(name)) return byExt.env;
  const ext = name.startsWith(".") ? name.slice(1).split(".")[0] : name.split(".").pop()!;
  return byExt[ext.toLowerCase()] ?? { icon: File, color: "var(--fg-muted)" };
}

/**
 * The icon of a file or folder: the active icon theme's (`features/iconThemes`), else
 * Forge's own. `path` (when known) lets theme patterns and path rules match on folders too;
 * `name` is used otherwise. `root` marks the project root (the theme's root folder icon),
 * `hollow` a folder that holds open files (its hollow variant, with the option on). Folder
 * colors (`fileIcons.folderColors`) apply to both themed and built-in folder icons.
 */
export function FileIcon(props: {
  name: string;
  path?: string;
  dir?: boolean;
  open?: boolean;
  size?: number;
  dimmed?: boolean;
  root?: boolean;
  hollow?: boolean;
}) {
  const kind = (): IconKind => (props.dir ? (props.open ? "folderOpen" : "folder") : "file");
  const themed = () => themedIconKey(props.path || props.name, kind(), { root: props.root, hollow: props.hollow });
  const color = () => (props.dir && !props.root ? folderColor(props.path || props.name) : null);
  const spec = (): Spec =>
    props.dir
      ? {
          icon: props.hollow ? (props.open ? FolderOpenDot : FolderDot) : props.open ? FolderOpen : Folder,
          color: color() ?? "var(--fg-muted)",
        }
      : specFor(props.name);
  return (
    <Show
      when={themed()}
      fallback={
        <Dynamic
          component={spec().icon}
          class="file-icon"
          size={props.size ?? 15}
          stroke-width={1.75}
          aria-hidden="true"
          style={{ color: spec().color, "flex-shrink": 0, opacity: props.dimmed ? 0.6 : undefined }}
        />
      }
    >
      {(key) => (
        <span
          class="file-icon themed-file-icon"
          aria-hidden="true"
          style={{ ...iconStyle(key(), kind(), props.size ?? 15, color()), ...(props.dimmed ? { opacity: "0.6" } : {}) }}
        />
      )}
    </Show>
  );
}
