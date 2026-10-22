/**
 * External Tools as stored in the user setting `tools.externalTools` (an `objects` list with
 * string values, read in Rust by `ExternalTool::from_object`): this codec turns them into
 * `ExternalTool`s for the UI. Expansion, validation and filters are computed in Rust.
 */
import type { SettingValue } from "@/ipc/settings";
import type { ExternalTool } from "@/ipc/runTools";
import { getSetting } from "@/state/settings";

export const TOOLS_KEY = "tools.externalTools";
export const DEFAULT_GROUP = "External Tools";

type ToolObject = Record<string, string>;

const flag = (v: string | undefined, fallback: boolean) => (v === "true" ? true : v === "false" ? false : fallback);

export function decodeTool(o: ToolObject): ExternalTool {
  return {
    name: o.name ?? "",
    group: o.group ?? "",
    description: o.description ?? "",
    program: o.program ?? "",
    arguments: o.arguments ?? "",
    workingDirectory: o.workingDirectory ?? "",
    synchronize: flag(o.synchronize, true),
    openConsole: flag(o.openConsole, true),
    activateOnStdout: flag(o.activateOnStdout, false),
    activateOnStderr: flag(o.activateOnStderr, false),
    outputFilters: (o.outputFilters ?? "")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean),
    enabled: flag(o.enabled, true),
  };
}

export function encodeTool(t: ExternalTool): ToolObject {
  return {
    name: t.name.trim(),
    group: t.group.trim() || DEFAULT_GROUP,
    description: t.description,
    program: t.program,
    arguments: t.arguments,
    workingDirectory: t.workingDirectory,
    synchronize: String(t.synchronize),
    openConsole: String(t.openConsole),
    activateOnStdout: String(t.activateOnStdout),
    activateOnStderr: String(t.activateOnStderr),
    outputFilters: t.outputFilters.join("\n"),
    enabled: String(t.enabled),
  };
}

export const decodeTools = (value: SettingValue | undefined): ExternalTool[] =>
  Array.isArray(value) ? (value as unknown as ToolObject[]).filter((o) => o && typeof o === "object").map(decodeTool) : [];

export const encodeTools = (tools: ExternalTool[]): SettingValue => tools.map(encodeTool) as unknown as SettingValue;

export const newTool = (group = DEFAULT_GROUP): ExternalTool => ({
  name: "",
  group,
  description: "",
  program: "",
  arguments: "",
  workingDirectory: "$ProjectFileDir$",
  synchronize: true,
  openConsole: true,
  activateOnStdout: false,
  activateOnStderr: false,
  outputFilters: [],
  enabled: true,
});

export const groupName = (t: ExternalTool) => t.group.trim() || DEFAULT_GROUP;

/** `Group/Name`: how "Run External tool" tasks refer to a tool. */
export const toolRef = (t: ExternalTool) => `${groupName(t)}/${t.name.trim()}`;

/** PHPStorm's keymap action id of a tool: `Tool_<group>_<name>`. */
export const toolActionId = (t: ExternalTool) => `Tool_${groupName(t)}_${t.name.trim()}`;

/** All tools in settings order. */
export const externalTools = (): ExternalTool[] => decodeTools(getSetting(TOOLS_KEY, []));

/** Enabled tools by group, groups in order of their first tool. */
export function toolGroups(tools: ExternalTool[] = externalTools()): { name: string; tools: ExternalTool[] }[] {
  const out: { name: string; tools: ExternalTool[] }[] = [];
  for (const t of tools) {
    if (!t.enabled || !t.name.trim()) continue;
    const g = out.find((x) => x.name === groupName(t));
    if (g) g.tools.push(t);
    else out.push({ name: groupName(t), tools: [t] });
  }
  return out;
}

export const findTool = (ref: string, tools: ExternalTool[] = externalTools()) =>
  tools.find((t) => toolRef(t) === ref.trim()) ?? tools.find((t) => t.name.trim() === ref.trim());
