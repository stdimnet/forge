//! Command lines of RubyForge's run configurations and actions, built from a
//! [`RubyContext`]:
//!
//! - `ruby` configurations: a script with interpreter options and arguments, optionally
//!   through `bundle exec` (option `bundleExec`: `auto`, `true`, `false`);
//! - `rake` configurations and the Rake tool window: `rake <task>[args]`;
//! - Bundler actions (`install`, `update [gem]`, `outdated`, `check`, `lock`) and the
//!   gem commands of the settings page (`gem install`);
//! - the IRB console (`irb`, `bundle exec irb`, `-I lib`);
//! - rdbg: any of these commands under the debugger (`rdbg --open --command -- …`).

use forge_runtools::args::split_args;
use forge_runtools::Configuration;

use crate::context::{RubyCommand, RubyContext};

/// Run configuration types.
pub const RUBY: &str = "ruby";
pub const RAKE: &str = "rake";

fn bundle_exec(cfg: &Configuration) -> bool {
    !matches!(cfg.option("bundleExec").trim(), "false" | "no")
}

/// The command line of a `ruby` (script) configuration.
pub fn script_command(cfg: &Configuration, ctx: &RubyContext) -> Result<RubyCommand, String> {
    let script = cfg.option("script").trim().replace('\\', "/");
    if script.is_empty() {
        return Err("Choose the Ruby script to run".into());
    }
    let mut args = split_args(&cfg.interpreter_options);
    args.push(script);
    args.extend(split_args(&cfg.arguments));
    let mut c = ctx.ruby_command(args, bundle_exec(cfg));
    let interpreter = cfg.option("interpreter").trim();
    if !interpreter.is_empty() && !(bundle_exec(cfg) && ctx.use_bundler) {
        c.program = interpreter.to_string();
    }
    c.cwd = cfg.working_directory.trim().to_string();
    Ok(c)
}

/// `name` with its arguments: `release[origin]`.
pub fn task_with_args(task: &str, args: &str) -> String {
    let args = args.trim();
    if args.is_empty() || task.contains('[') {
        task.to_string()
    } else {
        format!("{task}[{args}]")
    }
}

/// The command line of a `rake` configuration (`task`, `taskArgs`, Rake options in the
/// interpreter options field).
pub fn rake_command(cfg: &Configuration, ctx: &RubyContext) -> Result<RubyCommand, String> {
    let task = cfg.option("task").trim().to_string();
    let mut args = split_args(&cfg.interpreter_options);
    if !task.is_empty() {
        args.push(task_with_args(&task, cfg.option("taskArgs")));
    }
    args.extend(split_args(&cfg.arguments));
    let mut c = if bundle_exec(cfg) {
        ctx.gem_command("rake", "", args)
    } else {
        RubyCommand {
            program: ctx.tool("rake"),
            args,
            cwd: String::new(),
            env: ctx.env.clone(),
        }
    };
    c.cwd = cfg.working_directory.trim().to_string();
    Ok(c)
}

/// `rake --tasks --all` (the Rake tool window's list; `rake -W` for locations).
pub fn rake_tasks_command(ctx: &RubyContext, locations: bool) -> RubyCommand {
    let flag = if locations { "--where" } else { "--tasks" };
    ctx.gem_command("rake", "", vec![flag.into(), "--all".into()])
}

/// A Bundler action of the Bundler tool window or the editor's Gemfile banner.
pub fn bundler_action(
    action: &str,
    gem: Option<&str>,
    ctx: &RubyContext,
) -> Result<RubyCommand, String> {
    let gem = gem.map(str::trim).filter(|g| !g.is_empty());
    let mut args: Vec<&str> = match action {
        "install" => vec!["install"],
        "update" => vec!["update"],
        "outdated" => vec!["outdated"],
        "check" => vec!["check"],
        "lock" => vec!["lock"],
        "init" => vec!["init"],
        other => return Err(format!("Unknown Bundler action '{other}'")),
    };
    if let Some(g) = gem {
        if matches!(action, "update" | "outdated") {
            args.push(g);
        }
    }
    Ok(ctx.bundler_command(&args))
}

/// `gem install <name>` with the interpreter's `gem` (settings page: install RuboCop,
/// debug, ruby-lsp for this Ruby). `--user-install` keeps system Rubies untouched.
pub fn gem_install(
    name: &str,
    ctx: &RubyContext,
    user_install: bool,
) -> Result<RubyCommand, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(|c: char| c.is_whitespace() || c == ';' || c == '&') {
        return Err(format!("'{name}' is not a gem name"));
    }
    let mut args = vec![
        "install".to_string(),
        name.to_string(),
        "--no-document".into(),
    ];
    if user_install {
        args.push("--user-install".into());
    }
    Ok(RubyCommand {
        program: ctx.tool("gem"),
        args,
        cwd: String::new(),
        env: ctx.env.clone(),
    })
}

