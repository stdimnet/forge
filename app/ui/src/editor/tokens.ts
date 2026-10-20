import { StreamLanguage, type StreamParser } from "@codemirror/language";
import { tags as t, type Tag } from "@lezer/highlight";

/**
 * Token names the stream parsers return, mapped to highlight tags. Keeping one vocabulary
 * means every pack is colored by the same rules in `forgeHighlight`.
 */
export const tokenTable: Record<string, Tag | readonly Tag[]> = {
  keyword: t.keyword,
  control: t.controlKeyword,
  builtin: t.standard(t.variableName),
  variable: t.special(t.variableName),
  name: t.variableName,
  def: t.definition(t.variableName),
  func: t.function(t.definition(t.variableName)),
  property: t.propertyName,
  type: t.typeName,
  namespace: t.namespace,
  string: t.string,
  special: t.special(t.string),
  escape: t.escape,
  number: t.number,
  atom: t.atom,
  bool: t.bool,
  comment: t.lineComment,
  meta: t.meta,
  operator: t.operator,
  punct: t.punctuation,
  bracket: t.bracket,
  option: t.attributeName,
  label: t.labelName,
  heading: t.heading,
  link: t.link,
  url: t.url,
  regexp: t.regexp,
  invalid: t.invalid,
  inserted: t.inserted,
  deleted: t.deleted,
};

/** Define a stream language that uses the shared token vocabulary. */
export function streamLanguage<S>(parser: StreamParser<S>, extra: Partial<StreamParser<S>> = {}): StreamLanguage<S> {
  return StreamLanguage.define<S>({ ...parser, tokenTable, ...extra });
}
