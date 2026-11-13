//! WebForge analysis of a project (npm tool window, React / Vue / Angular component
//! navigation and line markers), run where its files are.
//!
//! Like [`crate::frameworks`], the analysis reads thousands of script and template files,
//! so it never goes file by file over the network: [`LocalWeb`] loads and caches the
//! project's sources next to them (in the desktop app for a local project, in the Forge
//! agent for a remote one) and answers [`WebCall`]s. Editor requests carry the buffer.
//! Offsets are UTF-16 (JS strings) as in the IPC commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use webforge_lang_web::{import_paths, npm, FrameworkIndex, Sources};

use crate::protocol::{decode_value, encode_value};
use crate::Backend;

/// How long loaded project files are reused before the project is read again.
const SOURCES_TTL: Duration = Duration::from_secs(5);
/// Script, template and manifest files read per project.
const FILE_LIMIT: usize = 8000;

/// A WebForge request. The reply type of each call is noted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WebCall {
    /// Every `package.json` with its scripts and package manager (`package_manager` is the
    /// `web.packageManager` setting); `refresh` reads the project again ->
    /// `Vec<webforge_lang_web::NpmPackage>`
    NpmPackages {
        refresh: bool,
        package_manager: String,
    },
    /// Components of `framework` (`react`, `vue`, `angular`) ->
    /// `Vec<webforge_lang_web::Component>`
    Components { framework: String },
    /// Go to Declaration -> `Vec<webforge_lang_web::Location>`
    Goto {
        framework: String,
        path: String,
        content: String,
        offset: usize,
    },
    /// Line markers of a file -> `Vec<webforge_lang_web::GutterMark>`
    Gutter {
        framework: String,
        path: String,
        content: String,
    },
    /// Go to Declaration on an import specifier (aliases of `tsconfig`, Vite and Webpack)
    /// -> `Option<webforge_lang_web::Location>`
    ImportGoto {
        path: String,
        content: String,
        offset: usize,
    },
    /// Completion of an import specifier ->
    /// `Option<webforge_lang_web::import_paths::ImportCompletion>`
    ImportComplete {
        path: String,
        content: String,
        offset: usize,
    },
    /// The Vite / Webpack aliases of the project -> `Vec<webforge_lang_web::aliases::Alias>`
    Aliases,
}

/// Send `call` to the backend and decode the reply.
pub fn web_call<T: DeserializeOwned>(backend: &dyn Backend, call: WebCall) -> Result<T> {
    decode_value(&backend.web(call)?)
}

/// Run `f` with the index of `framework` (`react`, `vue`, `angular`).
pub fn with_index<T>(
    sources: &Sources,
    framework: &str,
    f: impl FnOnce(&dyn FrameworkIndex) -> T,
) -> Result<T> {
    match framework {
        "react" => Ok(f(&webforge_fw_react::ReactIndex::new(sources))),
        "vue" => Ok(f(&webforge_fw_vue::VueIndex::new(sources))),
        "angular" => Ok(f(&webforge_fw_angular::AngularIndex::new(sources))),
        other => Err(ForgeError::Other(format!("unknown framework {other}"))),
    }
}

/// WebForge analysis of a folder on this machine; the loaded sources are cached briefly.
pub struct LocalWeb {
    root: PathBuf,
    sources: Mutex<Option<(Instant, Arc<Sources>)>>,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

impl LocalWeb {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            sources: Mutex::new(None),
        }
    }

    fn sources(&self, force: bool) -> Result<Arc<Sources>> {
        if !force {
            let guard = self.sources.lock().map_err(err)?;
            if let Some((at, s)) = guard.as_ref() {
                if at.elapsed() < SOURCES_TTL {
                    return Ok(s.clone());
                }
            }
        }
        let loaded = Arc::new(webforge_lang_web::fs::load(&self.root, FILE_LIMIT));
        *self.sources.lock().map_err(err)? = Some((Instant::now(), loaded.clone()));
        Ok(loaded)
    }

    /// Answer `call`, encoded with its reply type.
    pub fn call(&self, call: WebCall) -> Result<Vec<u8>> {
        match call {
            WebCall::NpmPackages {
                refresh,
                package_manager,
            } => encode_value(&npm::packages(&*self.sources(refresh)?, &package_manager)),
            WebCall::Components { framework } => {
                let sources = self.sources(false)?;
                encode_value(&with_index(&sources, &framework, |i| i.components())?)
            }
            WebCall::Goto {
                framework,
                path,
                content,
                offset,
            } => {
                let sources = self.sources(false)?;
                encode_value(&with_index(&sources, &framework, |i| {
                    i.goto(&path, &content, offset)
                })?)
            }
            WebCall::Gutter {
                framework,
                path,
                content,
            } => {
                let sources = self.sources(false)?;
                encode_value(&with_index(&sources, &framework, |i| {
                    i.gutter(&path, &content)
                })?)
            }
            WebCall::ImportGoto {
                path,
                content,
                offset,
            } => {
                let sources = self.sources(false)?;
                encode_value(&import_paths::goto(&sources, &path, &content, offset))
            }
            WebCall::ImportComplete {
                path,
                content,
                offset,
            } => {
                let sources = self.sources(false)?;
                encode_value(&import_paths::complete(&sources, &path, &content, offset))
            }
            WebCall::Aliases => encode_value(&self.sources(false)?.bundler_aliases().to_vec()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webforge_lang_web::{Component, NpmPackage};

    #[test]
    fn answers_npm_and_component_calls() {
        let dir = tempfile::tempdir().unwrap();
        let write = |p: &str, c: &str| {
            let path = dir.path().join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, c).unwrap();
        };
        write(
            "package.json",
            r#"{"name":"shop","scripts":{"dev":"vite","build":"vite build"},"dependencies":{"react":"^18"}}"#,
        );
        write("pnpm-lock.yaml", "lockfileVersion: '9.0'\n");
        write(
            "src/Button.tsx",
            "export function Button(props: { label: string }) { return <button>{props.label}</button>; }\n",
        );
        write(
            "src/App.tsx",
            "import { Button } from './Button';\nexport default function App() { return <Button label=\"Hi\" />; }\n",
        );
        let web = LocalWeb::new(dir.path());
        let packages: Vec<NpmPackage> = decode_value(
            &web.call(WebCall::NpmPackages {
                refresh: false,
                package_manager: "auto".into(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].scripts.len(), 2);
        assert_eq!(packages[0].package_manager.manager.id(), "pnpm");

        let components: Vec<Component> = decode_value(
            &web.call(WebCall::Components {
                framework: "react".into(),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(components.iter().any(|c| c.name == "Button"));
        assert!(web
            .call(WebCall::Components {
                framework: "svelte".into()
            })
            .is_err());
    }
}
