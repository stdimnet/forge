/**
 * Dockerfile: highlighting (CodeMirror's dockerfile mode with embedded shell), completion
 * of instructions, flags, stages and build arguments, hover documentation, and the checks
 * `docker build` fails on (unknown instruction, missing FROM).
 */
import { StreamLanguage } from "@codemirror/language";
import { dockerFile } from "@codemirror/legacy-modes/mode/dockerfile";
import type { EditorState } from "@codemirror/state";
import type { Completion, CompletionContext } from "@codemirror/autocomplete";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor, registerLanguage } from "@/editor/languages";
import { docDom, hoverDocs, indentFolding, syntaxDiagnostics, type SyntaxProblem } from "./shared";

export interface InstructionDoc {
  signature: string;
  doc: string;
}

export const INSTRUCTIONS: Record<string, InstructionDoc> = {
  FROM: { signature: "FROM [--platform=<platform>] <image>[:<tag>|@<digest>] [AS <name>]", doc: "Starts a new build stage from a base image. Every Dockerfile starts with `FROM` (only `ARG` may come before it). `AS name` names the stage so `COPY --from=name` can use it." },
  RUN: { signature: "RUN [--mount=…] [--network=…] <command>  |  RUN [\"executable\", \"param\"]", doc: "Runs a command in a new layer. The shell form uses `/bin/sh -c`; the exec form does not invoke a shell." },
  CMD: { signature: "CMD [\"executable\", \"param1\"]  |  CMD command param1", doc: "The default command of the container. Only the last `CMD` counts, and it is replaced by arguments to `docker run`." },
  LABEL: { signature: "LABEL <key>=<value> [<key>=<value> …]", doc: "Adds metadata to the image, e.g. `org.opencontainers.image.source`." },
  MAINTAINER: { signature: "MAINTAINER <name>", doc: "**Deprecated.** Use `LABEL org.opencontainers.image.authors` instead." },
  EXPOSE: { signature: "EXPOSE <port>[/<protocol>] [<port>[/<protocol>] …]", doc: "Documents the ports the container listens on. It does not publish them; use `-p` or `ports:` for that." },
  ENV: { signature: "ENV <key>=<value> [<key>=<value> …]", doc: "Sets environment variables for the build and for containers started from the image." },
  ADD: { signature: "ADD [--chown=<user>:<group>] [--chmod=<perms>] [--checksum=<hash>] <src> … <dest>", doc: "Copies files, directories or remote URLs into the image, and unpacks local tar archives. Prefer `COPY` unless you need those extras." },
  COPY: { signature: "COPY [--from=<stage>] [--chown=<user>:<group>] [--chmod=<perms>] [--link] <src> … <dest>", doc: "Copies files or directories from the build context (or another stage with `--from`) into the image." },
  ENTRYPOINT: { signature: "ENTRYPOINT [\"executable\", \"param1\"]  |  ENTRYPOINT command param1", doc: "Configures the container to run as an executable. `CMD` (or `docker run` arguments) become its arguments." },
  VOLUME: { signature: "VOLUME [\"/data\"]  |  VOLUME /data", doc: "Declares a mount point that holds externally mounted or anonymous volumes." },
  USER: { signature: "USER <user>[:<group>]", doc: "Sets the user for the following `RUN`, `CMD` and `ENTRYPOINT` instructions and for the running container." },
  WORKDIR: { signature: "WORKDIR /path/to/workdir", doc: "Sets the working directory for the following instructions, creating it if it does not exist." },
  ARG: { signature: "ARG <name>[=<default value>]", doc: "Defines a build-time variable passed with `--build-arg`. Unlike `ENV`, it is not available in the running container." },
  ONBUILD: { signature: "ONBUILD <INSTRUCTION>", doc: "Registers an instruction to run when this image is used as the base of another build." },
  STOPSIGNAL: { signature: "STOPSIGNAL <signal>", doc: "The system call signal sent to the container to make it exit, e.g. `SIGTERM` or `9`." },
  HEALTHCHECK: { signature: "HEALTHCHECK [--interval=30s] [--timeout=30s] [--start-period=0s] [--retries=3] CMD <command>  |  HEALTHCHECK NONE", doc: "Tells Docker how to test that the container still works." },
  SHELL: { signature: "SHELL [\"executable\", \"parameters\"]", doc: "Overrides the default shell used by the shell form of commands (`[\"/bin/sh\", \"-c\"]` on Linux)." },
};

