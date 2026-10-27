//! PHPForge quality tools (Settings | Languages & Frameworks | PHP | Quality Tools):
//! PHPStan, Psalm, PHP CS Fixer, PHP_CodeSniffer (phpcs/phpcbf), PHP Mess Detector, Laravel
//! Pint and Rector.
//!
//! - [`model`]: the tools and one configuration per tool (`php.tools.<id>.*`: inspection
//!   on/off, executable, PHP interpreter, configuration file, level/ruleset, options,
//!   timeout, severity).
//! - [`command`]: their command lines for checking a file (saved, on stdin or as a
//!   temporary copy) and for fixing it.
//! - [`parse`]: their JSON reports as [`Problem`]s, with the paths the tool printed mapped
//!   back into the project ([`PathContext`]: interpreter path mappings, stdin aliases).
//! - [`diff`]: the fixers' unified diffs, applied to the editor text or shown as a preview.
//!
//! The crate is pure: the app runs the commands through the project backend and the PHP
//! interpreter wrapping.

pub mod command;
pub mod diff;
pub mod model;
pub mod parse;
pub mod problem;

pub use command::{check_args, fix_args, fix_mode, in_place_args, FixMode, Input, ToolArgs};
pub use model::{QualityTool, SeverityChoice, ToolConfig, ToolKind};
pub use parse::{parse_report, FileFix, Report};
pub use problem::{PathContext, Problem, Severity};

/// How a check of one file is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckTrigger {
    /// The editor text differs from the saved file (on the fly while typing).
    Unsaved,
    /// The file was opened or saved: the text on disk is current.
    Saved,
}

/// The tools that check a file now, among those whose inspection is on. While the text is
/// unsaved only tools that can read it run: phpcs and PHP CS Fixer through stdin, PHPStan
/// through a temporary copy when `temp_copies` is possible (its PHP sees this machine's
/// temporary directory).
pub fn tools_to_run(
    configs: &[ToolConfig],
    trigger: CheckTrigger,
    temp_copies: bool,
) -> Vec<QualityTool> {
    configs
        .iter()
        .filter(|c| c.inspection)
        .map(|c| c.tool)
        .filter(|&t| match trigger {
            CheckTrigger::Saved => true,
            CheckTrigger::Unsaved => match t {
                QualityTool::Phpstan => temp_copies,
                t => t.checks_unsaved_text(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsaved_text_runs_only_tools_that_can_read_it() {
        let mut configs: Vec<ToolConfig> = QualityTool::ALL
            .iter()
            .map(|&t| ToolConfig::new(t))
            .collect();
        assert!(tools_to_run(&configs, CheckTrigger::Saved, true).is_empty());
        for c in &mut configs {
            c.inspection = true;
        }
        assert_eq!(tools_to_run(&configs, CheckTrigger::Saved, false).len(), 7);
        assert_eq!(
            tools_to_run(&configs, CheckTrigger::Unsaved, false),
            vec![QualityTool::Phpcs, QualityTool::PhpCsFixer]
        );
        assert!(tools_to_run(&configs, CheckTrigger::Unsaved, true).contains(&QualityTool::Phpstan));
    }
}
