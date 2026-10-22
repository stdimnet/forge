//! Parsing and completion on recorded `list --format=json` / `wp cli cmd-dump` output.

use std::collections::HashMap;

use phpforge_cli_tools::complete::{complete, Active, ItemKind};
use phpforge_cli_tools::definitions::{parse_symfony_list, parse_wp_cli_dump, CommandDef};
use phpforge_cli_tools::model::{Tool, ToolKind};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn setup() -> (Vec<Tool>, HashMap<String, Vec<CommandDef>>) {
    let tools = vec![
        Tool::new(ToolKind::Composer),
        Tool::new(ToolKind::Artisan),
        Tool::new(ToolKind::Symfony),
        Tool::new(ToolKind::WpCli),
    ];
    let mut defs = HashMap::new();
    defs.insert(
        "c".to_string(),
        parse_symfony_list(&fixture("composer-list.json")).unwrap(),
    );
    defs.insert(
        "a".to_string(),
        parse_symfony_list(&fixture("artisan-list.json")).unwrap(),
    );
    defs.insert(
        "s".to_string(),
        parse_symfony_list(&fixture("symfony-list.json")).unwrap(),
    );
    defs.insert(
        "wp".to_string(),
        parse_wp_cli_dump(&fixture("wp-cli-dump.json")).unwrap(),
    );
    (tools, defs)
}

fn labels(input: &str) -> Vec<String> {
    let (tools, defs) = setup();
    complete(input, input.encode_utf16().count(), &tools, &defs)
        .items
        .into_iter()
        .map(|i| i.label)
        .collect()
}

#[test]
fn parses_artisan_list() {
    let cmds = parse_symfony_list(&fixture("artisan-list.json")).unwrap();
    let model = cmds.iter().find(|c| c.name == "make:model").unwrap();
    assert_eq!(model.description, "Create a new Eloquent model class");
    assert!(model.usage[0].starts_with("make:model [-a|--all]"));
    // The application's `command` argument is not one of the command's own.
    assert_eq!(model.arguments.len(), 1);
    assert_eq!(model.arguments[0].name, "name");
    assert!(model.arguments[0].required);
    let migration = model.option("migration").unwrap();
    assert_eq!(migration.shortcuts, ["m"]);
    assert!(!migration.accept_value && !migration.global);
    assert!(model.option("env").unwrap().global);
    let complete = cmds.iter().find(|c| c.name == "_complete").unwrap();
    assert!(complete.hidden);
    let seed = cmds.iter().find(|c| c.name == "db:seed").unwrap();
    assert_eq!(
        seed.option("class").unwrap().default.as_deref(),
        Some("Database\\Seeders\\DatabaseSeeder")
    );
}

#[test]
fn parses_composer_and_symfony_lists() {
    let composer = parse_symfony_list(&fixture("composer-list.json")).unwrap();
    let show = composer.iter().find(|c| c.name == "show").unwrap();
    assert_eq!(show.aliases, ["info"]);
    assert_eq!(
        show.arguments
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["package", "version"]
    );
    assert_eq!(
        show.option("format").unwrap().default.as_deref(),
        Some("text")
    );
    assert!(
        composer
            .iter()
            .find(|c| c.name == "update")
            .unwrap()
            .option("with")
            .unwrap()
            .multiple
    );
    let symfony = parse_symfony_list(&fixture("symfony-list.json")).unwrap();
    let env = symfony[0].option("env").unwrap();
    assert_eq!(
        (env.shortcuts.as_slice(), env.default.as_deref()),
        (&["e".to_string()][..], Some("dev"))
    );
}

#[test]
fn parses_wp_cli_dump() {
    let cmds = parse_wp_cli_dump(&fixture("wp-cli-dump.json")).unwrap();
    let names: Vec<&str> = cmds.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "cache",
            "cache add",
            "cache flush",
            "core",
            "core version",
            "plugin",
            "plugin install",
            "plugin list"
        ]
    );
    assert!(cmds[0].group && !cmds[1].group);
    let add = &cmds[1];
    assert_eq!(
        add.usage,
        ["wp cache add <key> <value> [<group>] [<expiration>]"]
    );
    assert_eq!(add.arguments[0].description, "Cache key.");
    assert_eq!(add.arguments[3].default.as_deref(), Some("0"));
    let list = cmds.iter().find(|c| c.name == "plugin list").unwrap();
    let format = list.option("format").unwrap();
    assert_eq!(format.values, ["table", "csv", "count", "json", "yaml"]);
    assert_eq!(format.default.as_deref(), Some("table"));
    assert_eq!(format.description, "Render output in a particular format.");
    assert!(list.option("path").unwrap().global);
    let install = cmds.iter().find(|c| c.name == "plugin install").unwrap();
    assert!(install.arguments[0].array && install.arguments[0].required);
}

#[test]
fn completes_aliases() {
    assert_eq!(labels(""), ["c", "a", "s", "wp"]);
    assert_eq!(labels("w"), ["wp"]);
    // By the tool's name too.
    assert_eq!(labels("Lara"), ["a"]);
}

