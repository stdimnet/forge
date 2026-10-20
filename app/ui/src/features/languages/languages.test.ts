import { describe, expect, it } from "vitest";
import { languageIdFor, languageLabel, detectLanguage, wildcardMatch } from "@/editor/languages";
import "./index";
import { bashParser } from "./bash";
import { envParser, parseEnv } from "./env";
import { ignoreParser, ignoreProblems } from "./ignore";
import { dockerfileProblems, parseStatements, stageNames, variableNames } from "./dockerfile";
import { confParser, iniParser, iniProblems } from "./ini";
import { masterProblems, mainProblems, postfixMainParser, postfixMasterParser } from "./postfix";
import { yaml } from "@codemirror/lang-yaml";
import { COMPOSE_TAGS, composeSchema, namesUnder } from "./compose";
import { lezerProblems } from "./shared";
import { nodeAt, positionAt, schemaCompletion, schemaHoverAt } from "./yamlSchema";
import { contextAt, stateOf, styleOf, tokenize } from "./testkit";

describe("language detection", () => {
  const id = (p: string, first?: string) => languageIdFor(p, first);
  it("matches extensions, names and wildcards", () => {
    expect(id("src/app.py")).toBe("python");
    expect(id("scripts/deploy.sh")).toBe("shell");
    expect(id("config/a.yml")).toBe("yaml");
    expect(id("composer.json")).toBe("json");
    expect(id("Dockerfile")).toBe("dockerfile");
    expect(id("docker/Dockerfile.prod")).toBe("dockerfile");
    expect(id("docker-compose.override.yml")).toBe("docker-compose");
    expect(id("compose.yaml")).toBe("docker-compose");
    expect(id("composer.yml")).toBe("yaml");
    expect(id("/etc/postfix/main.cf")).toBe("postfix");
    expect(id("/etc/postfix/master.cf")).toBe("postfix-master");
    expect(id("pgsql-aliases.cf")).toBe("postfix");
    expect(id("mail/msg.eml")).toBe("eml");
    expect(id("src/Kernel.php")).toBe("php");
    expect(id("templates/x.blade.php")).toBe("blade");
  });
  it("recognises .env variants and ignore files", () => {
    for (const n of [".env", ".env.local", ".env.test.local", ".env.dist", "prod.env"]) expect(id(n)).toBe("dotenv");
    expect(id(".env.local.php")).toBe("php");
    for (const n of [".gitignore", ".dockerignore", "app/.npmignore", ".prettierignore", ".ignore"]) expect(id(n)).toBe("ignore");
  });
  it("prefers the Symfony services type over plain YAML", () => {
    expect(id("config/services.yaml")).toBe("symfony-services");
    expect(id("config/services_test.yaml")).toBe("symfony-services");
    expect(id("app/config/services/api.yml")).toBe("symfony-services");
    expect(id("config/packages/framework.yaml")).toBe("yaml");
  });
  it("detects scripts by shebang only without a known type", () => {
    expect(id("bin/run", "#!/usr/bin/env python3")).toBe("python");
    expect(id("bin/run", "#!/bin/bash")).toBe("shell");
    expect(id("bin/run", "#!/usr/bin/php")).toBeUndefined();
    expect(id("bin/run")).toBeUndefined();
    expect(languageLabel("bin/run", "#!/bin/sh")).toBe("Shell Script");
    expect(languageLabel("notes.txt")).toBe("Plain text");
    expect(detectLanguage("a.PY")?.id).toBe("python");
  });
  it("wildcards", () => {
    expect(wildcardMatch("a*b*c", "aXbYc")).toBe(true);
    expect(wildcardMatch("*.yml", "x.yml")).toBe(true);
    expect(wildcardMatch("ab*ba", "aba")).toBe(false);
  });
});

