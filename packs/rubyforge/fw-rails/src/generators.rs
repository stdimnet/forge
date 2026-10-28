//! Rails generators for the Run Rails Generator dialog: the built-in ones with their
//! argument syntax, the list `bin/rails generate` prints (with the generators of the
//! bundle's gems; trusted projects only), and the project's own `lib/generators`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    /// `model`, `active_record:migration`, `stimulus`.
    pub name: String,
    /// `Rails`, `ActiveRecord`, `Stimulus`, `Project`.
    pub group: String,
    /// `NAME [field[:type][:index] field[:type][:index]] [options]`.
    pub usage: String,
    pub description: String,
}

const FIELDS: &str = "NAME [field[:type][:index] field[:type][:index]]";

/// Rails' own generators (Rails 7.1 – 8).
pub const BUILT_IN: &[(&str, &str, &str)] = &[
    (
        "model",
        FIELDS,
        "A model with its migration, test and fixtures",
    ),
    (
        "controller",
        "NAME [action action]",
        "A controller with actions, views, routes and tests",
    ),
    (
        "scaffold",
        FIELDS,
        "Model, migration, controller, views, routes and tests of a resource",
    ),
    (
        "resource",
        FIELDS,
        "Model, migration, an empty controller and resource routes",
    ),
    (
        "migration",
        "NAME [field[:type][:index] field[:type][:index]]",
        "A database migration (AddXToY / RemoveXFromY / CreateJoinTableXY)",
    ),
    (
        "scaffold_controller",
        FIELDS,
        "A scaffolded controller and views for an existing model",
    ),
    (
        "authentication",
        "",
        "Session-based authentication (User, Session, controllers, views)",
    ),
    (
        "mailer",
        "NAME [method method]",
        "A mailer with its views and previews",
    ),
    ("job", "NAME", "An Active Job"),
    ("channel", "NAME [method method]", "An Action Cable channel"),
    ("mailbox", "NAME", "An Action Mailbox mailbox"),
    ("helper", "NAME", "A view helper module"),
    ("integration_test", "NAME", "An integration test"),
    ("system_test", "NAME", "A system test"),
    (
        "task",
        "NAMESPACE [task task]",
        "A Rake task file in lib/tasks",
    ),
    ("generator", "NAME", "A new generator in lib/generators"),
    ("application_record", "", "The ApplicationRecord base class"),
    ("benchmark", "NAME [report report]", "A benchmark script"),
    ("script", "NAME", "A script in script/"),
    ("jbuilder", FIELDS, "Jbuilder views of a resource"),
    ("stimulus", "NAME", "A Stimulus controller"),
];

/// The built-in generators.
pub fn built_in() -> Vec<Generator> {
    BUILT_IN
        .iter()
        .map(|(name, usage, description)| Generator {
            name: name.to_string(),
            group: if *name == "stimulus" {
                "Stimulus"
            } else {
                "Rails"
            }
            .into(),
            usage: usage.to_string(),
            description: description.to_string(),
        })
        .collect()
}

/// The project's generators: `lib/generators/<name>/<name>_generator.rb`.
pub fn project_generators<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<Generator> {
    let mut out: Vec<Generator> = Vec::new();
    for p in paths {
        let Some(rest) = p.strip_prefix("lib/generators/") else {
            continue;
        };
        let Some(stem) = rest.strip_suffix("_generator.rb") else {
            continue;
        };
        let name = stem.rsplit('/').next().unwrap_or(stem).to_string();
        let ns: Vec<&str> = stem.split('/').collect();
        let full = if ns.len() > 2 {
            format!("{}:{name}", ns[..ns.len() - 2].join(":"))
        } else {
            name
        };
        if !out.iter().any(|g| g.name == full) {
            out.push(Generator {
                name: full,
                group: "Project".into(),
                usage: "NAME".into(),
                description: format!("lib/generators/{stem}_generator.rb"),
            });
        }
    }
    out
}

/// The generators `bin/rails generate` lists (after "Please choose a generator below."),
/// with the descriptions of `known` ones.
pub fn parse_generate_output(output: &str, known: &[Generator]) -> Vec<Generator> {
    let mut out = Vec::new();
    let mut group = String::new();
    let mut listing = false;
    for line in output.lines() {
        if line.contains("Please choose a generator below") {
            listing = true;
            continue;
        }
        if !listing || line.trim().is_empty() {
            continue;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let t = line.trim();
        if !indented && t.ends_with(':') {
            group = t.trim_end_matches(':').to_string();
            continue;
        }
        if indented && !t.contains(' ') {
            let k = known
                .iter()
                .find(|g| g.name == t || t.strip_prefix("rails:") == Some(g.name.as_str()));
            out.push(Generator {
                name: t.to_string(),
                group: group.clone(),
                usage: k.map(|g| g.usage.clone()).unwrap_or_else(|| "NAME".into()),
                description: k.map(|g| g.description.clone()).unwrap_or_default(),
            });
        }
    }
    out
}

/// Whether a generator name is safe to put on a command line.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_from_rails_and_the_project() {
        let output = "Usage:\n  bin/rails generate GENERATOR [args] [options]\n\nGeneral options:\n  -h, [--help]     # Print generator's options and usage\n\nPlease choose a generator below.\n\nRails:\n  application_record\n  controller\n  model\n\nActiveRecord:\n  active_record:application_record\n  active_record:multi_db\n\nStimulus:\n  stimulus\n";
        let g = parse_generate_output(output, &built_in());
        let names: Vec<(&str, &str)> = g
            .iter()
            .map(|x| (x.group.as_str(), x.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("Rails", "application_record"),
                ("Rails", "controller"),
                ("Rails", "model"),
                ("ActiveRecord", "active_record:application_record"),
                ("ActiveRecord", "active_record:multi_db"),
                ("Stimulus", "stimulus"),
            ]
        );
        assert_eq!(g[2].usage, FIELDS);
        let p = project_generators(
            [
                "lib/generators/service/service_generator.rb",
                "lib/generators/admin/page/page_generator.rb",
            ]
            .into_iter(),
        );
        let names: Vec<&str> = p.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["service", "admin:page"]);
        assert!(valid_name("active_record:migration") && !valid_name("x; rm -rf"));
    }
}
