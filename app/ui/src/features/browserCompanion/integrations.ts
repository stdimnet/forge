/**
 * Framework integrations shown on Settings | Tools | Browser Companion: how to make error
 * pages, dev overlays and debuggers open files in Forge. Relevant ones (detected from
 * composer.json / package.json) are listed first.
 */
import type { ProjectProfile } from "@/ipc/companion";

export const FORGE_URL_TEMPLATE = "forge://open?file=%path&line=%line";

export type SnippetLanguage = "dotenv" | "php" | "yaml" | "ini" | "shell" | "js";

export interface Snippet {
  /** File the snippet belongs in, e.g. `.env` or `config/packages/framework.yaml`. */
  file: string;
  language: SnippetLanguage;
  code: string;
}

/** A change Forge may make itself after the user confirmed: one dotenv variable. */
export interface EnvChange {
  file: ".env" | ".env.local";
  key: string;
  value: string;
}

export interface Integration {
  id: string;
  title: string;
  description: string;
  /** Package names (composer or npm) that make the integration relevant. */
  packages: string[];
  snippets: Snippet[];
  apply?: EnvChange;
  /** Shown under the snippets; `backticks` mark inline code. */
  note?: string;
}

export const INTEGRATIONS: Integration[] = [
  {
    id: "laravel",
    title: "Laravel error page (Ignition)",
    description: "File links in Ignition's stack traces open in Forge.",
    packages: ["laravel/framework", "spatie/laravel-ignition", "facade/ignition"],
    snippets: [
      { file: ".env", language: "dotenv", code: "IGNITION_EDITOR=forge" },
      {
        file: "config/ignition.php",
        language: "php",
        code: `'editor' => env('IGNITION_EDITOR', 'forge'),

'editor_options' => [
    'forge' => [
        'label' => 'Forge',
        'url' => '${FORGE_URL_TEMPLATE}',
    ],
],`,
      },
    ],
    apply: { file: ".env", key: "IGNITION_EDITOR", value: "forge" },
    note: "Publish the config with `php artisan vendor:publish --tag=ignition-config`. No editor links at all? Install Ignition with `composer require --dev spatie/laravel-ignition`.",
  },
  {
    id: "symfony",
    title: "Symfony error page and profiler",
    description: "Exception pages, the profiler and dump() link to Forge.",
    packages: ["symfony/framework-bundle", "symfony/symfony"],
    snippets: [
      { file: ".env.local", language: "dotenv", code: 'SYMFONY_IDE="forge://open?file=%f&line=%l"' },
      {
        file: "config/packages/framework.yaml",
        language: "yaml",
        code: "framework:\n    ide: 'forge://open?file=%%f&line=%%l'",
      },
    ],
    apply: { file: ".env.local", key: "SYMFONY_IDE", value: "forge://open?file=%f&line=%l" },
  },
  {
    id: "xdebug",
    title: "Xdebug",
    description: "File names in Xdebug's error output and var_dump() become links to Forge.",
    packages: ["php"],
    snippets: [{ file: "php.ini", language: "ini", code: 'xdebug.file_link_format = "forge://open?file=%f&line=%l"' }],
  },
  {
    id: "whoops",
    title: "Whoops",
    description: "The Whoops error page opens files in Forge.",
    packages: ["filp/whoops"],
    snippets: [
      {
        file: "bootstrap.php",
        language: "php",
        code: `$handler = new \\Whoops\\Handler\\PrettyPageHandler();
$handler->addEditor('forge', 'forge://open?file=%file&line=%line');
$handler->setEditor('forge');`,
      },
    ],
  },
  {
    id: "launch-editor",
    title: "Vite, Vue DevTools and dev overlays (launch-editor)",
    description:
      "\"Open in editor\" in Vue DevTools, the Svelte inspector, React and Vite error overlays runs launch-editor, which starts Forge's launcher.",
    packages: ["vite", "vue", "nuxt", "svelte", "@sveltejs/kit", "react-scripts", "next", "@vue/cli-service"],
    snippets: [
      { file: "~/.profile", language: "shell", code: "export LAUNCH_EDITOR=forge\n# Next.js reads REACT_EDITOR instead\nexport REACT_EDITOR=forge" },
    ],
    note: "The `forge` command must be on your PATH. It accepts `file:line:column`, `--line N --column N file` and `file line column`.",
  },
  {
    id: "jsx-source",
    title: "Exact element locations for React 19, Preact and Solid",
    description:
      "React 19 no longer exposes where an element was written. This dev-only Babel plugin adds data-source-file and data-source-line, which the extension reads.",
    packages: ["react", "preact", "solid-js"],
    snippets: [
      {
        file: "forge-jsx-source.js",
        language: "js",
        code: `// Adds data-source-file / data-source-line to DOM elements in development.
export default function forgeJsxSource({ types: t }) {
  return {
    visitor: {
      JSXOpeningElement(path, state) {
        const { name, loc } = path.node;
        if (!loc || name.type !== "JSXIdentifier" || !/^[a-z]/.test(name.name)) return;
        path.node.attributes.push(
          t.jsxAttribute(t.jsxIdentifier("data-source-file"), t.stringLiteral(state.filename)),
          t.jsxAttribute(t.jsxIdentifier("data-source-line"), t.stringLiteral(String(loc.start.line))),
        );
      },
    },
  };
}`,
      },
      {
        file: "vite.config.ts",
        language: "js",
        code: `react({
  babel: { plugins: process.env.NODE_ENV === "production" ? [] : ["./forge-jsx-source.js"] },
})`,
      },
    ],
  },
  {
    id: "angular",
    title: "Angular",
    description:
      "Nothing to configure: in development mode (ng serve) the extension asks Angular for the component under the cursor and Forge opens its class.",
    packages: ["@angular/core"],
    snippets: [],
  },
];

/** Package names of the project: composer + npm, plus "php" for PHP projects. */
function projectPackages(profile: ProjectProfile | null | undefined): Set<string> {
  const names = new Set([...(profile?.composer ?? []), ...(profile?.npm ?? [])]);
  if (profile?.composer.length) names.add("php");
  return names;
}

export function isDetected(integration: Integration, profile: ProjectProfile | null | undefined): boolean {
  const names = projectPackages(profile);
  return integration.packages.some((p) => names.has(p));
}

/** Detected integrations first (keeping their order), then the rest. */
export function rankIntegrations(
  profile: ProjectProfile | null | undefined,
  all: Integration[] = INTEGRATIONS,
): { integration: Integration; detected: boolean }[] {
  const rows = all.map((integration) => ({ integration, detected: isDetected(integration, profile) }));
  return [...rows.filter((r) => r.detected), ...rows.filter((r) => !r.detected)];
}

/** Whether the integration's dotenv change is already in place. */
export function isApplied(integration: Integration, profile: ProjectProfile | null | undefined): boolean {
  const change = integration.apply;
  return !!change && profile?.env[change.file]?.[change.key] === change.value;
}

/** Dotenv keys and config files the settings page asks the backend about. */
export const PROFILE_ENV_KEYS = [...new Set(INTEGRATIONS.flatMap((i) => (i.apply ? [i.apply.key] : [])))];
export const PROFILE_FILES = ["config/ignition.php", "config/packages/framework.yaml"];
