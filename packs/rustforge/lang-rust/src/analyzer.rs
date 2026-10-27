//! rust-analyzer's configuration from Settings | Rust: check on save (with Clippy), cargo
//! features, the target triple, build scripts and proc macros. The same object is sent as
//! initialization options and as `workspace/configuration` settings (`rust-analyzer`).
//! Build scripts and proc macros run project code: they are off in untrusted projects (Safe
//! Mode refuses rust-analyzer altogether, see `forge_trust::SERVERS_RUNNING_PROJECT_CODE`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyzerSettings {
    pub check_on_save: bool,
    /// `clippy` or `check`.
    pub check_command: String,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    /// A target triple (`wasm32-unknown-unknown`); empty: the host.
    pub target: String,
    pub build_scripts: bool,
    pub proc_macros: bool,
    pub env: BTreeMap<String, String>,
    /// The project is trusted (build scripts and proc macros may run).
    pub trusted: bool,
}

impl Default for AnalyzerSettings {
    fn default() -> Self {
        Self {
            check_on_save: true,
            check_command: "clippy".into(),
            features: Vec::new(),
            all_features: false,
            no_default_features: false,
            target: String::new(),
            build_scripts: true,
            proc_macros: true,
            env: BTreeMap::new(),
            trusted: true,
        }
    }
}

impl AnalyzerSettings {
    /// The `rust-analyzer` configuration object.
    pub fn config(&self) -> Value {
        let features: Value = if self.all_features {
            json!("all")
        } else {
            json!(self.features)
        };
        let command = if self.check_command == "check" {
            "check"
        } else {
            "clippy"
        };
        json!({
            "checkOnSave": self.check_on_save && self.trusted,
            "check": {
                "command": command,
                "features": features,
                "noDefaultFeatures": self.no_default_features,
            },
            "cargo": {
                "features": features,
                "noDefaultFeatures": self.no_default_features,
                "target": if self.target.trim().is_empty() { Value::Null } else { json!(self.target.trim()) },
                "buildScripts": { "enable": self.build_scripts && self.trusted },
                "extraEnv": self.env,
            },
            "procMacro": { "enable": self.proc_macros && self.trusted },
        })
    }

    /// `{ "rust-analyzer": config }` for `workspace/configuration`.
    pub fn settings(&self) -> Value {
        json!({ "rust-analyzer": self.config() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_become_rust_analyzer_config() {
        let s = AnalyzerSettings {
            features: vec!["json".into(), "discounts".into()],
            target: "wasm32-unknown-unknown".into(),
            ..AnalyzerSettings::default()
        };
        let c = s.config();
        assert_eq!(c["check"]["command"], "clippy");
        assert_eq!(c["cargo"]["features"], json!(["json", "discounts"]));
        assert_eq!(c["cargo"]["target"], "wasm32-unknown-unknown");
        assert_eq!(c["procMacro"]["enable"], true);
        assert_eq!(s.settings()["rust-analyzer"]["checkOnSave"], true);
        let all = AnalyzerSettings {
            all_features: true,
            check_command: "check".into(),
            ..AnalyzerSettings::default()
        };
        assert_eq!(all.config()["cargo"]["features"], "all");
        assert_eq!(all.config()["check"]["command"], "check");
        assert_eq!(all.config()["cargo"]["target"], Value::Null);
        let untrusted = AnalyzerSettings {
            trusted: false,
            ..AnalyzerSettings::default()
        };
        let c = untrusted.config();
        assert_eq!(
            (
                c["procMacro"]["enable"].clone(),
                c["cargo"]["buildScripts"]["enable"].clone(),
                c["checkOnSave"].clone()
            ),
            (json!(false), json!(false), json!(false))
        );
    }
}
