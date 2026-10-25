//! Symfony events: listeners from `EventSubscriberInterface::getSubscribedEvents()`,
//! `#[AsEventListener]` attributes (on classes and methods) and `kernel.event_listener` tags
//! in `config/services*.yaml`; the framework's event names (`kernel.request`,
//! `KernelEvents::REQUEST`, `RequestEvent`); the application's event classes; completion
//! of event names and the data for gutter navigation between events and listeners.

use std::path::Path;

use phpforge_lang_php::{FileIndex, SymbolKind};
use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::complete::{CompletionItem, CompletionKind, CompletionList, FileKind};
use crate::phparray::{array_after, PhpValue};
use crate::strings::{call_context, string_at};
use crate::{Location, PhpFile, Snapshot};

/// Framework events: (name, constant, event class).
pub const KNOWN_EVENTS: &[(&str, &str, &str)] = &[
    (
        "kernel.request",
        "Symfony\\Component\\HttpKernel\\KernelEvents::REQUEST",
        "Symfony\\Component\\HttpKernel\\Event\\RequestEvent",
    ),
    (
        "kernel.controller",
        "Symfony\\Component\\HttpKernel\\KernelEvents::CONTROLLER",
        "Symfony\\Component\\HttpKernel\\Event\\ControllerEvent",
    ),
    (
        "kernel.controller_arguments",
        "Symfony\\Component\\HttpKernel\\KernelEvents::CONTROLLER_ARGUMENTS",
        "Symfony\\Component\\HttpKernel\\Event\\ControllerArgumentsEvent",
    ),
    (
        "kernel.view",
        "Symfony\\Component\\HttpKernel\\KernelEvents::VIEW",
        "Symfony\\Component\\HttpKernel\\Event\\ViewEvent",
    ),
    (
        "kernel.response",
        "Symfony\\Component\\HttpKernel\\KernelEvents::RESPONSE",
        "Symfony\\Component\\HttpKernel\\Event\\ResponseEvent",
    ),
    (
        "kernel.finish_request",
        "Symfony\\Component\\HttpKernel\\KernelEvents::FINISH_REQUEST",
        "Symfony\\Component\\HttpKernel\\Event\\FinishRequestEvent",
    ),
    (
        "kernel.terminate",
        "Symfony\\Component\\HttpKernel\\KernelEvents::TERMINATE",
        "Symfony\\Component\\HttpKernel\\Event\\TerminateEvent",
    ),
    (
        "kernel.exception",
        "Symfony\\Component\\HttpKernel\\KernelEvents::EXCEPTION",
        "Symfony\\Component\\HttpKernel\\Event\\ExceptionEvent",
    ),
    (
        "console.command",
        "Symfony\\Component\\Console\\ConsoleEvents::COMMAND",
        "Symfony\\Component\\Console\\Event\\ConsoleCommandEvent",
    ),
    (
        "console.signal",
        "Symfony\\Component\\Console\\ConsoleEvents::SIGNAL",
        "Symfony\\Component\\Console\\Event\\ConsoleSignalEvent",
    ),
    (
        "console.terminate",
        "Symfony\\Component\\Console\\ConsoleEvents::TERMINATE",
        "Symfony\\Component\\Console\\Event\\ConsoleTerminateEvent",
    ),
    (
        "console.error",
        "Symfony\\Component\\Console\\ConsoleEvents::ERROR",
        "Symfony\\Component\\Console\\Event\\ConsoleErrorEvent",
    ),
    (
        "form.pre_submit",
        "Symfony\\Component\\Form\\FormEvents::PRE_SUBMIT",
        "Symfony\\Component\\Form\\Event\\PreSubmitEvent",
    ),
    (
        "form.submit",
        "Symfony\\Component\\Form\\FormEvents::SUBMIT",
        "Symfony\\Component\\Form\\Event\\SubmitEvent",
    ),
    (
        "form.post_submit",
        "Symfony\\Component\\Form\\FormEvents::POST_SUBMIT",
        "Symfony\\Component\\Form\\Event\\PostSubmitEvent",
    ),
    (
        "form.pre_set_data",
        "Symfony\\Component\\Form\\FormEvents::PRE_SET_DATA",
        "Symfony\\Component\\Form\\Event\\PreSetDataEvent",
    ),
    (
        "form.post_set_data",
        "Symfony\\Component\\Form\\FormEvents::POST_SET_DATA",
        "Symfony\\Component\\Form\\Event\\PostSetDataEvent",
    ),
    (
        "security.authentication.success",
        "Symfony\\Component\\Security\\Core\\AuthenticationEvents::AUTHENTICATION_SUCCESS",
        "Symfony\\Component\\Security\\Core\\Event\\AuthenticationSuccessEvent",
    ),
    (
        "security.interactive_login",
        "Symfony\\Component\\Security\\Http\\SecurityEvents::INTERACTIVE_LOGIN",
        "Symfony\\Component\\Security\\Http\\Event\\InteractiveLoginEvent",
    ),
    (
        "security.switch_user",
        "Symfony\\Component\\Security\\Http\\SecurityEvents::SWITCH_USER",
        "Symfony\\Component\\Security\\Http\\Event\\SwitchUserEvent",
    ),
    (
        "Symfony\\Component\\Security\\Http\\Event\\LoginSuccessEvent",
        "",
        "Symfony\\Component\\Security\\Http\\Event\\LoginSuccessEvent",
    ),
    (
        "Symfony\\Component\\Security\\Http\\Event\\LoginFailureEvent",
        "",
        "Symfony\\Component\\Security\\Http\\Event\\LoginFailureEvent",
    ),
    (
        "Symfony\\Component\\Security\\Http\\Event\\LogoutEvent",
        "",
        "Symfony\\Component\\Security\\Http\\Event\\LogoutEvent",
    ),
    (
        "Symfony\\Component\\Security\\Http\\Event\\CheckPassportEvent",
        "",
        "Symfony\\Component\\Security\\Http\\Event\\CheckPassportEvent",
    ),
    (
        "Symfony\\Component\\Mailer\\Event\\MessageEvent",
        "",
        "Symfony\\Component\\Mailer\\Event\\MessageEvent",
    ),
    (
        "Symfony\\Component\\Messenger\\Event\\WorkerMessageFailedEvent",
        "",
        "Symfony\\Component\\Messenger\\Event\\WorkerMessageFailedEvent",
    ),
    (
        "Symfony\\Component\\Workflow\\Event\\TransitionEvent",
        "",
        "Symfony\\Component\\Workflow\\Event\\TransitionEvent",
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListenerSource {
    Subscriber,
    Attribute,
    Yaml,
}

/// One listener method of an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventListener {
    /// `kernel.request`, or the event class for class-named events.
    pub event: String,
    /// Event class, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_class: Option<String>,
    pub class: String,
    pub method: String,
    #[serde(default)]
    pub priority: i64,
    pub source: ListenerSource,
    /// The listener method (else its class or YAML definition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// Where the event is named (the array key, attribute or tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_at: Option<Location>,
}

/// An event class or event name constant of the application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    pub location: Location,
}

