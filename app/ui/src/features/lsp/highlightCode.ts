/** PHP syntax colors for code in documentation popups (Lezer, like the editor). */
import { highlightCode, tagHighlighter, tags as t } from "@lezer/highlight";
import { phpLanguage } from "@codemirror/lang-php";

const tokenClasses = tagHighlighter([
  { tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.definitionKeyword], class: "tok-keyword" },
  { tag: [t.string, t.special(t.string)], class: "tok-string" },
  { tag: [t.number, t.bool, t.null, t.atom], class: "tok-number" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], class: "tok-comment" },
  { tag: [t.variableName, t.special(t.variableName)], class: "tok-variable" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], class: "tok-function" },
  { tag: [t.className, t.typeName, t.namespace], class: "tok-type" },
  { tag: [t.propertyName, t.attributeName], class: "tok-property" },
  { tag: [t.operator, t.punctuation, t.bracket], class: "tok-operator" },
]);

let phpProgramParser: ReturnType<typeof phpLanguage.parser.configure> | undefined;

/** Append syntax-highlighted code (PHP; other languages as plain text) to `parent`. */
export function highlightInto(parent: HTMLElement, code: string, lang = "php"): void {
  if (!/^(php|)$/i.test(lang)) {
    parent.append(document.createTextNode(code));
    return;
  }
  phpProgramParser ??= phpLanguage.parser.configure({ top: "Program" });
  const tree = phpProgramParser.parse(code);
  highlightCode(
    code,
    tree,
    tokenClasses,
    (text, classes) => {
      if (!classes) {
        parent.append(document.createTextNode(text));
        return;
      }
      const span = document.createElement("span");
      span.className = classes;
      span.textContent = text;
      parent.append(span);
    },
    () => parent.append(document.createTextNode("\n")),
  );
}