const FLAGS: Record<string, string[]> = {
  FROM: ["--platform="],
  COPY: ["--from=", "--chown=", "--chmod=", "--link", "--parents", "--exclude="],
  ADD: ["--chown=", "--chmod=", "--checksum=", "--keep-git-dir=", "--link", "--exclude="],
  RUN: ["--mount=", "--network=", "--security="],
  HEALTHCHECK: ["--interval=", "--timeout=", "--start-period=", "--start-interval=", "--retries="],
};

const MOUNT_TYPES = ["bind", "cache", "tmpfs", "secret", "ssh"];

/** A Dockerfile instruction with its arguments, continuation lines joined. */
export interface Statement {
  instruction: string;
  /** Offsets of the instruction keyword. */
  from: number;
  to: number;
  args: string;
  line: number;
}

/** Logical instructions of a Dockerfile (skips comments, parser directives and blank lines). */
export function parseStatements(text: string): Statement[] {
  const out: Statement[] = [];
  const lines = text.split("\n");
  let offset = 0;
  let escape = "\\";
  let i = 0;
  while (i < lines.length && /^\s*#\s*(syntax|escape|check)\s*=/i.test(lines[i])) {
    const esc = /^\s*#\s*escape\s*=\s*(\S)/i.exec(lines[i]);
    if (esc) escape = esc[1];
    offset += lines[i].length + 1;
    i++;
  }
  const trailingEscape = new RegExp(`${escape.replace(/[\\^$*+?.()|[\]{}]/g, "\\$&")}\\s*$`);
  let current: Statement | null = null;
  for (; i < lines.length; i++) {
    const raw = lines[i];
    const start = offset;
    offset += raw.length + 1;
    if (/^\s*#/.test(raw) || (!raw.trim() && !current)) continue;
    if (!current) {
      const m = /^(\s*)(\S+)(?:\s+(.*))?$/.exec(raw);
      if (!m) continue;
      const from = start + m[1].length;
      current = { instruction: m[2].toUpperCase(), from, to: from + m[2].length, args: m[3] ?? "", line: i + 1 };
    } else {
      current.args += "\n" + raw;
    }
    if (raw.trimEnd().endsWith(escape)) {
      current.args = current.args.replace(trailingEscape, "");
    } else {
      out.push(current);
      current = null;
    }
  }
  if (current) out.push(current);
  return out;
}

export function dockerfileProblems(state: EditorState): SyntaxProblem[] {
  const problems: SyntaxProblem[] = [];
  let seenFrom = false;
  for (const s of parseStatements(state.doc.toString())) {
    if (!(s.instruction in INSTRUCTIONS)) {
      problems.push({ from: s.from, to: s.to, message: `Unknown instruction \`${s.instruction}\`` });
      continue;
    }
    if (s.instruction === "FROM") seenFrom = true;
    else if (!seenFrom && s.instruction !== "ARG") {
      problems.push({ from: s.from, to: s.to, message: `\`${s.instruction}\` before the first \`FROM\`; only \`ARG\` may come first` });
    }
    if (s.instruction === "MAINTAINER") {
      problems.push({ from: s.from, to: s.to, message: "`MAINTAINER` is deprecated; use `LABEL org.opencontainers.image.authors`", severity: "weak" });
    }
    if (s.instruction === "FROM" && !s.args.trim()) {
      problems.push({ from: s.from, to: s.to, message: "`FROM` needs a base image" });
    }
  }
  return problems;
}

/** Stage names from `FROM … AS name`. */
export function stageNames(text: string): string[] {
  return parseStatements(text)
    .filter((s) => s.instruction === "FROM")
    .map((s) => /\bAS\s+([\w.-]+)\s*$/i.exec(s.args.trim())?.[1])
    .filter((n): n is string => !!n);
}

/** Variables defined by `ARG` and `ENV`. */
export function variableNames(text: string): string[] {
  const names = new Set<string>();
  for (const s of parseStatements(text)) {
    if (s.instruction === "ARG") {
      const m = /^([A-Za-z_]\w*)/.exec(s.args.trim());
      if (m) names.add(m[1]);
    } else if (s.instruction === "ENV") {
      for (const m of s.args.matchAll(/(?:^|\s)([A-Za-z_]\w*)(?:=|\s)/g)) names.add(m[1]);
    }
  }
  return [...names];
}

const instructionOption = (name: string): Completion => ({
  label: name,
  type: "keyword",
  detail: INSTRUCTIONS[name].signature.split(" ").slice(1, 3).join(" "),
  info: () => docDom({ title: INSTRUCTIONS[name].signature, doc: INSTRUCTIONS[name].doc }),
  boost: name === "RUN" || name === "FROM" || name === "COPY" ? 3 : 0,
});

function completions(ctx: CompletionContext) {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  const text = ctx.state.doc.toString();

  const variable = ctx.matchBefore(/\$\{?[\w]*/);
  if (variable) {
    const from = variable.from + (variable.text.startsWith("${") ? 2 : 1);
    return { from, options: variableNames(text).map((label) => ({ label, type: "variable" })), validFor: /^\w*$/ };
  }
  // A new instruction: the first word of a line that is not a continuation.
  const prev = line.number > 1 ? ctx.state.doc.line(line.number - 1).text : "";
  const continued = line.number > 1 && /\\\s*$/.test(prev);
  const first = /^\s*([A-Za-z]*)$/.exec(before);
  if (first && !continued) {
    const word = ctx.matchBefore(/[A-Za-z]*/)!;
    if (word.from === word.to && !ctx.explicit) return null;
    return { from: word.from, options: Object.keys(INSTRUCTIONS).map(instructionOption), validFor: /^[A-Za-z]*$/ };
  }
  const stmt = /^\s*([A-Za-z]+)\s/.exec(before);
  if (!stmt || continued) return null;
  const instruction = stmt[1].toUpperCase();
  const fromFlag = ctx.matchBefore(/--from=[\w.-]*/);
  if (fromFlag && instruction === "COPY") {
    return {
      from: fromFlag.from + "--from=".length,
      options: stageNames(text).map((label) => ({ label, type: "class", detail: "stage" })),
      validFor: /^[\w.-]*$/,
    };
  }
  const mount = ctx.matchBefore(/--mount=(?:type=)?\w*/);
  if (mount && instruction === "RUN") {
    const typed = mount.text.replace(/^--mount=(type=)?/, "");
    return { from: mount.to - typed.length, options: MOUNT_TYPES.map((label) => ({ label, type: "enum" })), validFor: /^\w*$/ };
  }
  const flag = ctx.matchBefore(/--[\w-]*/);
  if (flag && FLAGS[instruction]) {
    return { from: flag.from, options: FLAGS[instruction].map((label) => ({ label, type: "property" })), validFor: /^--[\w-]*$/ };
  }
  if (instruction === "FROM") {
    const w = ctx.matchBefore(/\bAS\s+\w*$/i);
    if (w) return null;
  }
  return null;
}

export const dockerfileLanguage = StreamLanguage.define(dockerFile);

export function dockerfileSupport() {
  return [
    dockerfileLanguage,
    dockerfileLanguage.data.of({ autocomplete: completions }),
    dockerfileLanguage.data.of({ commentTokens: { line: "#" } }),
    indentFolding(),
    hoverDocs((state, pos) => {
      const line = state.doc.lineAt(pos);
      const m = /^(\s*)([A-Za-z]+)\b/.exec(line.text);
      if (!m) return null;
      const from = line.from + m[1].length;
      const to = from + m[2].length;
      const doc = INSTRUCTIONS[m[2].toUpperCase()];
      if (pos < from || pos > to || !doc) return null;
      return { from, to, dom: () => docDom({ title: doc.signature, doc: doc.doc }) };
    }),
  ];
}

registerLanguage({
  id: "dockerfile",
  label: "Dockerfile",
  extensions: ["dockerfile", "containerfile"],
  fileNames: ["Dockerfile", "Dockerfile.*", "Containerfile", "Containerfile.*"],
  indent: "    ",
  load: dockerfileSupport,
});

registerEditorExtension(({ path }) =>
  languageIdFor(path) === "dockerfile"
    ? syntaxDiagnostics(path, { source: "Dockerfile", serverIds: ["docker-langserver"] }, dockerfileProblems)
    : null,
);
