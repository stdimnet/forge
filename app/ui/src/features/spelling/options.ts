/** The `spelling.*` and `grammar.*` settings as the Rust side wants them. */
import type { SaveToData, SpellOptions } from "@/ipc/spell";
import { getSetting, saveSetting } from "@/state/settings";
import { isPluginActive } from "@/state/project";

export const PLUGIN_ID = "forge.natural-languages";
export const PROJECT_WORDS = "spelling.projectWords";
export const APPLICATION_WORDS = "spelling.applicationWords";

/** Grammar rule settings and the `forge-spell` rule ids they turn off. */
const RULE_SETTINGS: [string, string][] = [
  ["grammar.rules.repeatedWord", "repeated-word"],
  ["grammar.rules.articles", "a-an"],
  ["grammar.rules.sentenceStart", "sentence-start"],
  ["grammar.rules.doubleSpace", "double-space"],
];

export function spellOptions(): SpellOptions {
  return {
    spelling: getSetting<boolean>("spelling.enabled", true),
    grammar: getSetting<boolean>("grammar.enabled", true),
    comments: getSetting<boolean>("spelling.checkComments", true),
    strings: getSetting<boolean>("spelling.checkStrings", true),
    identifiers: getSetting<boolean>("spelling.checkIdentifiers", true),
    text: getSetting<boolean>("spelling.checkText", true),
    commitMessages: getSetting<boolean>("spelling.checkCommitMessages", true),
    fileNames: getSetting<boolean>("spelling.checkFileNames", false),
    disabledRules: RULE_SETTINGS.filter(([key]) => !getSetting<boolean>(key, true)).map(([, rule]) => rule),
  };
}

/** The project-level and application-level dictionary words. */
export function dictionaryWords(): string[] {
  return [...getSetting<string[]>(PROJECT_WORDS, []), ...getSetting<string[]>(APPLICATION_WORDS, [])];
}

/** Whether any inspection runs (the plugin can be disabled in Settings | Plugins). */
export function inspectionsOn(): boolean {
  const o = spellOptions();
  return isPluginActive(PLUGIN_ID) && (o.spelling || o.grammar);
}

/** The severity a problem is shown with (`spelling.severity`, `grammar.severity`). */
export function problemSeverity(kind: "typo" | "grammar"): string {
  return kind === "typo" ? getSetting<string>("spelling.severity", "typo") : getSetting<string>("grammar.severity", "grammar");
}

/** Everything a check depends on, for re-checking open editors when it changes. */
export function settingsFingerprint(): string {
  return JSON.stringify([
    inspectionsOn(),
    spellOptions(),
    dictionaryWords(),
    problemSeverity("typo"),
    problemSeverity("grammar"),
    getSetting<string[]>("spelling.customDictionaries", []),
  ]);
}

/** Add a word to the project-level or application-level dictionary. */
export async function saveWord({ word, level }: SaveToData): Promise<void> {
  const key = level === "project" ? PROJECT_WORDS : APPLICATION_WORDS;
  const words = getSetting<string[]>(key, []);
  if (words.some((w) => w.toLowerCase() === word.toLowerCase())) return;
  await saveSetting(key, [...words, word], level === "project" ? "project" : "user");
}
