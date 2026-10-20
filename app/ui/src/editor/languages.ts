import type { Extension } from "@codemirror/state";
import { StreamLanguage, type StreamParser } from "@codemirror/language";
import { php } from "@codemirror/lang-php";
import { json } from "@codemirror/lang-json";
import { yaml } from "@codemirror/lang-yaml";
import { sql, MySQL } from "@codemirror/lang-sql";
import { markdown } from "@codemirror/lang-markdown";
import { javascript } from "@codemirror/lang-javascript";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { xml } from "@codemirror/lang-xml";
import { properties } from "@codemirror/legacy-modes/mode/properties";
import { dotenv } from "./dotenv";
import { streamLanguage } from "./tokens";
import { jinjaSyntax, latteSyntax, mustacheSyntax, templateParser, twigSyntax, type TemplateSyntax } from "./templates";
import { dataLanguageFor, dataLanguageSupport } from "./templateData";
import { CONFIGURABLE_TEMPLATES, templateSyntaxFor } from "./templateDelimiters";
import { templateOverlay } from "./templateOverlay";
import { templateEditing } from "./templateEditing";
import { less } from "@codemirror/lang-less";
import { sass } from "@codemirror/lang-sass";
import { vue } from "@codemirror/lang-vue";
import { java } from "@codemirror/lang-java";
import { cpp } from "@codemirror/lang-cpp";
import { python as pythonLang } from "@codemirror/lang-python";
import { rust as rustLang } from "@codemirror/lang-rust";
import { go as goLang } from "@codemirror/lang-go";
import { c, csharp, kotlin, scala, dart, objectiveC } from "@codemirror/legacy-modes/mode/clike";
import { ruby } from "@codemirror/legacy-modes/mode/ruby";
import { perl } from "@codemirror/legacy-modes/mode/perl";
import { swift } from "@codemirror/legacy-modes/mode/swift";
import { powerShell } from "@codemirror/legacy-modes/mode/powershell";
import { groovy } from "@codemirror/legacy-modes/mode/groovy";
import { haskell } from "@codemirror/legacy-modes/mode/haskell";
import { erlang } from "@codemirror/legacy-modes/mode/erlang";
import { clojure } from "@codemirror/legacy-modes/mode/clojure";
import { r } from "@codemirror/legacy-modes/mode/r";
import { julia } from "@codemirror/legacy-modes/mode/julia";
import { elm } from "@codemirror/legacy-modes/mode/elm";
import { crystal } from "@codemirror/legacy-modes/mode/crystal";
import { coffeeScript } from "@codemirror/legacy-modes/mode/coffeescript";
import { stylus } from "@codemirror/legacy-modes/mode/stylus";
import { pug } from "@codemirror/legacy-modes/mode/pug";
import { protobuf } from "@codemirror/legacy-modes/mode/protobuf";
import { cmake } from "@codemirror/legacy-modes/mode/cmake";
import { gherkin } from "@codemirror/legacy-modes/mode/gherkin";
import { http } from "@codemirror/legacy-modes/mode/http";
import { puppet } from "@codemirror/legacy-modes/mode/puppet";
import { tcl } from "@codemirror/legacy-modes/mode/tcl";
import { vb } from "@codemirror/legacy-modes/mode/vb";
import { pascal } from "@codemirror/legacy-modes/mode/pascal";
import { fortran } from "@codemirror/legacy-modes/mode/fortran";
import { verilog } from "@codemirror/legacy-modes/mode/verilog";
import { vhdl } from "@codemirror/legacy-modes/mode/vhdl";
import { octave } from "@codemirror/legacy-modes/mode/octave";
import { commonLisp } from "@codemirror/legacy-modes/mode/commonlisp";
import { scheme } from "@codemirror/legacy-modes/mode/scheme";
import { velocity } from "@codemirror/legacy-modes/mode/velocity";
import { stex } from "@codemirror/legacy-modes/mode/stex";
import { textile } from "@codemirror/legacy-modes/mode/textile";
import { gas } from "@codemirror/legacy-modes/mode/gas";
import { oCaml, fSharp } from "@codemirror/legacy-modes/mode/mllike";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { dockerFile } from "@codemirror/legacy-modes/mode/dockerfile";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { nginx } from "@codemirror/legacy-modes/mode/nginx";
import { lua } from "@codemirror/legacy-modes/mode/lua";
import { diff } from "@codemirror/legacy-modes/mode/diff";

