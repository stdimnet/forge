//! Actions on Save IPC (Settings | Tools | Actions on Save). The UI asks which actions run
//! for a save (`save_actions_plan`), reformats through the language server, restricts it to
//! changed lines (`save_actions_changed_lines`), applies the PHP actions to the text
//! (`save_actions_php`), writes the file and runs the external tools on it where the project
//! lives (`save_actions_run_tools`). The logic is in `forge-save-actions` and
//! `phpforge-lang-php`.

use forge_core::Result;
use forge_save_actions::tools::{self, ToolLocation, ToolsReport};
use forge_save_actions::{ActionsOnSave, FileTypeChoice, SavePlan, Tool, Trigger};
use phpforge_lang_php::{cleanup, imports, rearrange};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::git::with_repo;
use crate::commands::workspace::blocking;
use crate::state::AppState;

fn settings_of(state: &AppState) -> ActionsOnSave {
    let settings = state.read_settings();
    ActionsOnSave::from_settings(|k| settings.get(k).cloned())
}

/// Custom tool paths from the settings (empty: the project's own installation).
fn custom_paths(state: &AppState) -> impl Fn(Tool) -> Option<String> + Send + 'static {
    let settings = state.read_settings();
    let paths: Vec<(Tool, Option<String>)> = Tool::ALL
        .iter()
        .map(|&t| (t, settings.get_as::<String>(t.path_key())))
        .collect();
    move |tool| {
        paths
            .iter()
            .find(|(t, _)| *t == tool)
            .and_then(|(_, p)| p.clone())
            .filter(|p| !p.trim().is_empty())
    }
}

/// Which actions run when `path` is saved.
#[tauri::command]
pub fn save_actions_plan(path: String, trigger: Trigger, state: State<'_, AppState>) -> SavePlan {
    forge_save_actions::plan(&path, trigger, &settings_of(&state))
}

/// "Changed lines": `formatted` limited to the lines changed since the last commit.
#[tauri::command]
pub async fn save_actions_changed_lines(
    path: String,
    original: String,
    formatted: String,
    state: State<'_, AppState>,
) -> Result<String> {
    // Not in a repository or not committed: the whole file counts as changed.
    let base = with_repo(&state, move |r| r.show("HEAD", &path))
        .await
        .ok()
        .flatten();
    Ok(forge_save_actions::restrict_to_changed_lines(
        base.as_deref(),
        &original,
        &formatted,
    ))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhpActions {
    pub optimize_imports: bool,
    pub rearrange: bool,
    pub code_cleanup: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpActionsResult {
    pub text: String,
    /// Titles of the actions that changed the text.
    pub applied: Vec<String>,
}

/// Apply Optimize imports, Rearrange code and Code cleanup to a PHP text.
pub fn apply_php_actions(text: &str, actions: &PhpActions) -> PhpActionsResult {
    let mut text = text.to_string();
    let mut applied = Vec::new();
    let steps: [(bool, &str, fn(&str) -> Option<String>); 3] = [
        (actions.code_cleanup, "Code cleanup", cleanup::code_cleanup),
        (
            actions.optimize_imports,
            "Optimize imports",
            imports::optimize_imports,
        ),
        (actions.rearrange, "Rearrange code", rearrange::rearrange),
    ];
    for (on, title, step) in steps {
        if !on {
            continue;
        }
        if let Some(next) = step(&text) {
            text = next;
            applied.push(title.to_string());
        }
    }
    PhpActionsResult { text, applied }
}

#[tauri::command]
pub async fn save_actions_php(text: String, actions: PhpActions) -> Result<PhpActionsResult> {
    blocking(move || Ok(apply_php_actions(&text, &actions))).await
}

/// Run the external tools on the saved file `path`.
#[tauri::command]
pub async fn save_actions_run_tools(
    path: String,
    tools: Vec<Tool>,
    state: State<'_, AppState>,
) -> Result<ToolsReport> {
    let custom = custom_paths(&state);
    let backend = state.backend()?;
    blocking(move || {
        tools::run_tools(backend.as_ref(), &path, &tools, &custom, tools::TOOL_TIMEOUT)
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveActionsInfo {
    /// File types of the "Reformat code" filter.
    pub file_types: Vec<FileTypeChoice>,
    /// Where the external tools were found (empty without an open project).
    pub tools: Vec<ToolLocation>,
}

/// What the Actions on Save page shows next to the checkboxes.
#[tauri::command]
pub async fn save_actions_info(state: State<'_, AppState>) -> Result<SaveActionsInfo> {
    let custom = custom_paths(&state);
    let backend = state.backend().ok();
    blocking(move || {
        Ok(SaveActionsInfo {
            file_types: forge_save_actions::file_type_choices(),
            tools: backend
                .map(|b| tools::detect_tools(b.as_ref(), &custom))
                .unwrap_or_default(),
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_actions_run_in_order_and_report_what_changed() {
        let src = "<?php\nuse B\\Two;\nuse A\\One;\nuse C\\Gone;\n\nclass X extends One\n{\n    public function f(Two $t) {}\n    const Y = 1;\n};\n";
        let out = apply_php_actions(
            src,
            &PhpActions {
                optimize_imports: true,
                rearrange: true,
                code_cleanup: true,
            },
        );
        assert_eq!(
            out.text,
            "<?php\nuse A\\One;\nuse B\\Two;\n\nclass X extends One\n{\n    const Y = 1;\n    public function f(Two $t) {}\n}\n"
        );
        assert_eq!(
            out.applied,
            vec!["Code cleanup", "Optimize imports", "Rearrange code"]
        );
        let none = apply_php_actions(src, &PhpActions::default());
        assert_eq!(none.text, src);
        assert!(none.applied.is_empty());
    }
}
