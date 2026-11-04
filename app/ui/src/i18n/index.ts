/**
 * Translatable UI text. `t("toolwindow.project.title")` returns the message of a key in the
 * IDE language (Settings | Appearance & Behavior | System Settings | Language), falling back
 * to English. Messages use ICU MessageFormat and are looked up and formatted by Rust
 * (`crates/forge-i18n`, through WebAssembly), synchronously:
 *
 * ```ts
 * t("vcs.commit.button")                                  // "Commit"
 * t("vcs.commit.files", { count: 3 })                     // "3 files"
 * <T k="toolwindow.project.title" />                      // reactive JSX text
 * ```
 *
 * `t()` reads the `locale` signal, so text rendered in a tracked scope (JSX, memos) follows a
 * language change without a reload. Text computed once (e.g. a title passed to
 * `registerToolWindow` at startup) changes with the next start, which is why the language
 * setting offers a restart. Catalogs live in `crates/forge-i18n/locales/<locale>/*.json`; see
 * docs/guide/developer/localization.md.
 */
import { createSignal, type JSX } from "solid-js";
import { forgeWasm, forgeWasmSyncForTests, type ForgeWasm } from "@/wasm";

/** Argument values of a message; dates are milliseconds since the epoch. */
export type MessageArgs = Record<string, string | number | boolean | null | undefined>;

export interface LocaleInfo {
  id: string;
  name: string;
  nativeName: string;
}

/** The setting that selects the IDE language. */
export const LANGUAGE_SETTING = "ide.language";

let wasm: ForgeWasm | null = null;
const [locale, setLocaleSignal] = createSignal("en");
/** Texts of argument-free keys in the current locale. */
let cache = new Map<string, string>();
const warned = new Set<string>();

/** The current IDE language (reactive), e.g. `en` or `de`. */
export { locale };

function engine(): ForgeWasm | null {
  // vitest: load the module synchronously on first use, so components render real text.
  wasm ??= forgeWasmSyncForTests();
  return wasm;
}

/** The text of `key` in the IDE language, formatted with `args`. */
export function t(key: string, args?: MessageArgs): string {
  locale(); // track the language
  const w = engine();
  if (!w) {
    if (!warned.has(key)) {
      warned.add(key);
      console.warn(`i18n: "${key}" requested before the catalogs were loaded`);
    }
    return key;
  }
  if (!args) {
    let text = cache.get(key);
    if (text === undefined) {
      text = w.i18n_t(key);
      cache.set(key, text);
    }
    return text;
  }
  return w.i18n_t_args(key, JSON.stringify(args));
}

/**
 * `t(key, args)` when the key exists, else `fallback`: for keys built from data (a settings
 * group a plugin names, a value of an enum) that may have no message.
 */
export function tOr(key: string, fallback: string, args?: MessageArgs): string {
  return hasMessage(key) ? t(key, args) : fallback;
}

/** Format a message source that is not in the catalogs (e.g. from a plugin). */
export function formatMessage(message: string, args: MessageArgs = {}): string {
  locale();
  const w = engine();
  return w ? w.i18n_format(message, JSON.stringify(args)) : message;
}

/** Whether `key` exists in the catalogs (in the IDE language or in English). */
export function hasMessage(key: string): boolean {
  locale();
  return engine()?.i18n_has(key) ?? false;
}

/** Reactive text for JSX: `<T k="common.ok" />`, `<T k="vcs.files" args={{ count }} />`. */
export function T(props: { k: string; args?: MessageArgs }): JSX.Element {
  return (() => t(props.k, props.args)) as unknown as JSX.Element;
}

/** Languages with a bundled catalog. */
export function availableLocales(): LocaleInfo[] {
  const w = engine();
  return w ? (JSON.parse(w.i18n_locales()) as LocaleInfo[]) : [{ id: "en", name: "English", nativeName: "English" }];
}

/** Switch the language now. Everything rendered through `t()` in a tracked scope follows. */
export function setLocale(id: string): void {
  const w = engine();
  const applied = w ? w.i18n_set_locale(id || "en", -new Date().getTimezoneOffset()) : "en";
  cache = new Map();
  document.documentElement.lang = applied;
  setLocaleSignal(applied);
}

/**
 * Load the catalogs and select the language stored in the settings. The workbench awaits
 * this before its features register, so text computed at startup is already translated.
 */
export async function initI18n(readLanguage: () => Promise<string | null | undefined>): Promise<void> {
  try {
    wasm = await forgeWasm();
  } catch (e) {
    console.error("i18n: catalogs not loaded", e);
    return;
  }
  let language: string | null | undefined = null;
  try {
    language = await readLanguage();
  } catch (e) {
    console.warn("i18n: language setting not read", e);
  }
  setLocale(typeof language === "string" ? language : "en");
}
