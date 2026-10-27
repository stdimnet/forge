/**
 * JavaForge Groovy IPC (app/src-tauri/src/commands/groovy.rs; crate packs/javaforge/lang-groovy):
 * the Groovy SDK and runnable scripts. Browser mode answers from `mock/jvmExtrasMock.ts`.
 */
import { invoke } from "./index";

export interface GroovyStatus {
  groovy: string | null;
  /** `setting`, `GROOVY_HOME`, `SDKMAN!` or `PATH`. */
  source: string | null;
  version: string | null;
  /** The Gradle build applies the `groovy` plugin. */
  gradleGroovy: boolean;
  /** How scripts run with the current settings: `groovy` or `gradle`. */
  runner: string;
}

export interface GroovyMain {
  class: string;
  name: string;
  line: number;
}

/** What can run in a Groovy file (`javaforge_lang_groovy::Runnable`). */
export interface GroovyRunnable {
  script: boolean;
  mains: GroovyMain[];
}

export const groovyApi = {
  status: () => invoke<GroovyStatus>("groovy_status"),
  runnable: (text: string) => invoke<GroovyRunnable>("groovy_runnable", { text }),
};
