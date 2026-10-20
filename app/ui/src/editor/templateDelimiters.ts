/**
 * Configurable template delimiters, like Smarty's `left_delimiter` / `right_delimiter` or Go's
 * `Template.Delims`. A template language declares its default delimiters; settings override
 * them for all files, per data language or per file pattern:
 *
 *   templates.<id>.leftDelimiter / rightDelimiter   "{" / "}" for every file
 *   templates.<id>.delimiterOverrides               ["latex=<< >>", "mail/*.tpl=[[ ]]"]
 *
 * An override entry is `<target>=<left> <right>`. The target is a data language id
 * (`latex`, `sql`, see `DATA_LANGUAGES`) or a file pattern (`legacy/pdf/*.tpl`). The most
 * specific entry wins: a matching file pattern, then the data language, then the plain
 * left/right delimiter settings, then the language's defaults. Nothing here knows about a
 * particular pair: the Smarty + LaTeX rule ships as data in the manifest's default settings.
 */
import { matchesGlobList } from "@/features/settings/globs";
import { getSetting } from "@/state/settings";
import { DATA_LANGUAGES, dataLanguageFor } from "./templateData";
import { goTemplateSyntaxWith, smartySyntaxWith, type TemplateSyntax } from "./templates";

export interface Delimiters {
  left: string;
  right: string;
}

export interface DelimiterRule extends Delimiters {
  /** Data language id or file pattern. */
  target: string;
}

/** A template language whose delimiters can be changed. */
export interface ConfigurableTemplate {
  /** Language id, also the settings prefix: `templates.<id>.leftDelimiter`. */
  id: string;
  defaults: Delimiters;
  /** The template syntax for a pair of delimiters. */
  syntax: (delimiters: Delimiters) => TemplateSyntax;
}

export const CONFIGURABLE_TEMPLATES: Record<string, ConfigurableTemplate> = {
  smarty: { id: "smarty", defaults: { left: "{", right: "}" }, syntax: (d) => smartySyntaxWith(d.left, d.right) },
  gotemplate: { id: "gotemplate", defaults: { left: "{{", right: "}}" }, syntax: (d) => goTemplateSyntaxWith(d.left, d.right) },
};

export const delimiterSettingKeys = (id: string) => ({
  left: `templates.${id}.leftDelimiter`,
  right: `templates.${id}.rightDelimiter`,
  overrides: `templates.${id}.delimiterOverrides`,
});

/** `latex=<< >>` → `{ target: "latex", left: "<<", right: ">>" }`; malformed entries give `undefined`. */
export function parseDelimiterRule(entry: string): DelimiterRule | undefined {
  // Targets never contain `=`, delimiters may (`{=`), so split at the first one.
  const eq = entry.indexOf("=");
  if (eq <= 0) return undefined;
  const target = entry.slice(0, eq).trim();
  const parts = entry.slice(eq + 1).trim().split(/\s+/);
  if (!target || parts.length !== 2 || !parts[0] || !parts[1]) return undefined;
  return { target, left: parts[0], right: parts[1] };
}

const isDataLanguage = (target: string) => DATA_LANGUAGES.some((l) => l.id === target.toLowerCase());

export interface DelimiterInput {
  /** `templates.<id>.leftDelimiter` / `rightDelimiter`; empty means the default. */
  left?: string;
  right?: string;
  overrides?: string[];
}

/** The delimiters of the file at `path`, whose data language is `dataLanguage`. */
export function resolveDelimiters(defaults: Delimiters, settings: DelimiterInput, path: string, dataLanguage: string): Delimiters {
  const rules = (settings.overrides ?? []).map(parseDelimiterRule).filter((r): r is DelimiterRule => r != null);
  const byPattern = rules.find((r) => !isDataLanguage(r.target) && matchesGlobList(path, r.target));
  if (byPattern) return { left: byPattern.left, right: byPattern.right };
  const byLanguage = rules.find((r) => r.target.toLowerCase() === dataLanguage.toLowerCase());
  if (byLanguage) return { left: byLanguage.left, right: byLanguage.right };
  return { left: settings.left?.trim() || defaults.left, right: settings.right?.trim() || defaults.right };
}

/** Delimiters of a file from the settings. Reactive: reading it in an effect tracks the settings. */
export function delimitersFor(template: ConfigurableTemplate, path: string): Delimiters {
  const keys = delimiterSettingKeys(template.id);
  const settings: DelimiterInput = {
    left: getSetting<string>(keys.left, ""),
    right: getSetting<string>(keys.right, ""),
    overrides: getSetting<string[]>(keys.overrides, []),
  };
  return resolveDelimiters(template.defaults, settings, path, dataLanguageFor(path).id);
}

export const templateSyntaxFor = (template: ConfigurableTemplate, path: string): TemplateSyntax =>
  template.syntax(delimitersFor(template, path));
