/**
 * Minimal markdown -> HTML for hover/documentation popups: ```php fences (with light
 * syntax colouring), inline code, **bold**, *italic*, bullet lists and paragraphs.
 * Everything is HTML-escaped first.
 */

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const PHP_KEYWORDS =
  /\b(abstract|class|interface|trait|enum|extends|implements|function|public|protected|private|static|final|readonly|const|case|new|return|use|namespace|fn|null|true|false|self|parent)\b/g;

/** Colour a line of PHP: strings, variables, attributes and keywords. */
export function highlightPhp(code: string): string {
  const out: string[] = [];
  const re = /('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")|(\$\w+)|(#\[|\])|([^'"$#\]]+|[#$])/g;
  for (const m of code.matchAll(re)) {
    if (m[1]) out.push(`<span class="md-str">${escape(m[1])}</span>`);
    else if (m[2]) out.push(`<span class="md-var">${escape(m[2])}</span>`);
    else if (m[3]) out.push(`<span class="md-attr">${escape(m[3])}</span>`);
    else out.push(escape(m[4]).replace(PHP_KEYWORDS, '<span class="md-kw">$1</span>'));
  }
  return out.join("");
}

function inline(s: string): string {
  return escape(s)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>");
}

export function renderMarkdown(md: string): string {
  const html: string[] = [];
  const lines = md.split("\n");
  let i = 0;
  let para: string[] = [];
  const flush = () => {
    if (para.length) html.push(`<p>${para.map(inline).join("<br>")}</p>`);
    para = [];
  };
  while (i < lines.length) {
    const line = lines[i];
    const fence = /^```(\w*)/.exec(line);
    if (fence) {
      flush();
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("```")) code.push(lines[i++]);
      i++;
      const body = fence[1] === "php" ? code.map(highlightPhp).join("\n") : escape(code.join("\n"));
      html.push(`<pre class="md-code">${body}</pre>`);
      continue;
    }
    if (/^- /.test(line)) {
      flush();
      const items: string[] = [];
      while (i < lines.length && /^- /.test(lines[i])) items.push(`<li>${inline(lines[i++].slice(2))}</li>`);
      html.push(`<ul>${items.join("")}</ul>`);
      continue;
    }
    if (!line.trim()) flush();
    else para.push(line);
    i++;
  }
  flush();
  return html.join("");
}