/// Event name of `KernelEvents::REQUEST`, `RequestEvent::class` or a string, with the
/// event class when known. `idx` resolves names in the file that wrote them.
pub fn resolve_event(
    expr: &PhpValue,
    idx: &FileIndex,
    offset: usize,
    project: &[PhpFile],
) -> Option<(String, Option<String>)> {
    match expr {
        PhpValue::Str(s) => {
            let class = KNOWN_EVENTS
                .iter()
                .find(|e| e.0 == s)
                .map(|e| e.2.to_string());
            Some((s.clone(), class))
        }
        PhpValue::Expr(e) => {
            let e = e.trim();
            if let Some(name) = e.strip_suffix("::class") {
                let fqn = idx
                    .resolve_class(name.trim(), offset)
                    .trim_start_matches('\\')
                    .to_string();
                // `RequestEvent::class` is an alias of `kernel.request`.
                if let Some(k) = KNOWN_EVENTS.iter().find(|k| k.2 == fqn && !k.1.is_empty()) {
                    return Some((k.0.to_string(), Some(fqn)));
                }
                return Some((fqn.clone(), Some(fqn)));
            }
            let (class, constant) = e.split_once("::")?;
            let fqn = idx
                .resolve_class(class.trim(), offset)
                .trim_start_matches('\\')
                .to_string();
            let full = format!("{fqn}::{}", constant.trim());
            if let Some(k) = KNOWN_EVENTS.iter().find(|k| k.1 == full) {
                return Some((k.0.to_string(), Some(k.2.to_string())));
            }
            // A constant of an application class: `OrderEvents::PLACED = 'order.placed'`.
            for f in project {
                for c in f.index.classes.iter().filter(|c| c.fqn == fqn) {
                    if let Some(m) = c.member(SymbolKind::Constant, constant.trim()) {
                        if let Some(v) = &m.value {
                            let v = v.trim().trim_matches(['\'', '"']).to_string();
                            return Some((v, None));
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// `kernel.request` -> `onKernelRequest` (Symfony's default listener method).
pub fn default_method(event: &str) -> String {
    let short = event.rsplit('\\').next().unwrap_or(event);
    let mut out = String::from("on");
    let mut upper = true;
    for c in short.chars() {
        if c == '.' || c == '_' || c == '-' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn method_location(f: &PhpFile, class: &str, method: &str) -> Option<Location> {
    let c = f.index.classes.iter().find(|c| c.fqn == class)?;
    let m = c.method(method)?;
    Some(Location::new(
        &f.path,
        m.name_range.line,
        m.name_range.column,
    ))
}

/// Listeners defined in PHP (subscribers and attributes).
pub fn php_listeners(php: &[PhpFile], project: &[PhpFile]) -> Vec<EventListener> {
    let mut out = Vec::new();
    for f in php {
        for c in &f.index.classes {
            // EventSubscriberInterface::getSubscribedEvents()
            if let Some(m) = c.method("getSubscribedEvents") {
                let (s, e) = (m.range.start, m.range.end.min(f.source.len()));
                if let Some((_, arr)) = array_after(&f.source, s, e, "return") {
                    for item in arr.items() {
                        let Some(key) = &item.key else { continue };
                        let Some((event, event_class)) =
                            resolve_event(key, &f.index, item.offset, project)
                        else {
                            continue;
                        };
                        let (l, col) = crate::line_col(&f.source, item.offset);
                        let declared_at = Some(Location::new(&f.path, l, col));
                        // 'method' | ['method', prio] | [['m1', prio], ['m2']]
                        let mut methods: Vec<(String, i64)> = Vec::new();
                        match &item.value {
                            PhpValue::Str(name) => methods.push((name.clone(), 0)),
                            PhpValue::Array(list) => {
                                if let Some(first) = list.first().and_then(|i| i.value.as_str()) {
                                    let prio = match list.get(1).map(|i| &i.value) {
                                        Some(PhpValue::Int(p)) => *p,
                                        _ => 0,
                                    };
                                    methods.push((first.to_string(), prio));
                                } else {
                                    for entry in list {
                                        let items = entry.value.items();
                                        if let Some(name) =
                                            items.first().and_then(|i| i.value.as_str())
                                        {
                                            let prio = match items.get(1).map(|i| &i.value) {
                                                Some(PhpValue::Int(p)) => *p,
                                                _ => 0,
                                            };
                                            methods.push((name.to_string(), prio));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        for (method, priority) in methods {
                            out.push(EventListener {
                                location: method_location(f, &c.fqn, &method)
                                    .or_else(|| declared_at.clone()),
                                event: event.clone(),
                                event_class: event_class.clone(),
                                class: c.fqn.clone(),
                                method,
                                priority,
                                source: ListenerSource::Subscriber,
                                declared_at: declared_at.clone(),
                            });
                        }
                    }
                }
            }
            // #[AsEventListener] on the class and on methods.
            let attrs = c.attributes.iter().map(|a| (a, None)).chain(
                c.methods()
                    .flat_map(|m| m.attributes.iter().map(move |a| (a, Some(m)))),
            );
            for (attr, on_method) in attrs {
                if !attr.fqn.ends_with("AsEventListener") && attr.name != "AsEventListener" {
                    continue;
                }
                let arg = |name: &str, pos: usize| {
                    attr.args
                        .iter()
                        .find(|a| a.name.as_deref() == Some(name))
                        .or_else(|| attr.args.iter().filter(|a| a.name.is_none()).nth(pos))
                        .map(|a| a.value.trim().to_string())
                };
                let to_value = |raw: &str| {
                    let t = raw.trim();
                    if (t.starts_with('\'') && t.ends_with('\''))
                        || (t.starts_with('"') && t.ends_with('"'))
                    {
                        PhpValue::Str(t[1..t.len() - 1].to_string())
                    } else {
                        PhpValue::Expr(t.to_string())
                    }
                };
                let method_name = arg("method", 1)
                    .map(|m| m.trim_matches(['\'', '"']).to_string())
                    .or_else(|| on_method.map(|m| m.name.clone()));
                let event = arg("event", 0)
                    .and_then(|e| resolve_event(&to_value(&e), &f.index, attr.range.start, project))
                    .or_else(|| {
                        // The event class from the method's first parameter.
                        let m = on_method.or_else(|| {
                            method_name
                                .as_deref()
                                .and_then(|n| c.method(n))
                                .or_else(|| c.method("__invoke"))
                        })?;
                        let ty = m.params.first()?.type_hint.clone()?;
                        let ty = ty.trim_start_matches('?');
                        resolve_event(
                            &PhpValue::Expr(format!("{ty}::class")),
                            &f.index,
                            attr.range.start,
                            project,
                        )
                    });
                let Some((event, event_class)) = event else {
                    continue;
                };
                let method = method_name.unwrap_or_else(|| {
                    if c.method("__invoke").is_some() {
                        "__invoke".into()
                    } else {
                        default_method(&event)
                    }
                });
                let priority = arg("priority", 2).and_then(|p| p.parse().ok()).unwrap_or(0);
                out.push(EventListener {
                    location: method_location(f, &c.fqn, &method).or_else(|| {
                        Some(Location::new(
                            &f.path,
                            c.name_range.line,
                            c.name_range.column,
                        ))
                    }),
                    declared_at: Some(Location::new(&f.path, attr.range.line, attr.range.column)),
                    event,
                    event_class,
                    class: c.fqn.clone(),
                    method,
                    priority,
                    source: ListenerSource::Attribute,
                });
            }
        }
    }
    out
}

/// `kernel.event_listener` tags in `config/services*.yaml`.
pub fn yaml_listeners(root: &Path, php: &[PhpFile]) -> Vec<EventListener> {
    let mut out = Vec::new();
    let mut files = vec![root.join("config/services.yaml")];
    if let Ok(dir) = std::fs::read_dir(root.join("config/services")) {
        let mut more: Vec<_> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "yaml" || e == "yml"))
            .collect();
        more.sort();
        files.extend(more);
    }
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(docs) = YamlLoader::load_from_str(&text) else {
            continue;
        };
        let rel = crate::rel_path(root, &file);
        for doc in &docs {
            let Some(Yaml::Hash(services)) = (match doc {
                Yaml::Hash(h) => h.get(&Yaml::String("services".into())),
                _ => None,
            }) else {
                continue;
            };
            for (id, def) in services {
                let Some(id) = id.as_str() else { continue };
                let Yaml::Hash(def) = def else { continue };
                let class = def
                    .get(&Yaml::String("class".into()))
                    .and_then(|c| c.as_str())
                    .unwrap_or(id)
                    .trim_start_matches('\\')
                    .to_string();
                let Some(Yaml::Array(tags)) = def.get(&Yaml::String("tags".into())) else {
                    continue;
                };
                for tag in tags {
                    let Yaml::Hash(t) = tag else { continue };
                    let get = |k: &str| t.get(&Yaml::String(k.into()));
                    if get("name").and_then(|n| n.as_str()) != Some("kernel.event_listener") {
                        continue;
                    }
                    let Some(event) = get("event").and_then(|e| e.as_str()).map(String::from)
                    else {
                        continue;
                    };
                    let method = get("method")
                        .and_then(|m| m.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| {
                            let has_invoke = php.iter().any(|f| {
                                f.index
                                    .classes
                                    .iter()
                                    .any(|c| c.fqn == class && c.method("__invoke").is_some())
                            });
                            if has_invoke {
                                "__invoke".into()
                            } else {
                                default_method(&event)
                            }
                        });
                    let priority = get("priority").and_then(|p| p.as_i64()).unwrap_or(0);
                    let line = crate::container::key_line(&text, id, None, 0).unwrap_or(0);
                    let declared = Location::new(&rel, line, 0);
                    let location = php
                        .iter()
                        .find_map(|f| method_location(f, &class, &method))
                        .unwrap_or_else(|| declared.clone());
                    out.push(EventListener {
                        event_class: KNOWN_EVENTS
                            .iter()
                            .find(|k| k.0 == event)
                            .map(|k| k.2.to_string()),
                        event,
                        class: class.clone(),
                        method,
                        priority,
                        source: ListenerSource::Yaml,
                        location: Some(location),
                        declared_at: Some(declared),
                    });
                }
            }
        }
    }
    out
}

/// Event classes of the application (extending an `Event` base class) and string
/// constants of `*Events` classes.
pub fn event_defs(php: &[PhpFile]) -> Vec<EventDef> {
    let mut out = Vec::new();
    for f in php {
        for c in &f.index.classes {
            let is_event = c.extends.iter().any(|e| {
                e.ends_with("EventDispatcher\\Event")
                    || e.ends_with("Contracts\\EventDispatcher\\Event")
                    || e == "Event"
            }) || (c.name.ends_with("Event")
                && c.kind == SymbolKind::Class
                && f.path.contains("/Event/"));
            if is_event {
                out.push(EventDef {
                    name: c.fqn.clone(),
                    class: Some(c.fqn.clone()),
                    location: Location::new(&f.path, c.name_range.line, c.name_range.column),
                });
            }
            if c.name.ends_with("Events") {
                for m in c.members.iter().filter(|m| m.kind == SymbolKind::Constant) {
                    if let Some(v) = &m.value {
                        let v = v.trim();
                        if v.starts_with(['\'', '"']) {
                            out.push(EventDef {
                                name: v.trim_matches(['\'', '"']).to_string(),
                                class: None,
                                location: Location::new(
                                    &f.path,
                                    m.name_range.line,
                                    m.name_range.column,
                                ),
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

/// All listeners of a project.
pub fn listeners(root: &Path, php: &[PhpFile]) -> Vec<EventListener> {
    let mut out = php_listeners(php, php);
    out.extend(yaml_listeners(root, php));
    out.sort_by(|a, b| {
        a.event
            .cmp(&b.event)
            .then(b.priority.cmp(&a.priority))
            .then(a.class.cmp(&b.class))
    });
    out
}

/// Whether the string at `offset` names an event.
fn event_string_at(path: &str, text: &str, offset: usize) -> Option<(usize, String)> {
    match FileKind::of(path) {
        FileKind::Yaml => {
            let ls = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let line = &text[ls..offset];
            let at = line.rfind("event:")?;
            let rest = &line[at + 6..];
            let value = rest.trim_start().trim_start_matches(['\'', '"']);
            if value.contains([',', '}', ' ']) {
                return None;
            }
            let from = offset - value.len();
            Some((from, value.to_string()))
        }
        FileKind::Php => {
            let s = string_at(text, offset)?;
            if let Some(call) = call_context(text, s.open) {
                let ok = match call.function.as_str() {
                    "AsEventListener" => {
                        call.named.as_deref() == Some("event")
                            || (call.named.is_none() && call.arg == 0)
                    }
                    "addListener" | "removeListener" | "hasListeners" | "getListeners" => {
                        call.arg == 0
                    }
                    "dispatch" => call.arg == 1,
                    _ => false,
                };
                return ok.then(|| (s.start, s.prefix.clone()));
            }
            // An array key in getSubscribedEvents().
            let before = text[..s.open].trim_end();
            let in_subscriber = text[..s.open].rfind("function ").is_some_and(|f| {
                text[f + 9..]
                    .trim_start()
                    .starts_with("getSubscribedEvents")
            });
            let key = before.ends_with('[') || before.ends_with(',');
            let after = text[s.end..].trim_start_matches(['\'', '"']).trim_start();
            (in_subscriber
                && key
                && (after.starts_with("=>")
                    || after.is_empty()
                    || after.starts_with('\n')
                    || !s.closed))
                .then(|| (s.start, s.prefix.clone()))
        }
        _ => None,
    }
}

/// Completion of event names (framework events and the application's).
pub fn complete(snap: &Snapshot, path: &str, text: &str, offset: usize) -> Option<CompletionList> {
    let offset = offset.min(text.len());
    let (from, prefix) = event_string_at(path, text, offset)?;
    let p = prefix.to_ascii_lowercase();
    let mut items: Vec<CompletionItem> = Vec::new();
    let mut push = |name: &str, class: Option<&str>, info: &str| {
        if name.contains('\\')
            || !name.to_ascii_lowercase().contains(&p)
            || items.iter().any(|i| i.label == name)
        {
            return;
        }
        items.push(CompletionItem {
            label: name.to_string(),
            kind: CompletionKind::Event,
            detail: class
                .map(|c| c.rsplit('\\').next().unwrap_or(c).to_string())
                .unwrap_or_default(),
            info: info.to_string(),
            apply: None,
        });
    };
    for (name, constant, class) in KNOWN_EVENTS {
        push(
            name,
            Some(class),
            constant.rsplit('\\').next().unwrap_or(constant),
        );
    }
    for d in &snap.event_defs {
        push(&d.name, d.class.as_deref(), "");
    }
    for l in &snap.listeners {
        push(&l.event, l.event_class.as_deref(), "");
    }
    items.sort_by(|a, b| {
        let ap = !a.label.to_ascii_lowercase().starts_with(&p);
        let bp = !b.label.to_ascii_lowercase().starts_with(&p);
        ap.cmp(&bp).then(a.label.cmp(&b.label))
    });
    Some(CompletionList { from, items })
}

/// Listeners of an event name or class.
pub fn listeners_of<'a>(
    snap: &'a Snapshot,
    event: &'a str,
) -> impl Iterator<Item = &'a EventListener> + 'a {
    snap.listeners
        .iter()
        .filter(move |l| l.event == event || l.event_class.as_deref() == Some(event))
}

/// Go to Declaration on an event name string (its listeners) or on a listener method name
/// in `getSubscribedEvents()`.
pub fn goto(snap: &Snapshot, path: &str, text: &str, offset: usize) -> Option<Vec<Location>> {
    if FileKind::of(path) != FileKind::Php && FileKind::of(path) != FileKind::Yaml {
        return None;
    }
    if FileKind::of(path) == FileKind::Php {
        let s = string_at(text, offset)?;
        let in_subscriber = text[..s.open].rfind("function ").is_some_and(|f| {
            text[f + 9..]
                .trim_start()
                .starts_with("getSubscribedEvents")
        });
        let before = text[..s.open].trim_end();
        if in_subscriber
            && (before.ends_with("=>") || before.ends_with("=> [") || before.ends_with('['))
            && !text[s.end..]
                .trim_start_matches(['\'', '"'])
                .trim_start()
                .starts_with("=>")
        {
            let idx = phpforge_lang_php::parser::index_source(path, text);
            let class = idx.class_at(s.open)?;
            let m = class.method(&s.value)?;
            return Some(vec![Location::new(
                path,
                m.name_range.line,
                m.name_range.column,
            )]);
        }
    }
    let (from, _) = event_string_at(path, text, offset)?;
    let end = text[from..]
        .find(['\'', '"', '\n', ' ', ',', '}'])
        .map(|e| from + e)
        .unwrap_or(text.len());
    let name = &text[from..end];
    let found: Vec<Location> = listeners_of(snap, name)
        .filter_map(|l| l.location.clone())
        .collect();
    (!found.is_empty()).then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixture, scan, Config};

    const SUBSCRIBER: &str = r#"<?php

namespace App\EventSubscriber;

use App\Event\OrderPlacedEvent;
use Symfony\Component\EventDispatcher\EventSubscriberInterface;
use Symfony\Component\HttpKernel\Event\RequestEvent;
use Symfony\Component\HttpKernel\KernelEvents;

final class LocaleSubscriber implements EventSubscriberInterface
{
    public static function getSubscribedEvents(): array
    {
        return [
            KernelEvents::REQUEST => [['onKernelRequest', 20]],
            OrderPlacedEvent::class => 'onOrderPlaced',
            'kernel.response' => ['onResponse', -10],
        ];
    }

    public function onKernelRequest(RequestEvent $event): void {}
    public function onOrderPlaced(OrderPlacedEvent $event): void {}
    public function onResponse($event): void {}
}
"#;

    const LISTENER: &str = r#"<?php

namespace App\EventListener;

use App\Event\OrderPlacedEvent;
use Symfony\Component\EventDispatcher\Attribute\AsEventListener;
use Symfony\Component\HttpKernel\Event\ExceptionEvent;

#[AsEventListener(event: 'kernel.exception', method: 'onException', priority: 5)]
final class ExceptionListener
{
    public function onException(ExceptionEvent $event): void {}

    #[AsEventListener]
    public function notify(OrderPlacedEvent $event): void {}
}
"#;

    const EVENT: &str = "<?php\nnamespace App\\Event;\nuse Symfony\\Contracts\\EventDispatcher\\Event;\nfinal class OrderPlacedEvent extends Event {}\n";

    fn project() -> tempfile::TempDir {
        let dir = fixture::project();
        let r = dir.path();
        fixture::write(r, "src/EventSubscriber/LocaleSubscriber.php", SUBSCRIBER);
        fixture::write(r, "src/EventListener/ExceptionListener.php", LISTENER);
        fixture::write(r, "src/Event/OrderPlacedEvent.php", EVENT);
        let services = format!(
            "{}\n    App\\Service\\Mailer:\n        tags:\n            - {{ name: kernel.event_listener, event: kernel.terminate }}\n",
            fixture::SERVICES
        );
        fixture::write(r, "config/services.yaml", &services);
        dir
    }

    #[test]
    fn indexes_subscribers_attributes_and_tags() {
        let dir = project();
        let snap = scan(dir.path(), &Config::default());
        let got: Vec<(&str, &str, i64, ListenerSource)> = snap
            .listeners
            .iter()
            .map(|l| (l.event.as_str(), l.method.as_str(), l.priority, l.source))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "App\\Event\\OrderPlacedEvent",
                    "notify",
                    0,
                    ListenerSource::Attribute
                ),
                (
                    "App\\Event\\OrderPlacedEvent",
                    "onOrderPlaced",
                    0,
                    ListenerSource::Subscriber
                ),
                (
                    "kernel.exception",
                    "onException",
                    5,
                    ListenerSource::Attribute
                ),
                (
                    "kernel.request",
                    "onKernelRequest",
                    20,
                    ListenerSource::Subscriber
                ),
                (
                    "kernel.response",
                    "onResponse",
                    -10,
                    ListenerSource::Subscriber
                ),
                (
                    "kernel.terminate",
                    "onKernelTerminate",
                    0,
                    ListenerSource::Yaml
                ),
            ]
        );
        let req = snap
            .listeners
            .iter()
            .find(|l| l.event == "kernel.request")
            .unwrap();
        assert_eq!(
            req.event_class.as_deref(),
            Some("Symfony\\Component\\HttpKernel\\Event\\RequestEvent")
        );
        assert_eq!(req.location.as_ref().unwrap().line, 20);
        assert!(snap
            .event_defs
            .iter()
            .any(|d| d.name == "App\\Event\\OrderPlacedEvent"));
        assert_eq!(
            default_method("kernel.finish_request"),
            "onKernelFinishRequest"
        );
    }

    fn at(src: &str) -> (String, usize) {
        let pos = src.find('¦').unwrap();
        (src.replacen('¦', "", 1), pos)
    }

    #[test]
    fn completes_event_names_and_navigates() {
        let dir = project();
        let snap = scan(dir.path(), &Config::default());
        let (src, pos) = at("<?php\n#[AsEventListener(event: 'kernel.re¦')]");
        let list = complete(&snap, "src/X.php", &src, pos).unwrap();
        let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["kernel.request", "kernel.response"]);
        assert_eq!(list.items[0].detail, "RequestEvent");

        let (src, pos) =
            at("        tags:\n            - { name: kernel.event_listener, event: console.e¦");
        let list = complete(&snap, "config/services.yaml", &src, pos).unwrap();
        assert_eq!(list.items[0].label, "console.error");

        let (src, pos) = at("<?php\nclass S {\n    public static function getSubscribedEvents(): array\n    {\n        return ['kernel.ex¦' => 'x'];\n    }\n}");
        let list = complete(&snap, "src/S.php", &src, pos).unwrap();
        assert_eq!(list.items[0].label, "kernel.exception");

        let (src, pos) = at("$dispatcher->addListener('kernel.requ¦est', $fn);");
        let locs = goto(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(locs[0].path, "src/EventSubscriber/LocaleSubscriber.php");

        let src = SUBSCRIBER.replace("'onOrderPlaced'", "'onOrder¦Placed'");
        let (src, pos) = at(&src);
        let locs = goto(&snap, "src/EventSubscriber/LocaleSubscriber.php", &src, pos).unwrap();
        assert_eq!(locs[0].line, 21);
    }

    #[test]
    fn gutter_marks_link_events_and_listeners() {
        use crate::gutter::{marks, GutterKind};
        let dir = project();
        let snap = scan(dir.path(), &Config::default());
        let m = marks(
            &snap,
            "src/EventSubscriber/LocaleSubscriber.php",
            SUBSCRIBER,
        );
        let l = m
            .iter()
            .find(|m| m.kind == GutterKind::Listener && m.line == 21)
            .unwrap();
        assert_eq!(l.title, "Listens to OrderPlacedEvent");
        assert_eq!(l.targets[0].path, "src/Event/OrderPlacedEvent.php");
        let r = m
            .iter()
            .find(|m| m.kind == GutterKind::Listener && m.line == 20)
            .unwrap();
        assert_eq!(r.title, "Listens to kernel.request (priority 20)");

        let m = marks(&snap, "src/Event/OrderPlacedEvent.php", EVENT);
        assert_eq!(m[0].kind, GutterKind::Event);
        assert_eq!(m[0].targets.len(), 2);
        assert!(
            m[0].title.starts_with("2 listeners of OrderPlacedEvent"),
            "{}",
            m[0].title
        );

        let src = "<?php\nnamespace App\\Service;\nuse App\\Event\\OrderPlacedEvent;\nclass Checkout {\n    public function pay() {\n        $this->dispatcher->dispatch(new OrderPlacedEvent($order));\n    }\n}\n";
        let m = marks(&snap, "src/Service/Checkout.php", src);
        assert_eq!((m[0].kind, m[0].line), (GutterKind::Event, 5));
    }
}