describe("bash tokens", () => {
  const t = (src: string) => tokenize(bashParser, src);
  it("styles keywords, builtins, variables and strings", () => {
    const [line] = t('if [ -f "$FILE" ]; then echo ok; fi');
    expect(styleOf(line, "if")).toBe("control");
    expect(styleOf(line, "-f")).toBe("option");
    expect(styleOf(line, "echo")).toBe("builtin");
    expect(styleOf(line, "fi")).toBe("control");
    expect(line.some(([, s]) => s === "variable")).toBe(true);
  });
  it("recognises assignments, functions and comments", () => {
    const lines = t("NAME=value\nbackup() {\n  cp a b # copy\n}\nfunction deploy { :; }");
    expect(styleOf(lines[0], "NAME")).toBe("def");
    expect(styleOf(lines[1], "backup")).toBe("func");
    expect(lines[2].at(-1)).toEqual(["# copy", "comment"]);
    expect(styleOf(lines[4], "deploy")).toBe("func");
  });
  it("nests command substitution inside strings", () => {
    const [line] = t('echo "today: $(date +%F) ${HOME}" && true');
    expect(line.some(([t, st]) => t === "${HOME" && st === "variable")).toBe(true);
    expect(line.at(-1)).toEqual(["true", "builtin"]);
    expect(styleOf(line, "&&")).toBe("operator");
  });
  it("highlights heredoc bodies until the delimiter", () => {
    const lines = t("cat <<EOF\nhello $USER\nEOF\nls");
    expect(lines[1].some(([, s]) => s === "string")).toBe(true);
    expect(lines[1].some(([, s]) => s === "variable")).toBe(true);
    expect(lines[2]).toEqual([["EOF", "keyword"]]);
    expect(styleOf(lines[3], "ls")).toBeNull();
  });
  it("keeps single quotes literal across lines", () => {
    const lines = t("echo 'a $b\nc' done");
    expect(lines[0].some(([, s]) => s === "variable")).toBe(false);
    expect(lines[1][0]).toEqual(["c'", "string"]);
  });
  it("marks the shebang", () => {
    expect(t("#!/bin/bash")[0][0][1]).toBe("meta");
  });
});

describe(".env", () => {
  it("tokenizes keys, values, interpolation and comments", () => {
    const lines = tokenize(envParser, 'export APP_ENV=dev # local\nURL="http://${HOST}:80"\nDEBUG=true\nPORT=8080\n# note');
    expect(styleOf(lines[0], "export")).toBe("keyword");
    expect(styleOf(lines[0], "APP_ENV")).toBe("def");
    expect(lines[0].at(-1)![1]).toBe("comment");
    expect(lines[1].some(([t, s]) => t === "${HOST}" && s === "variable")).toBe(true);
    expect(styleOf(lines[2], "true")).toBe("atom");
    expect(styleOf(lines[3], "8080")).toBe("number");
    expect(lines[4][0][1]).toBe("comment");
  });
  it("spans multi-line double quotes", () => {
    const lines = tokenize(envParser, 'KEY="a\nb"\nNEXT=1');
    expect(lines[1][0][1]).toBe("string");
    expect(styleOf(lines[2], "NEXT")).toBe("def");
  });
  it("parses entries and reports problems", () => {
    const { entries, problems } = parseEnv('A=1\nA=2\nB\nC=hello world\nD="open\nE=1');
    expect(entries.map((e) => e.key)).toEqual(["A", "A", "B", "C", "D"]);
    const msgs = problems.map((p) => p.message);
    expect(msgs.some((m) => m.includes("already defined on line 1"))).toBe(true);
    expect(msgs.some((m) => m.includes("no `=`"))).toBe(true);
    expect(msgs.some((m) => m.includes("must be quoted"))).toBe(true);
    expect(msgs.some((m) => m.includes("Unterminated"))).toBe(true);
  });
  it("accepts valid files", () => {
    expect(parseEnv('# c\nA=1\nB="x y"\nC=$(pwd)\nD=a#b\n').problems).toEqual([]);
  });
});

