/**
 * Settings | Languages & Frameworks | Ruby, like RubyMine's "Ruby SDK and Gems": the
 * interpreters Forge found (rbenv, rvm, chruby, asdf, mise, Homebrew, the system's) and
 * added by hand, the project's choice (automatic: the version file, else the version
 * manager's global version, else the newest), the gem executables Forge uses with
 * Install for missing ones, then the schema settings (Bundler, Rake, RuboCop, Ruby LSP,
 * debugger) and Ruby LSP from Settings | Language Servers. The interpreter choice stays in
 * the Settings draft until Apply.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { Plus, RefreshCw } from "lucide-solid";
import { rubyApi, type RubyInterpreter } from "@/ipc/ruby";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { LanguageServersPage } from "@/features/langservers/LanguageServersPage";
import { installGem, installRubyLsp } from "./install";
import { errorText, refreshRubyStatus } from "./state";
import { t as tr } from "@/i18n";
import "./ruby.css";

const REASONS: Record<string, string> = {
  setting: "chosen for the project",
  versionFile: "asked for by",
  global: "the version manager's global version",
  newest: "the newest interpreter found",
};

const TOOLS: { tool: string; label: string; gem?: "debug" | "rubocop"; licence: string }[] = [
  { tool: "bundle", label: "Bundler", licence: "MIT" },
  { tool: "rake", label: "Rake", licence: "MIT" },
  { tool: "rspec", label: "RSpec", licence: "MIT" },
  { tool: "rubocop", label: "RuboCop", gem: "rubocop", licence: "MIT" },
  { tool: "rdbg", label: "rdbg (debug gem)", gem: "debug", licence: "BSD-2-Clause" },
  { tool: "irb", label: "IRB", licence: "BSD-2-Clause" },
  { tool: "ruby-lsp", label: "Ruby LSP", licence: "MIT" },
];

export function RubySettingsPage() {
  const [status, { refetch }] = createResource(() => refreshRubyStatus(true));
  const [adding, setAdding] = createSignal(false);
  const [newPath, setNewPath] = createSignal("");
  const [addError, setAddError] = createSignal("");
  const chosen = () => draftValue<string>("ruby.interpreter") ?? "";
  const added = () => draftValue<string[]>("ruby.interpreters") ?? [];
  const isChosen = (i: RubyInterpreter) => (chosen() ? chosen() === i.path || chosen() === i.name : false);
  const add = async () => {
    setAddError("");
    try {
      const i = await rubyApi.probeInterpreter(newPath().trim());
      setDraft("ruby.interpreters", [...new Set([...added(), i.path])]);
      setDraft("ruby.interpreter", i.path);
      setAdding(false);
      setNewPath("");
    } catch (e) {
      setAddError(errorText(e));
    }
  };
  const install = async (tool: string, gem?: "debug" | "rubocop") => {
    if (tool === "ruby-lsp") await installRubyLsp();
    else if (gem) await installGem(gem, gem === "debug" ? "Debugging Ruby needs rdbg." : "RuboCop checks and formats Ruby code.");
    await refetch();
  };
  return (
    <div class="ruby-settings">
      <h3 class="set-section-title">{tr("langs.ruby.rubySdk")}</h3>
      <div class="ruby-row">
        <select data-testid="ruby-interpreter" value={chosen()} onChange={(e) => setDraft("ruby.interpreter", e.currentTarget.value)}>
          <option value="">{`Automatic${status()?.chosen ? ` (${status()!.chosen!.interpreter.name})` : ": no interpreter found"}`}</option>
          <For each={status()?.interpreters ?? []}>{(i) => <option value={i.path}>{i.name}</option>}</For>
        </select>
        <button class="btn-secondary" onClick={() => setAdding(!adding())}>
          <Plus size={13} /> {tr("langs.ruby.addInterpreterEllipsis")}
        </button>
        <button class="icon-btn" title={tr("langs.ruby.detectInterpretersAgain")} onClick={() => void refetch()}>
          <RefreshCw size={13} />
        </button>
      </div>
      <Show when={adding()}>
        <div class="ruby-row">
          <input placeholder="/opt/rubies/ruby-3.4.1/bin/ruby" value={newPath()} onInput={(e) => setNewPath(e.currentTarget.value)} />
          <button class="btn-secondary" disabled={!newPath().trim()} onClick={() => void add()}>
            {tr("common.add")}
          </button>
        </div>
        <Show when={addError()}>
          <div class="ruby-missing">{addError()}</div>
        </Show>
      </Show>
      <div class="ruby-interpreters" role="listbox">
        <For each={status()?.interpreters ?? []} fallback={<div class="ruby-interpreter muted">{tr("langs.ruby.noRubyFoundInstallOne")}</div>}>
          {(i) => (
            <div
              class="ruby-interpreter"
              classList={{ selected: isChosen(i) }}
              role="option"
              data-ruby-interpreter={i.name}
              title={i.path}
              onClick={() => setDraft("ruby.interpreter", isChosen(i) ? "" : i.path)}
            >
              <input type="radio" checked={isChosen(i) || (!chosen() && status()?.chosen?.interpreter.path === i.path)} />
              <span>{i.name}</span>
              <span class="ruby-mono">{i.path}</span>
            </div>
          )}
        </For>
      </div>
      <Show when={status()}>
        {(s) => (
          <div class="ruby-facts">
            <span>{tr("langs.ruby.projectRuby")}</span>
            <span class="ruby-mono">
              {s().chosen
                ? `${s().chosen!.interpreter.name} — ${REASONS[s().chosen!.reason] ?? ""}${s().chosen!.reason === "versionFile" && s().chosen!.request ? ` ${s().chosen!.request!.file}` : ""}`
                : "none"}
            </span>
            <span>{tr("langs.ruby.versionFile")}</span>
            <span class="ruby-mono" classList={{ "ruby-missing": !!s().request && s().chosen?.reason !== "versionFile" && !s().setting }}>
              {s().request
                ? `${s().request!.file}: ${s().request!.version}${s().chosen?.reason !== "versionFile" && !s().setting ? " (not installed)" : ""}`
                : "none"}
            </span>
            <span>{tr("langs.ruby.versionManagers")}</span>
            <span class="ruby-mono">{s().managers.join(", ") || "none"}</span>
            <span>{tr("langs.ruby.gems")}</span>
            <span class="ruby-mono" title={s().probe?.gemPath.join("\n")}>
              {s().probe?.userDir || "unknown"}
            </span>
          </div>
        )}
      </Show>

      <h3 class="set-section-title">{tr("common.tools")}</h3>
      <table class="ruby-tools">
        <tbody>
          <For each={TOOLS}>
            {(t) => {
              const path = () => (t.tool === "ruby-lsp" ? status()?.lsp.path : status()?.tools[t.tool]);
              return (
                <tr data-ruby-tool={t.tool}>
                  <td>{t.label}</td>
                  <td class="ruby-tool-path" classList={{ "ruby-missing": !path() }} title={path() ?? ""}>
                    {path() ?? (status()?.bundler && t.tool !== "ruby-lsp" ? tr("langs.ruby.notFoundBundleExec") : tr("common.notFound"))}
                    {t.tool === "ruby-lsp" && status()?.lsp.managed ? tr("langs.ruby.installedByForge") : ""}
                  </td>
                  <td class="muted">{t.licence}</td>
                  <td>
                    <Show when={!path() && (t.gem || t.tool === "ruby-lsp")}>
                      <button class="pill-btn" onClick={() => void install(t.tool, t.gem)}>
                        {tr("common.install")}
                      </button>
                    </Show>
                  </td>
                </tr>
              );
            }}
          </For>
        </tbody>
      </table>
      <SchemaPage pageId="lang.ruby" />
      <h3 class="set-section-title">{tr("common.languageServer")}</h3>
      <LanguageServersPage groups={["Ruby"]} />
    </div>
  );
}
