/** Read-only, syntax-highlighted SQL of the object dialogs' previews. */
import { createEffect } from "solid-js";
import { highlightCode, tagHighlighter, tags as t } from "@lezer/highlight";
import { MSSQL, MySQL, PLSQL, PostgreSQL, SQLite, StandardSQL, type SQLDialect } from "@codemirror/lang-sql";
import type { Dialect } from "@/ipc/db";

const classes = tagHighlighter([
  { tag: [t.keyword, t.typeName, t.modifier, t.operatorKeyword], class: "tok-keyword" },
  { tag: [t.string, t.special(t.string)], class: "tok-string" },
  { tag: [t.number, t.bool, t.null], class: "tok-number" },
  { tag: [t.comment, t.lineComment, t.blockComment], class: "tok-comment" },
  { tag: [t.special(t.name), t.name], class: "tok-variable" },
]);

const DIALECTS: Partial<Record<Dialect, SQLDialect>> = { mysql: MySQL, postgres: PostgreSQL, sqlite: SQLite, sqlserver: MSSQL, oracle: PLSQL, clickhouse: MySQL };

export function SqlPreview(props: { sql: string; dialect: Dialect; placeholder?: string; testId?: string }) {
  let pre!: HTMLPreElement;
  createEffect(() => {
    const sql = props.sql;
    pre.textContent = "";
    if (!sql) {
      const span = document.createElement("span");
      span.className = "muted";
      span.textContent = props.placeholder ?? "";
      pre.append(span);
      return;
    }
    const lang = (DIALECTS[props.dialect] ?? StandardSQL).language;
    highlightCode(
      sql,
      lang.parser.parse(sql),
      classes,
      (text, cls) => {
        if (!cls) return void pre.append(document.createTextNode(text));
        const span = document.createElement("span");
        span.className = cls;
        span.textContent = text;
        pre.append(span);
      },
      () => pre.append(document.createTextNode("\n")),
    );
  });
  return <pre ref={pre} class="dbo-sql" data-testid={props.testId} />;
}
