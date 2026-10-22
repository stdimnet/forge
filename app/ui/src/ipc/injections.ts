/**
 * Language injections computed in Rust (`crates/forge-injections`, `commands/injections.rs`):
 * SQL, HTML, CSS, JSON, RegExp, JavaScript and XML inside PHP and JavaScript/TypeScript
 * strings. Offsets are UTF-16, like CodeMirror's. The `*Sync` variants run the same Rust code
 * as WebAssembly for the editor, which needs the answer at once while typing; they return
 * `undefined` while the module is still loading (callers then use the async command).
 */
import { invoke } from "./index";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";

export type LiteralKind = "phpSingle" | "phpDouble" | "phpHeredoc" | "phpNowdoc" | "jsSingle" | "jsDouble" | "jsTemplate";

export interface Span {
  from: number;
  to: number;
}

export interface InjectedLiteral {
  from: number;
  to: number;
  contentFrom: number;
  contentTo: number;
  kind: LiteralKind;
}

export interface Injection {
  /** Editor language id: `sql`, `html`, `css`, `json`, `regexp`, `javascript`, `xml`. */
  language: string;
  /** Display name: `SQL`, `RegExp`. */
  label: string;
  source: "comment" | "rule";
  /** Id of the built-in rule that injected it. */
  rule?: string | null;
  from: number;
  to: number;
  /** The injected text, without quotes, delimiters and interpolations. */
  ranges: Span[];
  literals: InjectedLiteral[];
  /** The `// language=` comment. */
  comment?: Span | null;
  /** Identifies the place for Un-inject Language. */
  key: string;
  /** The value of the literals, unescaped and joined (what Run SQL sends). */
  text: string;
}

export interface Fragment {
  language: string;
  label: string;
  kind: LiteralKind;
  /** The source range the fragment stands for. */
  from: number;
  to: number;
  text: string;
  /** For each UTF-16 unit of `text` and its end: the source offset. */
  map: number[];
}

export interface InjectionEdit {
  from: number;
  to: number;
  text: string;
}

export interface InjectionRule {
  id: string;
  name: string;
  language: string;
  languageLabel: string;
  host: "php" | "js";
  hostLabel: string;
  places: string;
}

export interface InjectableLanguage {
  id: string;
  label: string;
}

/** The `editor.injections.*` settings that requests carry. */
export interface InjectionSettings {
  disabled: string[];
  uninjected: string[];
  /** Parameters marked `#[Language]` (from the PHP index). */
  places?: { name: string; argument: number; method: boolean; language: string }[];
}

export interface FindRequest extends InjectionSettings {
  text: string;
  path: string;
}

export interface OffsetRequest extends FindRequest {
  offset: number;
}

export interface InjectRequest {
  text: string;
  path: string;
  offset: number;
  /** Language id or name. */
  language: string;
}

let wasm: ForgeWasm | null = null;
void forgeWasm().then(
  (w) => (wasm = w),
  () => undefined,
);

export const injectionsApi = {
  find: (request: FindRequest) => invoke<Injection[]>("injections_find", { request }),
  at: (request: OffsetRequest) => invoke<Injection | null>("injections_at", { request }),
  fragment: (request: OffsetRequest) => invoke<Fragment | null>("injections_fragment", { request }),
  escape: (kind: LiteralKind, text: string) => invoke<string>("injections_escape", { kind, text }),
  inject: (request: InjectRequest) => invoke<InjectionEdit | null>("injections_inject", { request }),
  rules: () => invoke<InjectionRule[]>("injections_rules"),
  languages: () => invoke<InjectableLanguage[]>("injections_languages"),

  findSync(request: FindRequest): Injection[] | undefined {
    return wasm ? callWasmSync<Injection[]>(wasm.injections_find, request) : undefined;
  },
  fragmentSync(request: OffsetRequest): Fragment | null | undefined {
    return wasm ? callWasmSync<Fragment | null>(wasm.injections_fragment, request) : undefined;
  },
  escapeSync(kind: LiteralKind, text: string): string | undefined {
    return wasm ? callWasmSync<string>(wasm.injections_escape, { kind, text }) : undefined;
  },
};
