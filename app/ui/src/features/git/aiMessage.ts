/**
 * "Generate Commit Message with AI": builds the prompt from the diff of the selected
 * changes and calls the shared `ai_complete` command (see docs/ARCHITECTURE.md).
 */
import { invoke } from "@/ipc";

export const MAX_DIFF_CHARS = 14000;
const MAX_FILE_CHARS = 4000;

export const COMMIT_SYSTEM_PROMPT = [
  "You write git commit messages.",
  "Rules:",
  "- First line: a concise summary in the imperative mood (\"Add\", \"Fix\", \"Refactor\"), at most 72 characters, no trailing period.",
  "- If the change needs explanation, add a blank line and a short body wrapped at 72 characters explaining what and why (not how).",
  "- Do not mention file names unless they are essential. No markdown, no code fences, no quotes.",
  "- Reply with the commit message only.",
].join("\n");

/** Shorten a multi-file unified diff so every file keeps a share of the budget. */
export function truncateDiff(diff: string, max = MAX_DIFF_CHARS): string {
  if (diff.length <= max) return diff;
  const files = diff.split(/(?=^diff --git )/m);
  const perFile = Math.max(400, Math.min(MAX_FILE_CHARS, Math.floor(max / Math.max(1, files.length))));
  let out = "";
  for (const f of files) {
    const part = f.length > perFile ? `${f.slice(0, perFile)}\n… (${f.length - perFile} more characters of this file omitted)\n` : f;
    if (out.length + part.length > max) {
      out += `\n… (${files.length - files.indexOf(f)} more files omitted)\n`;
      break;
    }
    out += part;
  }
  return out;
}

export interface PromptFile {
  path: string;
  status: string;
}

export function buildCommitPrompt(diff: string, files: PromptFile[], branch?: string | null): string {
  const list = files.map((f) => `${f.status.padEnd(9)} ${f.path}`).join("\n");
  return [
    branch ? `Current branch: ${branch}` : "",
    `Changed files:\n${list || "(none)"}`,
    `Diff:\n${truncateDiff(diff) || "(no textual diff)"}`,
    "Write the commit message for these changes.",
  ]
    .filter(Boolean)
    .join("\n\n");
}

/** Strip code fences, quotes and over-long subjects from a model reply. */
export function cleanCommitMessage(raw: string): string {
  let text = raw.trim();
  const fence = text.match(/^```[a-z]*\n([\s\S]*?)\n?```$/i);
  if (fence) text = fence[1].trim();
  text = text.replace(/^(commit message:)\s*/i, "");
  if (/^["'`].*["'`]$/s.test(text)) text = text.slice(1, -1).trim();
  const [subject, ...body] = text.split("\n");
  let s = subject.trim().replace(/\.$/, "");
  if (s.length > 72) s = s.slice(0, 72).replace(/\s+\S*$/, "");
  const rest = body.join("\n").trim();
  return rest ? `${s}\n\n${rest}` : s;
}

export async function generateCommitMessage(diff: string, files: PromptFile[], branch?: string | null): Promise<string> {
  const raw = await invoke<string>("ai_complete", {
    request: {
      system: COMMIT_SYSTEM_PROMPT,
      messages: [{ role: "user", content: buildCommitPrompt(diff, files, branch) }],
      maxTokens: 300,
    },
  });
  return cleanCommitMessage(raw);
}