describe("ignore files", () => {
  it("tokenizes patterns", () => {
    const lines = tokenize(ignoreParser, "# c\n###> x ###\n!keep.log\n/build/**/*.o\n\\#file");
    expect(lines[0][0][1]).toBe("comment");
    expect(lines[1][0][1]).toBe("meta");
    expect(lines[2][0]).toEqual(["!", "keyword"]);
    expect(lines[3].filter(([, s]) => s === "regexp").length).toBe(2);
    expect(lines[4][0][1]).toBe("escape");
  });
  it("flags duplicates only", () => {
    expect(ignoreProblems("a\n# a\nb\na\n")).toHaveLength(1);
    expect(ignoreProblems("a\nb\n")).toEqual([]);
  });
});

describe("Dockerfile", () => {
  const df = "# syntax=docker/dockerfile:1\nARG V=1\nFROM php:8.3 AS base\nENV A=1 B=2\nRUN apt-get update \\\n  && apt-get install -y git\nFROM base AS app\nCOPY --from=base /x /y\n";
  it("joins continued instructions", () => {
    const s = parseStatements(df);
    expect(s.map((x) => x.instruction)).toEqual(["ARG", "FROM", "ENV", "RUN", "FROM", "COPY"]);
    expect(s[3].args).toContain("apt-get install");
  });
  it("collects stages and variables", () => {
    expect(stageNames(df)).toEqual(["base", "app"]);
    expect(variableNames(df).sort()).toEqual(["A", "B", "V"]);
  });
  it("reports unknown instructions and a missing FROM", () => {
    expect(dockerfileProblems(stateOf(df))).toEqual([]);
    const p = dockerfileProblems(stateOf("RUN x\nFROM a\nFOO bar\nMAINTAINER me"));
    expect(p.map((x) => x.message.slice(0, 12))).toEqual(["`RUN` before", "Unknown inst", "`MAINTAINER`"]);
  });
});

describe("Postfix", () => {
  it("tokenizes main.cf", () => {
    const lines = tokenize(
      postfixMainParser,
      "# comment\nmyhostname = mail.example.org\nsmtpd_recipient_restrictions = permit_mynetworks,\n    reject_unauth_destination\nalias_maps = hash:/etc/aliases\nmessage_size_limit = 10240000\nsmtpd_tls_security_level = may\nmydestination = $myhostname, localhost",
    );
    expect(lines[0][0][1]).toBe("comment");
    expect(styleOf(lines[1], "myhostname")).toBe("def");
    expect(styleOf(lines[2], "permit_mynetworks")).toBe("keyword");
    expect(styleOf(lines[3], "reject_unauth_destination")).toBe("keyword");
    expect(styleOf(lines[4], "hash")).toBe("type");
    expect(styleOf(lines[5], "10240000")).toBe("number");
    expect(styleOf(lines[7], "$myhostname")).toBe("variable");
  });
  it("tokenizes master.cf columns", () => {
    const lines = tokenize(
      postfixMasterParser,
      "smtp      inet  n       -       y       -       -       smtpd\n  -o syslog_name=postfix/submission\n  -o smtpd_tls_security_level=encrypt",
    );
    expect(styleOf(lines[0], "smtp")).toBe("def");
    expect(styleOf(lines[0], "inet")).toBe("keyword");
    expect(styleOf(lines[0], "n")).toBe("atom");
    expect(styleOf(lines[0], "smtpd")).toBe("builtin");
    expect(styleOf(lines[1], "-o")).toBe("option");
    expect(styleOf(lines[1], "syslog_name")).toBe("def");
    expect(styleOf(lines[2], "encrypt")).toBeNull();
  });
  it("finds problems", () => {
    const messages = mainProblems("a = 1\nb = 2\na = 3\nnonsense\n").map((p) => p.message);
    expect(messages).toContain("`a` is defined again on line 3; the last definition wins");
    expect(messages).toContain("Expected `parameter = value`");
    expect(messages).toHaveLength(2);
    expect(masterProblems("smtp inet n - y - - smtpd\nbad foo n - y - - x\nshort inet n\n")).toHaveLength(2);
  });
});