/**
 * Syntax modes, matched by file name. Packs can add or override languages with
 * `registerLanguage`; later registrations win, so e.g. a Blade or Twig mode replaces
 * the generic HTML one for its files.
 */
export interface LanguageDef {
  id: string;
  /** Name shown in the status bar, as in PHPStorm ("PHP", "YAML", "Dotenv"). */
  label: string;
  /** Lower-case extensions without dot (`yml`), matched against the end of the name. */
  extensions?: string[];
  /** Exact lower-case file names (`dockerfile`) or name prefixes ending in `*` (`.env*`). */
  fileNames?: string[];
  load: (path: string) => Extension;
  /** Status bar label for a concrete file, e.g. "Twig (LaTeX)". Defaults to `label`. */
  labelFor?: (path: string) => string;
  /** Detects extensionless scripts from their first line (`#!/usr/bin/env python3`). */
  shebang?: RegExp;
  /** Indentation inserted by Tab / auto-indent. Defaults to four spaces. */
  indent?: string;
}

export const DEFAULT_INDENT = "    ";

const stream = (parser: StreamParser<unknown>) => () => StreamLanguage.define(parser);

/**
 * A template language whose content is highlighted in the file's data language. `syntax` is
 * re-read when the settings it uses change (delimiters), so open editors follow them.
 */
function template(id: string, label: string, extensions: string[], syntax: (path: string) => TemplateSyntax): LanguageDef {
  return {
    id,
    label,
    extensions,
    load: (path) => {
      const data = dataLanguageFor(path);
      // A data language without a Lezer grammar (LaTeX, INI, Shell) is tokenized around the
      // tags, so a `%` or `#` inside a tag does not start a comment in the data language.
      const parser = data.support ? undefined : data.parser;
      const aroundTags = parser ? (s: TemplateSyntax) => StreamLanguage.define(templateParser(s, parser)) : undefined;
      return [aroundTags ? [] : dataLanguageSupport(data), templateOverlay(() => syntax(path), aroundTags), templateEditing()];
    },
    labelFor: (path) => {
      const data = dataLanguageFor(path);
      return data.id === "html" ? label : `${label} (${data.label})`;
    },
  };
}

