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
import { gherkin } from "./gherkin";
import { streamLanguage } from "./tokens";
import { djangoSyntax, latteSyntax, mustacheSyntax, templateParser, type TemplateSyntax } from "./templates";
import { templateOverlay } from "./templateOverlay";
import { templateEditing } from "./templateEditing";
import { dataLanguage, dataLanguageSupport } from "./templateData";
import { liveTemplateSyntax } from "./templateDelimiters";
import { fileTypeOf, type FileTypeInfo } from "./fileTypes";
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
import { customFileTypeLanguage } from "./customFileType";

/**
 * Syntax modes by file type id. Which file type a file has (names, extensions, shebangs,
 * template data languages, labels, indentation) is decided in Rust
 * (`crates/forge-filetypes`, fed by the plugin manifests' `fileTypes`); this module only
 * maps the id to CodeMirror support. Packs add or replace modes with `registerLanguage`.
 */
export interface LanguageDef {
  /** File type id, as in the manifests (`yaml`, `docker-compose`). */
  id: string;
  load: (path: string, info: FileTypeInfo) => Extension;
}

export const DEFAULT_INDENT = "    ";

const stream = (parser: StreamParser<unknown>) => () => StreamLanguage.define(parser);

/**
 * A template language: the data language's highlighting with the template tags on top.
 * Without `syntax` the delimiters come from Rust (`FileTypeInfo.delimiters`, the
 * `templates.delimiters` setting) and open editors follow when they change.
 */
const template =
  (syntax?: TemplateSyntax) =>
  (path: string, info: FileTypeInfo): Extension => {
    const live = syntax ? undefined : liveTemplateSyntax(info.id ?? "", path, info);
    const initial = syntax ?? live?.();
    if (!initial) return dataLanguageSupport(info.dataLanguage?.id);
    const resolve = live ? () => live() ?? initial : initial;
    // A data language without a Lezer grammar (LaTeX, INI, Shell) is tokenized around the
    // tags, so a `%` or `#` inside a tag does not start a comment in the data language.
    const data = dataLanguage(info.dataLanguage?.id);
    const parser = data.support ? undefined : data.parser;
    const aroundTags = parser ? (s: TemplateSyntax) => StreamLanguage.define(templateParser(s, parser)) : undefined;
    return [aroundTags ? [] : dataLanguageSupport(data.id), templateOverlay(resolve, aroundTags), templateEditing()];
  };

const loaders = new Map<string, LanguageDef["load"]>(
  Object.entries({
    php: () => php(),
    blade: () => php({ plain: false }),
    json: () => json(),
    yaml: () => yaml(),
    sql: () => sql({ dialect: MySQL }),
    markdown: () => markdown(),
    javascript: () => javascript(),
    jsx: () => javascript({ jsx: true }),
    typescript: () => javascript({ typescript: true }),
    tsx: () => javascript({ typescript: true, jsx: true }),
    css: () => css(),
    scss: () => sass(),
    sass: () => sass({ indented: true }),
    less: () => less(),
    stylus: stream(stylus),
    html: () => html(),
    vue: () => vue(),
    twig: template(),
    smarty: template(),
    latte: template(latteSyntax),
    gotemplate: template(),
    jinja: template(),
    django: template(djangoSyntax),
    handlebars: template(mustacheSyntax),
    velocity: stream(velocity),
    pug: stream(pug),
    xml: () => xml(),
    dotenv: () => streamLanguage(dotenv),
    properties: stream(properties),
    shell: stream(shell),
    dockerfile: stream(dockerFile),
    toml: stream(toml),
    nginx: stream(nginx),
    lua: stream(lua),
    diff: stream(diff),
    python: () => pythonLang(),
    go: () => goLang(),
    rust: () => rustLang(),
    java: () => java(),
    cpp: () => cpp(),
    c: stream(c),
    csharp: stream(csharp),
    kotlin: stream(kotlin),
    scala: stream(scala),
    dart: stream(dart),
    objc: stream(objectiveC),
    swift: stream(swift),
    ruby: stream(ruby),
    perl: stream(perl),
    powershell: stream(powerShell),
    batch: stream(properties),
    groovy: stream(groovy),
    haskell: stream(haskell),
    erlang: stream(erlang),
    elixir: stream(ruby),
    clojure: stream(clojure),
    r: stream(r),
    julia: stream(julia),
    elm: stream(elm),
    crystal: stream(crystal),
    ocaml: stream(oCaml),
    fsharp: stream(fSharp),
    coffeescript: stream(coffeeScript),
    lisp: stream(commonLisp),
    scheme: stream(scheme),
    tcl: stream(tcl),
    vb: stream(vb),
    pascal: stream(pascal),
    fortran: stream(fortran),
    verilog: stream(verilog),
    vhdl: stream(vhdl),
    octave: stream(octave),
    asm: stream(gas),
    latex: stream(stex),
    textile: stream(textile),
    protobuf: stream(protobuf),
    cmake: stream(cmake),
    gherkin: () => streamLanguage(gherkin),
    http: stream(http),
    puppet: stream(puppet),
    makefile: stream(shell),
    htaccess: stream(properties),
    log: stream(properties),
    ignore: stream(properties),
  } satisfies Record<string, LanguageDef["load"]>),
);

/** Register (or replace) the syntax mode of a file type. */
export function registerLanguage(def: LanguageDef): void {
  loaders.set(def.id, def.load);
}

/** The editor extensions for a file's language (empty for plain text). */
export function languageFor(path: string, content?: string): Extension[] {
  const info = fileTypeOf(path, content);
  // Custom file types (Settings | Editor | File Types) bring their syntax from Rust.
  if (info.custom) return [customFileTypeLanguage(info.custom)];
  const load = info.id ? loaders.get(info.id) : undefined;
  return load ? [load(path, info)] : [];
}

/** Status bar label, e.g. "PHP", "Twig (LaTeX)" or "Plain text". */
export const languageLabel = (path: string, content?: string): string => fileTypeOf(path, content).label;

export const languageIdFor = (path: string, content?: string): string | undefined =>
  fileTypeOf(path, content).id ?? undefined;

export const indentFor = (path: string, content?: string): string => fileTypeOf(path, content).indent || DEFAULT_INDENT;

/** Whether the file's lines wrap softly (Settings | Editor | General | Soft Wraps). */
export const softWrapFor = (path: string): boolean => fileTypeOf(path).softWrap;