describe("INI and conf", () => {
  it("detects the types", () => {
    expect(languageIdFor("php.ini")).toBe("ini");
    expect(languageIdFor("/etc/mysql/my.cnf")).toBe("ini");
    expect(languageIdFor(".editorconfig")).toBe("ini");
    expect(languageIdFor("nginx.conf")).toBe("nginx");
    expect(languageIdFor("sysctl.conf")).toBe("conf");
    expect(languageIdFor("supervisord.conf")).toBe("ini");
  });
  it("tokenizes ini", () => {
    const lines = tokenize(iniParser, "; c\n[PHP]\nmemory_limit = 512M ; big\ndisplay_errors=Off\nname = \"x y\"\nurl = http://a.b/c");
    expect(lines[0][0][1]).toBe("comment");
    expect(lines[1][0]).toEqual(["[PHP]", "heading"]);
    expect(styleOf(lines[2], "memory_limit")).toBe("def");
    expect(styleOf(lines[2], "512M")).toBe("number");
    expect(lines[2].at(-1)![1]).toBe("comment");
    expect(styleOf(lines[3], "Off")).toBe("atom");
    expect(styleOf(lines[4], '"x y"')).toBe("string");
    expect(styleOf(lines[5], "http://a.b/c")).toBe("url");
  });
  it("tokenizes conf with blocks and key/value without separator", () => {
    const lines = tokenize(confParser, "# c\nnet.ipv4.ip_forward = 1\nworker_processes auto;\nserver {\n  listen 80;\n}");
    expect(lines[0][0][1]).toBe("comment");
    expect(styleOf(lines[1], "net.ipv4.ip_forward")).toBe("def");
    expect(styleOf(lines[2], "worker_processes")).toBe("def");
    expect(styleOf(lines[2], ";")).toBe("punct");
    expect(styleOf(lines[3], "{")).toBe("bracket");
    expect(styleOf(lines[4], "80")).toBe("number");
  });
  it("reports broken sections and duplicates", () => {
    const m = iniProblems("[a\nx=1\n[b]\ny=1\ny=2\nz[]=1\nz[]=2").map((p) => p.message);
    expect(m).toHaveLength(2);
    expect(m[0]).toContain("closing");
    expect(m[1]).toContain("already set");
  });
});