const languages: LanguageDef[] = [
  { id: "php", label: "PHP", extensions: ["php", "phtml", "inc"], load: () => php() },
  { id: "blade", label: "Blade", extensions: ["blade.php"], load: () => php({ plain: false }) },
  { id: "json", label: "JSON", extensions: ["json", "jsonc", "json5", "lock", "webmanifest", "geojson", "har"], fileNames: [".prettierrc", ".babelrc", ".eslintrc", "composer.lock", ".php-cs-fixer.cache"], load: () => json() },
  { id: "yaml", label: "YAML", extensions: ["yml", "yaml", "neon"], load: () => yaml() },
  { id: "sql", label: "SQL", extensions: ["sql", "ddl"], load: () => sql({ dialect: MySQL }) },
  { id: "markdown", label: "Markdown", extensions: ["md", "markdown"], load: () => markdown() },
  { id: "javascript", label: "JavaScript", extensions: ["js", "mjs", "cjs", "es6"], load: () => javascript() },
  { id: "jsx", label: "JavaScript React", extensions: ["jsx"], load: () => javascript({ jsx: true }) },
  { id: "typescript", label: "TypeScript", extensions: ["ts", "mts", "cts"], load: () => javascript({ typescript: true }) },
  { id: "tsx", label: "TypeScript React", extensions: ["tsx"], load: () => javascript({ typescript: true, jsx: true }) },
  { id: "css", label: "CSS", extensions: ["css", "pcss", "postcss"], load: () => css() },
  { id: "scss", label: "SCSS", extensions: ["scss"], load: () => sass() },
  { id: "sass", label: "Sass", extensions: ["sass"], load: () => sass({ indented: true }) },
  { id: "less", label: "Less", extensions: ["less"], load: () => less() },
  { id: "stylus", label: "Stylus", extensions: ["styl", "stylus"], load: stream(stylus) },
  { id: "html", label: "HTML", extensions: ["html", "htm", "xhtml", "svelte", "astro"], load: () => html() },
  { id: "vue", label: "Vue", extensions: ["vue"], load: () => vue() },
  template("twig", "Twig", ["twig", "html.twig"], () => twigSyntax),
  template("smarty", "Smarty", ["tpl", "smarty"], (path) => templateSyntaxFor(CONFIGURABLE_TEMPLATES.smarty, path)),
  template("latte", "Latte", ["latte"], () => latteSyntax),
  template("gotemplate", "Go Template", ["tmpl", "gotmpl", "gohtml"], (path) => templateSyntaxFor(CONFIGURABLE_TEMPLATES.gotemplate, path)),
  template("jinja", "Jinja2", ["j2", "jinja", "jinja2", "njk", "nunjucks"], () => jinjaSyntax),
  template("handlebars", "Handlebars", ["hbs", "handlebars", "mustache"], () => mustacheSyntax),
  { id: "velocity", label: "Velocity", extensions: ["vm", "vtl"], load: stream(velocity) },
  { id: "pug", label: "Pug", extensions: ["pug", "jade"], load: stream(pug) },
  { id: "xml", label: "XML", extensions: ["xml", "xsd", "xsl", "svg", "dist", "xliff", "xlf"], load: () => xml() },
  {
    id: "dotenv",
    label: "Dotenv",
    extensions: ["env"],
    fileNames: [".env*", "*.env"],
    load: () => streamLanguage(dotenv),
  },
  { id: "properties", label: "Properties", extensions: ["properties", "ini", "cfg", "conf", "editorconfig"], fileNames: [".editorconfig", ".gitattributes"], load: stream(properties) },
  { id: "shell", label: "Shell Script", extensions: ["sh", "bash", "zsh"], fileNames: [".bashrc", ".zshrc", ".profile", "artisan.sh"], load: stream(shell) },
  { id: "dockerfile", label: "Dockerfile", extensions: ["dockerfile"], fileNames: ["dockerfile", "containerfile", "dockerfile.*"], load: stream(dockerFile) },
  { id: "toml", label: "TOML", extensions: ["toml"], load: stream(toml) },
  { id: "nginx", label: "Nginx", extensions: ["nginx"], fileNames: ["nginx.conf"], load: stream(nginx) },
  { id: "lua", label: "Lua", extensions: ["lua"], load: stream(lua) },
  { id: "diff", label: "Diff", extensions: ["diff", "patch"], load: stream(diff) },
  { id: "python", label: "Python", extensions: ["py", "pyw", "pyi"], load: () => pythonLang() },
  { id: "go", label: "Go", extensions: ["go"], load: () => goLang() },
  { id: "rust", label: "Rust", extensions: ["rs"], load: () => rustLang() },
  { id: "java", label: "Java", extensions: ["java"], load: () => java() },
  { id: "cpp", label: "C++", extensions: ["cpp", "cc", "cxx", "hpp", "hh", "hxx", "ino"], load: () => cpp() },
  { id: "c", label: "C", extensions: ["c", "h"], load: stream(c) },
  { id: "csharp", label: "C#", extensions: ["cs", "csx"], load: stream(csharp) },
  { id: "kotlin", label: "Kotlin", extensions: ["kt", "kts"], load: stream(kotlin) },
  { id: "scala", label: "Scala", extensions: ["scala", "sc"], load: stream(scala) },
  { id: "dart", label: "Dart", extensions: ["dart"], load: stream(dart) },
  { id: "objc", label: "Objective-C", extensions: ["m", "mm"], load: stream(objectiveC) },
  { id: "swift", label: "Swift", extensions: ["swift"], load: stream(swift) },
  { id: "ruby", label: "Ruby", extensions: ["rb", "rake", "gemspec", "ru"], fileNames: ["gemfile", "rakefile", "vagrantfile"], load: stream(ruby) },
  { id: "perl", label: "Perl", extensions: ["pl", "pm", "t"], load: stream(perl) },
  { id: "powershell", label: "PowerShell", extensions: ["ps1", "psm1", "psd1"], load: stream(powerShell) },
  { id: "batch", label: "Batch", extensions: ["bat", "cmd"], load: stream(properties) },
  { id: "groovy", label: "Groovy", extensions: ["groovy", "gradle", "gvy"], fileNames: ["jenkinsfile"], load: stream(groovy) },
  { id: "haskell", label: "Haskell", extensions: ["hs", "lhs"], load: stream(haskell) },
  { id: "erlang", label: "Erlang", extensions: ["erl", "hrl"], load: stream(erlang) },
  { id: "elixir", label: "Elixir", extensions: ["ex", "exs"], load: stream(ruby) },
  { id: "clojure", label: "Clojure", extensions: ["clj", "cljs", "cljc", "edn"], load: stream(clojure) },
  { id: "r", label: "R", extensions: ["r"], load: stream(r) },
  { id: "julia", label: "Julia", extensions: ["jl"], load: stream(julia) },
  { id: "elm", label: "Elm", extensions: ["elm"], load: stream(elm) },
  { id: "crystal", label: "Crystal", extensions: ["cr"], load: stream(crystal) },
  { id: "ocaml", label: "OCaml", extensions: ["ml", "mli"], load: stream(oCaml) },
  { id: "fsharp", label: "F#", extensions: ["fs", "fsi", "fsx"], load: stream(fSharp) },
  { id: "coffeescript", label: "CoffeeScript", extensions: ["coffee"], load: stream(coffeeScript) },
  { id: "lisp", label: "Common Lisp", extensions: ["lisp", "lsp", "cl"], load: stream(commonLisp) },
  { id: "scheme", label: "Scheme", extensions: ["scm", "ss", "rkt"], load: stream(scheme) },
  { id: "tcl", label: "Tcl", extensions: ["tcl"], load: stream(tcl) },
  { id: "vb", label: "Visual Basic", extensions: ["vb", "vbs"], load: stream(vb) },
  { id: "pascal", label: "Pascal", extensions: ["pas", "pp"], load: stream(pascal) },
  { id: "fortran", label: "Fortran", extensions: ["f", "f90", "f95", "for"], load: stream(fortran) },
  { id: "verilog", label: "Verilog", extensions: ["v", "sv", "svh"], load: stream(verilog) },
  { id: "vhdl", label: "VHDL", extensions: ["vhd", "vhdl"], load: stream(vhdl) },
  { id: "octave", label: "MATLAB/Octave", extensions: ["matlab", "oct"], load: stream(octave) },
  { id: "asm", label: "Assembly", extensions: ["asm", "s"], load: stream(gas) },
  { id: "latex", label: "LaTeX", extensions: ["tex", "sty", "cls", "bib"], load: stream(stex) },
  { id: "textile", label: "Textile", extensions: ["textile"], load: stream(textile) },
  { id: "protobuf", label: "Protocol Buffers", extensions: ["proto"], load: stream(protobuf) },
  { id: "cmake", label: "CMake", extensions: ["cmake"], fileNames: ["cmakelists.txt"], load: stream(cmake) },
  { id: "gherkin", label: "Gherkin", extensions: ["feature"], load: stream(gherkin) },
  { id: "http", label: "HTTP Request", extensions: ["http", "rest"], load: stream(http) },
  { id: "puppet", label: "Puppet", extensions: ["pp.puppet"], load: stream(puppet) },
  { id: "makefile", label: "Makefile", extensions: ["mk", "make"], fileNames: ["makefile", "gnumakefile"], load: stream(shell) },
  { id: "htaccess", label: "Apache Config", fileNames: [".htaccess", "httpd.conf", "apache2.conf"], load: stream(properties) },
  { id: "log", label: "Log", extensions: ["log"], load: stream(properties) },
  { id: "ignore", label: "Ignore File", fileNames: [".gitignore", ".dockerignore", ".npmignore"], load: stream(properties) },
];

