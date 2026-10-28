//! Command lines of the Rails plugin, built from the project's [`RubyContext`]:
//! `bin/rails server` (the "Rails" run configuration), `bin/rails console`, `dbconsole`,
//! `generate` / `destroy`, `routes` and the generator list. `bin/rails` runs with the
//! project's Ruby (it sets up Bundler itself); without it, `bundle exec rails`.

use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use rubyforge_lang_ruby::{RubyCommand, RubyContext};

use crate::generators::valid_name;

/// The run configuration type of `bin/rails server`.
pub const RAILS: &str = "rails";

/// `ruby bin/rails <args>`, or `bundle exec rails <args>` without a binstub.
pub fn rails(ctx: &RubyContext, has_binstub: bool, args: Vec<String>) -> RubyCommand {
    if has_binstub {
        let mut all = vec!["bin/rails".to_string()];
        all.extend(args);
        ctx.ruby_command(all, false)
    } else {
        ctx.gem_command("rails", "", args)
    }
}

fn environment(env: &str) -> Result<Option<String>, String> {
    let env = env.trim();
    if env.is_empty() {
        return Ok(None);
    }
    if !env.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("'{env}' is not an environment name"));
    }
    Ok(Some(env.to_string()))
}

/// The "Rails" run configuration: `bin/rails server -b <host> -p <port> -e <environment>`
/// with Ruby options (`interpreterOptions`) and extra server arguments.
pub fn server_command(cfg: &Configuration, ctx: &RubyContext, has_binstub: bool) -> Result<RubyCommand, String> {
    let mut args = vec!["server".to_string()];
    let host = cfg.option("host").trim();
    if !host.is_empty() {
        args.extend(["-b".into(), host.to_string()]);
    }
    let port = cfg.option("port").trim();
    if !port.is_empty() {
        port.parse::<u16>().map_err(|_| format!("'{port}' is not a port"))?;
        args.extend(["-p".into(), port.to_string()]);
    }
    if let Some(env) = environment(cfg.option("environment"))? {
        args.extend(["-e".into(), env]);
    }
    args.extend(split_args(&cfg.arguments));
    let mut c = rails(ctx, has_binstub, args);
    let ruby_opts = split_args(&cfg.interpreter_options);
    if !ruby_opts.is_empty() && has_binstub {
        c.args.splice(0..0, ruby_opts);
    }
    c.cwd = cfg.working_directory.trim().to_string();
    Ok(c)
}

/// `bin/rails console [-e env] [--sandbox]`.
pub fn console_command(ctx: &RubyContext, has_binstub: bool, env: &str, sandbox: bool) -> Result<RubyCommand, String> {
    let mut args = vec!["console".to_string()];
    if let Some(e) = environment(env)? {
        args.extend(["-e".into(), e]);
    }
    if sandbox {
        args.push("--sandbox".into());
    }
    Ok(rails(ctx, has_binstub, args))
}

/// `bin/rails dbconsole [-e env]`.
pub fn dbconsole_command(ctx: &RubyContext, has_binstub: bool, env: &str) -> Result<RubyCommand, String> {
    let mut args = vec!["dbconsole".to_string()];
    if let Some(e) = environment(env)? {
        args.extend(["-e".into(), e]);
    }
    Ok(rails(ctx, has_binstub, args))
}

/// `bin/rails generate <generator> <arguments> [options]` (or `destroy`).
pub fn generate_command(
    ctx: &RubyContext,
    has_binstub: bool,
    generator: &str,
    arguments: &str,
    options: &[String],
    destroy: bool,
) -> Result<RubyCommand, String> {
    let generator = generator.trim();
    if !valid_name(generator) {
        return Err(format!("'{generator}' is not a generator"));
    }
    let mut args = vec![
        if destroy { "destroy" } else { "generate" }.to_string(),
        generator.to_string(),
    ];
    args.extend(split_args(arguments));
    for o in options {
        let o = o.trim();
        if !o.starts_with("--") {
            return Err(format!("'{o}' is not an option"));
        }
        args.push(o.to_string());
    }
    Ok(rails(ctx, has_binstub, args))
}

/// `bin/rails routes` (its output is read by [`crate::routes::parse_rails_routes`]).
pub fn routes_command(ctx: &RubyContext, has_binstub: bool) -> RubyCommand {
    rails(ctx, has_binstub, vec!["routes".into()])
}

/// `bin/rails generate` without a generator: the list of generators.
pub fn generators_command(ctx: &RubyContext, has_binstub: bool) -> RubyCommand {
    rails(ctx, has_binstub, vec!["generate".into()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> RubyContext {
        RubyContext {
            ruby: "/home/anna/.rbenv/versions/3.3.6/bin/ruby".into(),
            use_bundler: true,
            ..RubyContext::default()
        }
    }

    #[test]
    fn rails_command_lines() {
        let mut cfg = Configuration {
            type_id: RAILS.into(),
            arguments: "--early-hints".into(),
            interpreter_options: "-W0".into(),
            ..Configuration::default()
        };
        cfg.options.insert("port".into(), "3001".into());
        cfg.options.insert("host".into(), "0.0.0.0".into());
        cfg.options.insert("environment".into(), "development".into());
        let c = server_command(&cfg, &ctx(), true).unwrap();
        assert_eq!(c.program, "/home/anna/.rbenv/versions/3.3.6/bin/ruby");
        assert_eq!(
            c.args.join(" "),
            "-W0 bin/rails server -b 0.0.0.0 -p 3001 -e development --early-hints"
        );
        let c = server_command(&Configuration::default(), &ctx(), false).unwrap();
        assert_eq!((c.program.as_str(), c.args.join(" ")), ("bundle", "exec rails server".into()));
        cfg.options.insert("port".into(), "x".into());
        assert!(server_command(&cfg, &ctx(), true).is_err());

        let c = console_command(&ctx(), true, "test", true).unwrap();
        assert_eq!(c.args.join(" "), "bin/rails console -e test --sandbox");
        assert!(console_command(&ctx(), true, "x; rm", false).is_err());
        let c = generate_command(&ctx(), true, "model", "Article title:string body:text", &["--pretend".into()], false).unwrap();
        assert_eq!(c.args.join(" "), "bin/rails generate model Article title:string body:text --pretend");
        let c = generate_command(&ctx(), true, "controller", "Pages", &[], true).unwrap();
        assert_eq!(c.args[1], "destroy");
        assert!(generate_command(&ctx(), true, "model;x", "", &[], false).is_err());
        assert_eq!(routes_command(&ctx(), true).args, ["bin/rails", "routes"]);
        assert_eq!(dbconsole_command(&ctx(), true, "").unwrap().args, ["bin/rails", "dbconsole"]);
    }
}