/// The IRB console: `bundle exec irb` in Bundler projects, else the interpreter's `irb`,
/// with `lib` on the load path.
pub fn irb_command(ctx: &RubyContext, has_lib: bool) -> RubyCommand {
    let mut args = Vec::new();
    if has_lib {
        args.push("-I".to_string());
        args.push("lib".to_string());
    }
    ctx.gem_command("irb", "", args)
}

/// Where rdbg listens: a placeholder Forge's debugger replaces with a free port.
pub const PORT_PLACEHOLDER: &str = "${port}";

/// A command under rdbg (`rdbg --open --host 127.0.0.1 --port <port> --command -- <cmd>`):
/// the debuggee waits for the debugger to connect before it runs.
pub fn rdbg_command(rdbg: &str, command: &RubyCommand) -> RubyCommand {
    let rdbg = if rdbg.trim().is_empty() {
        "rdbg"
    } else {
        rdbg.trim()
    };
    let mut args = vec![
        "--open".to_string(),
        "--host=127.0.0.1".to_string(),
        format!("--port={PORT_PLACEHOLDER}"),
        "--command".to_string(),
        "--".to_string(),
        command.program.clone(),
    ];
    args.extend(command.args.iter().cloned());
    RubyCommand {
        program: rdbg.to_string(),
        args,
        cwd: command.cwd.clone(),
        env: command.env.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> RubyContext {
        let mut c = RubyContext {
            ruby: "/home/anna/.rbenv/versions/3.3.6/bin/ruby".into(),
            use_bundler: true,
            ..RubyContext::default()
        };
        c.tools.insert(
            "rake".into(),
            "/home/anna/.rbenv/versions/3.3.6/bin/rake".into(),
        );
        c.tools.insert(
            "gem".into(),
            "/home/anna/.rbenv/versions/3.3.6/bin/gem".into(),
        );
        c
    }

    #[test]
    fn scripts_and_rake_tasks() {
        let mut cfg = Configuration::new("report", RUBY).with_option("script", "bin/report.rb");
        cfg.interpreter_options = "-w".into();
        cfg.arguments = "--month 2026-10".into();
        let c = script_command(&cfg, &ctx()).unwrap();
        assert_eq!(c.program, "bundle");
        assert_eq!(
            c.args,
            ["exec", "ruby", "-w", "bin/report.rb", "--month", "2026-10"]
        );
        let cfg = cfg.with_option("bundleExec", "false");
        let c = script_command(&cfg, &ctx()).unwrap();
        assert_eq!(c.program, "/home/anna/.rbenv/versions/3.3.6/bin/ruby");
        assert!(script_command(&Configuration::new("x", RUBY), &ctx()).is_err());

        let cfg = Configuration::new("release", RAKE)
            .with_option("task", "release")
            .with_option("taskArgs", "origin");
        let c = rake_command(&cfg, &ctx()).unwrap();
        assert_eq!(c.args, ["exec", "rake", "release[origin]"]);
        let c = rake_command(&cfg.with_option("bundleExec", "false"), &ctx()).unwrap();
        assert_eq!(c.program, "/home/anna/.rbenv/versions/3.3.6/bin/rake");
        assert_eq!(
            rake_tasks_command(&ctx(), false).args,
            ["exec", "rake", "--tasks", "--all"]
        );
    }

    #[test]
    fn bundler_gems_irb_and_rdbg() {
        assert_eq!(
            bundler_action("update", Some("rack"), &ctx()).unwrap().args,
            ["update", "rack"]
        );
        assert_eq!(
            bundler_action("install", Some("rack"), &ctx())
                .unwrap()
                .args,
            ["install"]
        );
        assert!(bundler_action("exec", None, &ctx()).is_err());
        let g = gem_install("ruby-lsp", &ctx(), true).unwrap();
        assert_eq!(g.program, "/home/anna/.rbenv/versions/3.3.6/bin/gem");
        assert_eq!(
            g.args,
            ["install", "ruby-lsp", "--no-document", "--user-install"]
        );
        assert!(gem_install("x; rm -rf /", &ctx(), false).is_err());
        assert_eq!(irb_command(&ctx(), true).args, ["exec", "irb", "-I", "lib"]);
        let script = script_command(
            &Configuration::new("r", RUBY).with_option("script", "app.rb"),
            &ctx(),
        )
        .unwrap();
        let d = rdbg_command("/gems/bin/rdbg", &script);
        assert_eq!(d.program, "/gems/bin/rdbg");
        assert_eq!(
            d.args.join(" "),
            "--open --host=127.0.0.1 --port=${port} --command -- bundle exec ruby app.rb"
        );
    }
}