export function registerLanguage(def: LanguageDef): void {
  const idx = languages.findIndex((l) => l.id === def.id);
  if (idx >= 0) languages.splice(idx, 1);
  languages.push(def);
}

/** `*` matches any run of characters; everything else is literal. */
export function wildcardMatch(pattern: string, text: string): boolean {
  const parts = pattern.split("*");
  if (!text.startsWith(parts[0])) return false;
  let tail = text.slice(parts[0].length);
  if (parts.length === 1) return tail === "";
  for (const part of parts.slice(1, -1)) {
    const i = tail.indexOf(part);
    if (i < 0) return false;
    tail = tail.slice(i + part.length);
  }
  return tail.endsWith(parts[parts.length - 1]);
}

/** How specifically `def` claims a file: higher wins, 0 means no match. */
function score(def: LanguageDef, path: string, name: string): number {
  let best = 0;
  for (const raw of def.fileNames ?? []) {
    const pattern = raw.toLowerCase();
    const isPath = pattern.includes("/");
    if (!(isPath ? wildcardMatch(`*/${pattern}`, `/${path}`) : wildcardMatch(pattern, name))) continue;
    // A path pattern beats a file name, an exact name beats a wildcard pattern.
    const literal = pattern.replaceAll("*", "").length;
    best = Math.max(best, 1000 + literal + (pattern.includes("*") ? 0 : 500) + (isPath ? 500 : 0));
  }
  for (const ext of def.extensions ?? []) {
    if (name.endsWith(`.${ext}`) || name === ext) best = Math.max(best, ext.length + 1);
  }
  return best;
}

