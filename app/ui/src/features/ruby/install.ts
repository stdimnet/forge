/**
 * Installs that Forge only starts with the user's consent, always with the user's own
 * RubyGems (nothing is bundled or downloaded by Forge itself):
 *
 * - Ruby LSP (MIT): `gem install ruby-lsp --install-dir <Forge's data folder>/…` for the
 *   project's Ruby, then Ruby LSP is configured again (`ruby_lsp_install_command`);
 * - gems of the interpreter (`debug` for rdbg, `rubocop`): `gem install <gem>`
 *   (`--user-install` for system Rubies).
 *
 * Command lines are built in Rust and run in the Run tool window.
 */
import { rubyApi } from "@/ipc/ruby";
import { confirmDialog } from "@/features/browserCompanion/ConfirmDialog";
import { errorText, refreshRubyStatus, rubyStatus, runRubyCommand, showRubyBalloon } from "./state";

const LICENCES: Record<string, string> = {
  debug: "BSD-2-Clause, Ruby's debugger",
  rubocop: "MIT",
  "ruby-lsp": "MIT",
};

/** Install Ruby LSP into Forge's data folder with the project interpreter's gem. */
export async function installRubyLsp(): Promise<boolean> {
  const s = rubyStatus() ?? (await refreshRubyStatus());
  if (!s?.chosen) {
    showRubyBalloon("No Ruby interpreter", "Install Ruby (rbenv, rvm, chruby, asdf, mise or your system's package) first.", 8000);
    return false;
  }
  let cmd;
  try {
    cmd = await rubyApi.lspInstallCommand();
  } catch (e) {
    showRubyBalloon("Cannot install Ruby LSP", errorText(e), 8000);
    return false;
  }
  const ok = await confirmDialog({
    title: "Install Ruby LSP",
    message: `Ruby LSP (Shopify, MIT) is not installed for ${s.chosen.interpreter.name}. Forge installs it with this Ruby's RubyGems from rubygems.org into Forge's data folder; your Ruby and its gems stay untouched.`,
    detail: [cmd.program.split(/[\\/]/).pop(), ...cmd.args].join(" "),
    question: "Install it now?",
    okLabel: "Install",
  });
  if (!ok) return false;
  const code = await runRubyCommand(cmd, "gem install ruby-lsp");
  await refreshRubyStatus(true);
  await rubyApi.lspRefresh().catch(() => undefined);
  if (code === 0) showRubyBalloon("Ruby LSP is installed", "Code insight for Ruby starts with the next Ruby file you open.", 5000);
  else showRubyBalloon("Ruby LSP was not installed", "See the Run tool window for RubyGems' output.", 8000);
  return code === 0;
}

/** Install a gem (`debug`, `rubocop`) with the project interpreter's gem. */
export async function installGem(gem: "debug" | "rubocop", why: string): Promise<boolean> {
  const s = rubyStatus() ?? (await refreshRubyStatus());
  let cmd;
  try {
    cmd = await rubyApi.gemInstallCommand(gem);
  } catch (e) {
    showRubyBalloon(`Cannot install ${gem}`, errorText(e), 8000);
    return false;
  }
  const ok = await confirmDialog({
    title: `Install the ${gem} gem`,
    message: `${why} Forge installs the ${gem} gem (${LICENCES[gem] ?? "see rubygems.org"}) with ${s?.chosen?.interpreter.name ?? "the project's Ruby"}. In Bundler projects, add it to the Gemfile instead.`,
    detail: [cmd.program.split(/[\\/]/).pop(), ...cmd.args].join(" "),
    question: "Install it now?",
    okLabel: "Install",
  });
  if (!ok) return false;
  const code = await runRubyCommand(cmd, `gem install ${gem}`);
  await refreshRubyStatus(true);
  if (code === 0) showRubyBalloon(`${gem} is installed`, "", 4000);
  return code === 0;
}
