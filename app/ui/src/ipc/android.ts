/**
 * JavaForge Android IPC (app/src-tauri/src/commands/android.rs; crate packs/javaforge/android):
 * the SDK, devices and emulators, and Logcat. Browser mode answers from
 * `mock/androidMock.ts` (the demo app, `?project=android`).
 */
import { invoke, listen } from "./index";

export interface SdkLocation {
  path: string;
  /** `setting`, `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `local.properties` or `default`. */
  source: string;
}

export interface AndroidModuleInfo {
  dir: string;
  gradlePath: string;
  kind: "application" | "library";
  namespace?: string;
  applicationId?: string;
  minSdk?: string;
  targetSdk?: string;
  compileSdk?: string;
  kotlin: boolean;
}

export interface AndroidStatus {
  sdk: SdkLocation | null;
  adb: string | null;
  emulator: string | null;
  hint: string | null;
  modules: AndroidModuleInfo[];
  remote: boolean;
}

export interface Device {
  serial: string;
  /** `device` (online), `offline`, `unauthorized`, … */
  state: string;
  model: string | null;
  product: string | null;
  device: string | null;
  transportId: string | null;
  usb: string | null;
  emulator: boolean;
  avd: string | null;
}

export interface Avd {
  name: string;
  /** Serial of the running emulator. */
  serial: string | null;
}

export interface DevicesSnapshot {
  devices: Device[];
  avds: Avd[];
  adb: string | null;
  emulator: string | null;
  hint: string | null;
  error: string | null;
}

export type LogLevel = "V" | "D" | "I" | "W" | "E" | "A";

export interface FrameLink {
  path: string;
  line: number;
  /** Byte range of `File.kt:42` in the message. */
  start: number;
  end: number;
}

export interface LogEntry {
  id: number;
  date: string;
  time: string;
  pid: number;
  tid: number;
  level: LogLevel;
  tag: string;
  message: string;
  package?: string;
  stack: boolean;
  link?: FrameLink;
}

export interface LogcatView {
  entries: LogEntry[];
  total: number;
  matched: number;
  packages: string[];
}

export interface LogcatBatch {
  serial: string;
  entries: LogEntry[];
  cleared?: boolean;
  ended?: boolean;
}

export const androidApi = {
  status: () => invoke<AndroidStatus>("android_status"),
  devices: () => invoke<DevicesSnapshot>("android_devices"),
  startEmulator: (avd: string) => invoke<void>("android_emulator_start", { avd }),
  logcatStart: (serial: string, query?: string) => invoke<void>("android_logcat_start", { serial, query }),
  logcatStop: () => invoke<void>("android_logcat_stop"),
  logcatView: (query: string, limit?: number) => invoke<LogcatView>("android_logcat_view", { query, limit }),
  logcatClear: () => invoke<void>("android_logcat_clear"),
  onLogcat: (cb: (b: LogcatBatch) => void) => listen<LogcatBatch>("android://logcat", cb),
};