describe("YAML schema engine", () => {
  const yml = "services:\n  web:\n    image: nginx\n    ports:\n      - \"80:80\"\n    depends_on:\n      - db\n    build:\n      ";
  it("computes the key path at the cursor", () => {
    const s = stateOf(yml);
    const p = positionAt(s, s.doc.length);
    expect(p.path).toEqual(["services", "web", "build"]);
    expect(p.keyPosition).toBe(true);
    const item = stateOf("services:\n  web:\n    depends_on:\n      - ");
    expect(positionAt(item, item.doc.length).path).toEqual(["services", "web", "depends_on", "-"]);
    const val = stateOf("services:\n  web:\n    restart: ");
    const pv = positionAt(val, val.doc.length);
    expect(pv.keyPosition).toBe(false);
    expect(pv.currentKey).toBe("restart");
    expect(pv.path).toEqual(["services", "web"]);
  });
  it("handles sequence items that are mappings", () => {
    const s = stateOf("services:\n  a:\n    tags:\n      - name: x\n        ");
    expect(positionAt(s, s.doc.length).path).toEqual(["services", "a", "tags", "-"]);
    const dashAtKey = stateOf("tags:\n- name: x\n  ");
    expect(positionAt(dashAtKey, dashAtKey.doc.length).path).toEqual(["tags", "-"]);
  });
  it("walks the schema", () => {
    expect(nodeAt(composeSchema, ["services", "web", "healthcheck"])?.keys?.interval).toBeDefined();
    expect(nodeAt(composeSchema, ["services", "web", "x-custom"])).toBeNull();
    expect(nodeAt(composeSchema, ["nope"])).toBeNull();
  });
  it("completes service keys without repeating present ones", () => {
    const complete = schemaCompletion({ root: composeSchema });
    const res = complete(contextAt("services:\n  web:\n    image: x\n    |"))!;
    const labels = res.options.map((o) => o.label);
    expect(labels).toContain("ports");
    expect(labels).not.toContain("image");
    expect(res.options.find((o) => o.label === "build")?.apply).toBe("build:\n      ");
  });
  it("completes values", () => {
    const complete = schemaCompletion({ root: composeSchema });
    const res = complete(contextAt("services:\n  web:\n    restart: |"))!;
    expect(res.options.map((o) => o.label)).toEqual(["no", "always", "on-failure", "unless-stopped"]);
    expect(complete(contextAt("services:\n  web:\n    image: |"))).toBeNull();
  });
  it("completes the merge tags !reset and !override as the first token of a value", () => {
    const complete = schemaCompletion({ root: composeSchema, tags: COMPOSE_TAGS });
    const labels = (doc: string) => complete(contextAt(doc))?.options.map((o) => o.label) ?? null;
    expect(labels("services:\n  web:\n    ports: |")).toEqual(["!reset", "!override"]);
    expect(labels("services:\n  web:\n    restart: |")).toEqual(["no", "always", "on-failure", "unless-stopped", "!reset", "!override"]);
    const typed = complete(contextAt("services:\n  web:\n    build: !re|"))!;
    expect(typed.options.map((o) => o.label)).toEqual(["!reset", "!override"]);
    expect(typed.from).toBe("services:\n  web:\n    build: ".length);
    expect(typed.validFor instanceof RegExp && typed.validFor.test("!reset")).toBe(true);
    // After a tag, the values of the key follow, not another tag.
    expect(labels("services:\n  web:\n    restart: !override |")).toEqual(["no", "always", "on-failure", "unless-stopped"]);
    expect(labels("services:\n  web:\n    image: nginx !|")).toBeNull();
    expect(labels("services:\n  web:\n    |")).not.toContain("!reset");
  });
  it("documents the merge tags on hover", () => {
    const doc = "services:\n  web:\n    ports: !reset []\n    build: !override\n      context: .\n    image: !custom x\n";
    const s = stateOf(doc, yaml());
    const hover = (needle: string, offset = 1) => schemaHoverAt(composeSchema, COMPOSE_TAGS, s, doc.indexOf(needle) + offset);
    const reset = hover("!reset")!;
    expect(doc.slice(reset.from, reset.to)).toBe("!reset");
    expect(reset.dom().textContent).toContain("type's default value");
    const override = hover("!override", "!override".length)!;
    expect(doc.slice(override.from, override.to)).toBe("!override");
    expect(override.dom().textContent).toContain("instead of merging");
    expect(hover("!custom")).toBeNull();
    // Keys keep their own documentation.
    expect(hover("ports")?.dom().textContent).toContain("Published ports");
  });
  it("parses tagged values without syntax errors", () => {
    const doc =
      "services:\n  web:\n    build: !reset null\n    ports: !reset []\n    environment: !reset {}\n" +
      "    command: !reset\n    volumes: !override\n      - ./data:/data\n    networks: !override [\"front\"]\n";
    expect(lezerProblems(stateOf(doc, yaml()))).toEqual([]);
  });
  it("lists declared names", () => {
    expect(namesUnder("services:\n  web:\n    image: a\n  db:\n    image: b\nvolumes:\n  data:\n", "services")).toEqual(["web", "db"]);
    expect(namesUnder("volumes:\n  data:\n", "networks")).toEqual([]);
  });
});