/**
 * Language for a file: file names and path patterns first, then the longest matching
 * extension (so `x.blade.php` beats `php`), then the shebang of `firstLine`. Later
 * registrations win ties.
 */
export function detectLanguage(path: string, firstLine?: string): LanguageDef | undefined {
  const lower = path.toLowerCase();
  const name = lower.split("/").pop() ?? lower;
  let best: { def: LanguageDef; score: number } | undefined;
  for (const def of languages) {
    const s = score(def, lower, name);
    if (s > 0 && (!best || s >= best.score)) best = { def, score: s };
  }
  if (best) return best.def;
  if (firstLine?.startsWith("#!")) return [...languages].reverse().find((l) => l.shebang?.test(firstLine));
  return undefined;
}

const firstLineOf = (content?: string) => content?.slice(0, content.search(/\r?\n|$/));

export function languageFor(path: string, content?: string): Extension[] {
  const def = detectLanguage(path, firstLineOf(content));
  return def ? [def.load(path)] : [];
}

export function languageLabel(path: string, content?: string): string {
  const def = detectLanguage(path, firstLineOf(content));
  return def ? (def.labelFor?.(path) ?? def.label) : "Plain text";
}

export const languageIdFor = (path: string, content?: string) => detectLanguage(path, firstLineOf(content))?.id;

export const indentFor = (path: string, content?: string) =>
  detectLanguage(path, firstLineOf(content))?.indent ?? DEFAULT_INDENT;
