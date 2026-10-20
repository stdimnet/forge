/**
 * Template delimiters per (template language, data language) pair, like Smarty's
 * `left_delimiter` or Twig's `tag_variable` / `tag_block` / `tag_comment` lexer options.
 *
 * Each configurable template language declares its delimiter fields with their built-in
 * defaults. The `templates.delimiters` setting holds only the differences, one entry per pair:
 *
 *   { template = "smarty", language = "latex", left = "<<", right = ">>" }
 *   { template = "twig", language = "latex", variableStart = "\VAR{", variableEnd = "}", … }
 *
 * Nothing here knows about a particular pair: the LaTeX entries ship as default data in the
 * manifest (`app/src-tauri/plugins/core.toml`).
 */
import type { SettingObject } from "@/ipc/settings";
import { getSetting } from "@/state/settings";
import { dataLanguageFor } from "./templateData";
import { goTemplateSyntaxWith, jinjaSyntaxWith, smartySyntaxWith, twigSyntaxWith, type TemplateSyntax } from "./templates";

/** The setting with the delimiter entries. */
export const DELIMITERS_KEY = "templates.delimiters";

/** One delimiter of a template language, e.g. Twig's variable start `{{`. */
export interface DelimiterField {
  key: string;
  label: string;
  default: string;
}

/** Delimiter values by field key. */
export type DelimiterValues = Record<string, string>;

/** A template language whose delimiters can be changed. */
export interface ConfigurableTemplate {
  /** Language id, as used in the entries' `template` field. */
  id: string;
  label: string;
  /** Pairs of fields: start and end of each tag kind, in display order. */
  fields: DelimiterField[];
  syntax: (values: DelimiterValues) => TemplateSyntax;
}

/** A `templates.delimiters` entry: the pair plus the delimiter fields that differ. */
export type DelimiterEntry = SettingObject & { template: string; language: string };

const pair = (kind: string, label: string, start: string, end: string): DelimiterField[] => [
  { key: `${kind}Start`, label: `${label} start`, default: start },
  { key: `${kind}End`, label: `${label} end`, default: end },
];

/** Twig and Jinja: `tag_variable`, `tag_block` and `tag_comment` (Jinja's `*_start_string`). */
const twigFields = [...pair("variable", "Variable", "{{", "}}"), ...pair("block", "Block", "{%", "%}"), ...pair("comment", "Comment", "{#", "#}")];

const leftRight = (left: string, right: string): DelimiterField[] => [
  { key: "left", label: "Left delimiter", default: left },
  { key: "right", label: "Right delimiter", default: right },
];

export const CONFIGURABLE_TEMPLATES: Record<string, ConfigurableTemplate> = {
  smarty: { id: "smarty", label: "Smarty", fields: leftRight("{", "}"), syntax: (v) => smartySyntaxWith(v.left, v.right) },
  twig: { id: "twig", label: "Twig", fields: twigFields, syntax: (v) => twigSyntaxWith(v) },
  jinja: { id: "jinja", label: "Jinja2", fields: twigFields, syntax: (v) => jinjaSyntaxWith(v) },
  gotemplate: { id: "gotemplate", label: "Go Template", fields: leftRight("{{", "}}"), syntax: (v) => goTemplateSyntaxWith(v.left, v.right) },
};

/** The delimiters of `template` for files in `dataLanguage`: the defaults, overridden by the matching entry. */
export function resolveDelimiters(template: ConfigurableTemplate, entries: readonly SettingObject[], dataLanguage: string): DelimiterValues {
  const entry = entries.find((e) => e.template === template.id && e.language?.toLowerCase() === dataLanguage.toLowerCase());
  return Object.fromEntries(template.fields.map((f) => [f.key, entry?.[f.key]?.trim() || f.default]));
}

/** Delimiters of a file from the settings. Reactive: reading it in an effect tracks the settings. */
export function delimitersFor(template: ConfigurableTemplate, path: string): DelimiterValues {
  return resolveDelimiters(template, getSetting<SettingObject[]>(DELIMITERS_KEY, []), dataLanguageFor(path).id);
}

export const templateSyntaxFor = (template: ConfigurableTemplate, path: string): TemplateSyntax =>
  template.syntax(delimitersFor(template, path));

/** The entries without a template or data language, or that repeat an earlier pair. */
export function invalidEntries(entries: readonly SettingObject[]): number[] {
  const seen = new Set<string>();
  const bad: number[] = [];
  entries.forEach((e, i) => {
    const key = `${e.template}\u0000${e.language}`;
    if (!e.template || !e.language || seen.has(key)) bad.push(i);
    seen.add(key);
  });
  return bad;
}
