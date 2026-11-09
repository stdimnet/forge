//! Schemas bundled with Forge for the built-in mappings (licences in `schemas/NOTICE`).
//! They are stored compressed (see `build.rs`) and parsed once on first use.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};

use crate::schema::SchemaDoc;

pub struct Bundled {
    /// File name in `schemas/`.
    pub file: &'static str,
    /// The URLs it is published under; the first is its canonical URI.
    pub uris: &'static [&'static str],
    data: &'static [u8],
}

macro_rules! bundled {
    ($file:literal, [$($uri:literal),+ $(,)?]) => {
        Bundled {
            file: $file,
            uris: &[$($uri),+],
            data: include_bytes!(concat!(env!("OUT_DIR"), "/", $file, ".deflate")),
        }
    };
}

pub const COMPOSER: &str = "https://getcomposer.org/schema.json";
pub const PACKAGE: &str = "https://json.schemastore.org/package.json";
pub const TSCONFIG: &str = "https://json.schemastore.org/tsconfig.json";
pub const JSCONFIG: &str = "https://json.schemastore.org/jsconfig.json";
pub const ESLINTRC: &str = "https://json.schemastore.org/eslintrc.json";
pub const PRETTIERRC: &str = "https://json.schemastore.org/prettierrc.json";
pub const COMPOSE: &str =
    "https://raw.githubusercontent.com/compose-spec/compose-spec/master/schema/compose-spec.json";
pub const GITHUB_WORKFLOW: &str = "https://json.schemastore.org/github-workflow.json";
pub const GITHUB_ACTION: &str = "https://json.schemastore.org/github-action.json";
pub const GITLAB_CI: &str = "https://gitlab.com/gitlab-org/gitlab-foss/-/raw/master/app/assets/javascripts/editor/schema/ci.json";

pub const MINECRAFT_BUKKIT: &str = concat!(
    "https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/",
    "minecraft-bukkit-plugin.json"
);
pub const MINECRAFT_PAPER: &str = concat!(
    "https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/",
    "minecraft-paper-plugin.json"
);
pub const MINECRAFT_VELOCITY: &str = concat!(
    "https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/",
    "minecraft-velocity-plugin.json"
);
pub const MINECRAFT_BUNGEE: &str = concat!(
    "https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/",
    "minecraft-bungee-plugin.json"
);

pub const MINECRAFT_FABRIC_MOD: &str = concat!(
    "https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/",
    "minecraft-fabric-mod.json"
);
pub const MINECRAFT_MIXINS: &str = concat!(
    "https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/",
    "minecraft-mixins.json"
);

pub const UNITY_ASMDEF: &str = concat!(
    "https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/",
    "unity-asmdef.json"
);
pub const UNITY_ASMREF: &str = concat!(
    "https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/",
    "unity-asmref.json"
);

/// The strict definitions of every Kubernetes object (`kubernetes::VERSION`).
pub const KUBERNETES: &str = "https://raw.githubusercontent.com/kubenote/kubernetes-schema/v1.34.1/standalone-strict/_definitions.json";

pub const OPENAPI_30: &str = "https://spec.openapis.org/oas/3.0/schema/2021-09-28";
pub const OPENAPI_31: &str = "https://spec.openapis.org/oas/3.1/schema/2022-10-07";

