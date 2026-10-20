//! The component model shared by the React, Vue and Angular plugins, and the gutter marks
//! and navigation answers they return to the UI.

use serde::{Deserialize, Serialize};

use crate::text::Location;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Framework {
    React,
    Vue,
    Angular,
}

impl Framework {
    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "react" => Some(Framework::React),
            "vue" => Some(Framework::Vue),
            "angular" => Some(Framework::Angular),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    /// React function component (also `memo`/`forwardRef` wrapped).
    Function,
    /// React class component.
    Class,
    /// Vue single-file component.
    SingleFile,
    /// Vue component defined with `defineComponent` in a script file.
    Script,
    /// Angular `@Component`.
    Component,
    /// Angular `@Directive`.
    Directive,
    /// Angular `@Pipe`.
    Pipe,
}

/// A prop / input, or an event / output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub name: String,
    /// Another name templates use (Angular `@Input('alias')`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub location: Location,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_text: Option<String>,
    #[serde(default)]
    pub required: bool,
}

impl Member {
    /// The name templates use for it.
    pub fn public_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// `UserCard` (class name for Angular, file name for Vue single-file components).
    pub name: String,
    pub framework: Framework,
    pub kind: ComponentKind,
    /// Angular selector (`app-user-card`, `[appHighlight]`) or pipe name; Vue tag name
    /// (`user-card`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Where the component is defined (its name).
    pub location: Location,
    #[serde(default)]
    pub props: Vec<Member>,
    #[serde(default)]
    pub events: Vec<Member>,
    /// Template file (Angular `templateUrl`) or inline template start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<Location>,
    /// Style files (`styleUrls`, `styleUrl`).
    #[serde(default)]
    pub styles: Vec<Location>,
    /// Exported as the module's default export.
    #[serde(default)]
    pub default_export: bool,
    /// Number of places it is used in templates and JSX (filled by the index).
    #[serde(default)]
    pub usages: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GutterKind {
    /// Component class -> its template.
    Template,
    /// Component class -> its stylesheets.
    Styles,
    /// Template -> the component class that uses it.
    Component,
    /// Definition -> usages in templates / JSX.
    Usages,
    /// A runnable script (package.json).
    Run,
}

/// A line marker: an icon in the gutter with its targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMark {
    /// 0-based line.
    pub line: u32,
    pub kind: GutterKind,
    pub title: String,
    #[serde(default)]
    pub targets: Vec<Location>,
}