#[test]
fn completes_commands() {
    assert_eq!(labels("a make")[..2], ["make:migration", "make:model"]);
    assert_eq!(labels("a m:mo"), ["make:model"]);
    // Hidden commands are not offered.
    assert!(!labels("a ").contains(&"_complete".to_string()));
    assert_eq!(labels("a mig")[..2], ["migrate", "migrate:fresh"]);
    let (tools, defs) = setup();
    let c = complete("a make:mo", 6, &tools, &defs);
    // The whole word is replaced.
    assert_eq!((c.from, c.to), (2, 9));
    let model = c.items.iter().find(|i| i.label == "make:model").unwrap();
    assert_eq!(model.insert, "make:model ");
    assert_eq!(model.detail, "<name>");
    assert_eq!(model.kind, ItemKind::Command);
}

#[test]
fn completes_wp_subcommands() {
    assert_eq!(labels("wp pl"), ["plugin"]);
    assert_eq!(labels("wp plugin "), ["install", "list"]);
    assert_eq!(labels("wp plugin li"), ["list"]);
    let (tools, defs) = setup();
    let c = complete("wp cache ", 9, &tools, &defs);
    assert_eq!(c.items[0].kind, ItemKind::Command);
    assert_eq!(c.help.unwrap().command.name, "cache");
}

#[test]
fn completes_options_and_values() {
    let opts = labels("a make:model Order --");
    assert_eq!(opts[..3], ["--all", "--controller", "--factory"]);
    // Command options come before the application's.
    assert!(
        opts.iter().position(|o| o == "--help").unwrap()
            > opts.iter().position(|o| o == "--test").unwrap()
    );
    // Used options are not offered again, unless they may repeat.
    assert!(!labels("a make:model Order -m --").contains(&"--migration".to_string()));
    assert!(labels("c update --with=a/b --").contains(&"--with".to_string()));
    assert_eq!(labels("a make:model Order -m"), ["--migration"]);
    // Values of WP-CLI options.
    assert_eq!(
        labels("wp plugin list --format="),
        ["table", "csv", "count", "json", "yaml"]
    );
    // Prefix matches before substring matches.
    assert_eq!(labels("wp plugin list --status=in"), ["inactive", "dropin"]);
    let (tools, defs) = setup();
    let c = complete("wp plugin list --format=js", 26, &tools, &defs);
    assert_eq!((c.from, c.to), (24, 26));
    assert_eq!(c.items[0].insert, "json ");
    let opt = complete("c show --format", 15, &tools, &defs)
        .items
        .into_iter()
        .find(|i| i.label == "--format")
        .unwrap();
    assert_eq!(opt.insert, "--format=");
    assert_eq!(opt.detail, "-f");
    assert_eq!(
        opt.description,
        "Format of the output: text or json [default: text]"
    );
}

#[test]
fn completes_arguments_and_shows_help() {
    let (tools, defs) = setup();
    let c = complete("a make:model ", 13, &tools, &defs);
    assert_eq!(c.hint.as_deref(), Some("<name>: The name of the model"));
    let help = c.help.unwrap();
    assert_eq!(help.tool, "Laravel Artisan");
    assert_eq!(help.command.name, "make:model");
    assert_eq!(help.active, Some(Active::Argument("name".into())));
    // With nothing typed, the options follow.
    assert!(c.items.iter().any(|i| i.label == "--migration"));
    // `help <command_name>` completes command names, `list <namespace>` namespaces.
    assert_eq!(labels("a help make:m"), ["make:migration", "make:model"]);
    assert_eq!(labels("a list ma"), ["make"]);
    // A value after an option that needs one.
    let c = complete("s debug:router --format ", 24, &tools, &defs);
    assert_eq!(
        c.help.unwrap().active,
        Some(Active::Option("format".into()))
    );
    // Composer's aliases resolve to their command.
    let c = complete("c info ", 7, &tools, &defs);
    assert_eq!(c.help.unwrap().command.name, "show");
    assert_eq!(
        c.hint.as_deref().map(|h| h.starts_with("<package>")),
        Some(true)
    );
    let c = complete("c show laravel/framework ", 25, &tools, &defs);
    assert_eq!(
        c.help.unwrap().active,
        Some(Active::Argument("version".into()))
    );
    // Abbreviations resolve when unique.
    let c = complete("s d:s:u --", 10, &tools, &defs);
    assert_eq!(c.help.unwrap().command.name, "doctrine:schema:update");
}

#[test]
fn explains_what_cannot_be_completed() {
    let (tools, defs) = setup();
    assert_eq!(
        complete("x foo", 5, &tools, &defs).hint.as_deref(),
        Some("Unknown tool alias 'x'")
    );
    assert_eq!(
        complete("a nope ", 7, &tools, &defs).hint.as_deref(),
        Some("Unknown command 'nope'")
    );
    let none = complete("a ", 2, &tools, &HashMap::new());
    assert_eq!(
        none.hint.as_deref(),
        Some("No commands of Laravel Artisan are known")
    );
    assert!(complete("", 0, &[], &defs).hint.is_some());
}