pub static BUNDLED: &[Bundled] = &[
    bundled!(
        "openapi-3.0.json",
        [
            "https://spec.openapis.org/oas/3.0/schema/2021-09-28",
            "https://spec.openapis.org/oas/3.0/schema/2019-04-02"
        ]
    ),
    bundled!(
        "openapi-3.1.json",
        [
            "https://spec.openapis.org/oas/3.1/schema/2022-10-07",
            "https://spec.openapis.org/oas/3.1/schema/2022-02-27"
        ]
    ),
    bundled!("composer.json", ["https://getcomposer.org/schema.json"]),
    bundled!(
        "package.json",
        [
            "https://json.schemastore.org/package.json",
            "https://www.schemastore.org/package.json"
        ]
    ),
    bundled!(
        "tsconfig.json",
        [
            "https://json.schemastore.org/tsconfig.json",
            "https://json.schemastore.org/tsconfig",
            "https://www.schemastore.org/tsconfig.json"
        ]
    ),
    bundled!(
        "jsconfig.json",
        [
            "https://json.schemastore.org/jsconfig.json",
            "https://www.schemastore.org/jsconfig.json"
        ]
    ),
    bundled!(
        "eslintrc.json",
        [
            "https://json.schemastore.org/eslintrc.json",
            "https://www.schemastore.org/eslintrc.json"
        ]
    ),
    bundled!(
        "prettierrc.json",
        [
            "https://json.schemastore.org/prettierrc.json",
            "https://www.schemastore.org/prettierrc.json"
        ]
    ),
    bundled!(
        "compose-spec.json",
        [
            "https://raw.githubusercontent.com/compose-spec/compose-spec/master/schema/compose-spec.json",
            "https://raw.githubusercontent.com/compose-spec/compose-go/master/schema/compose-spec.json",
            "https://raw.githubusercontent.com/compose-spec/compose-spec/master/schema/compose_spec.json"
        ]
    ),
    bundled!(
        "github-workflow.json",
        [
            "https://json.schemastore.org/github-workflow.json",
            "https://www.schemastore.org/github-workflow.json"
        ]
    ),
    bundled!(
        "github-action.json",
        [
            "https://json.schemastore.org/github-action.json",
            "https://www.schemastore.org/github-action.json"
        ]
    ),
    bundled!(
        "gitlab-ci.json",
        [
            "https://gitlab.com/gitlab-org/gitlab-foss/-/raw/master/app/assets/javascripts/editor/schema/ci.json",
            "https://gitlab.com/gitlab-org/gitlab/-/raw/master/app/assets/javascripts/editor/schema/ci.json",
            "https://json.schemastore.org/gitlab-ci.json",
            "https://gitlab.com/.gitlab-ci.yml"
        ]
    ),
    bundled!(
        "kubernetes-definitions.json",
        ["https://raw.githubusercontent.com/kubenote/kubernetes-schema/v1.34.1/standalone-strict/_definitions.json"]
    ),
    // Written for Forge (JavaForge's Minecraft plugin support).
    bundled!(
        "minecraft-bukkit-plugin.json",
        ["https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/minecraft-bukkit-plugin.json"]
    ),
    bundled!(
        "minecraft-paper-plugin.json",
        ["https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/minecraft-paper-plugin.json"]
    ),
    bundled!(
        "minecraft-velocity-plugin.json",
        ["https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/minecraft-velocity-plugin.json"]
    ),
    bundled!(
        "minecraft-bungee-plugin.json",
        ["https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/minecraft-bungee-plugin.json"]
    ),
    bundled!(
        "minecraft-fabric-mod.json",
        ["https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/minecraft-fabric-mod.json"]
    ),
    bundled!(
        "minecraft-mixins.json",
        ["https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/minecraft-mixins.json"]
    ),
    // Written for Forge (DotNetForge's Unity support).
    bundled!(
        "unity-asmdef.json",
        ["https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/unity-asmdef.json"]
    ),
    bundled!(
        "unity-asmref.json",
        ["https://raw.githubusercontent.com/Reeperk/Forge/main/crates/forge-jsonschema/schemas/unity-asmref.json"]
    ),
];

/// UI path of bundled schemas: `json-schema://bundled/<file>`.
pub const LOCATION_PREFIX: &str = "json-schema://bundled/";

/// The bundled schema published under `uri`.
pub fn find(uri: &str) -> Option<&'static Bundled> {
    let d = crate::uri::document(uri);
    let d = d
        .strip_prefix("http://")
        .map_or(d.to_string(), |r| format!("https://{r}"));
    BUNDLED.iter().find(|b| b.uris.contains(&d.as_str()))
}

/// The bundled schema of a `json-schema://bundled/<file>` location.
pub fn by_location(location: &str) -> Option<&'static Bundled> {
    let file = location.strip_prefix(LOCATION_PREFIX)?;
    BUNDLED.iter().find(|b| b.file == file)
}

impl Bundled {
    pub fn uri(&self) -> &'static str {
        self.uris[0]
    }

    pub fn location(&self) -> String {
        format!("{LOCATION_PREFIX}{}", self.file)
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        flate2::read::DeflateDecoder::new(self.data)
            .read_to_string(&mut out)
            .expect("bundled schema");
        out
    }

    /// The parsed schema (parsed once).
    pub fn doc(&self) -> Arc<SchemaDoc> {
        static DOCS: OnceLock<Mutex<HashMap<&'static str, Arc<SchemaDoc>>>> = OnceLock::new();
        let docs = DOCS.get_or_init(Default::default);
        if let Some(d) = docs.lock().expect("bundled lock").get(self.file) {
            return d.clone();
        }
        let doc = Arc::new(
            SchemaDoc::parse(self.uri(), &self.location(), self.text())
                .expect("bundled schema parses"),
        );
        docs.lock()
            .expect("bundled lock")
            .insert(self.file, doc.clone());
        doc
    }
}

/// Add a bundled schema (and the URLs it is known under) to a registry.
pub fn load_into(reg: &mut crate::schema::Registry, b: &Bundled) {
    let doc = b.doc();
    let uri = doc.uri.clone();
    reg.insert(doc);
    for alias in &b.uris[1..] {
        reg.alias(alias, &uri);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_schema_parses() {
        for b in BUNDLED {
            let doc = b.doc();
            assert!(doc.root.as_object().is_some(), "{}", b.file);
            assert_eq!(by_location(&b.location()).unwrap().file, b.file);
        }
        assert_eq!(
            find("http://json.schemastore.org/package.json#")
                .unwrap()
                .file,
            "package.json"
        );
        assert_eq!(find(COMPOSER).unwrap().file, "composer.json");
        assert!(find("https://example.com/x.json").is_none());
    }
}
