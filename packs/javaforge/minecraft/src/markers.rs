//! Gutter markers in Java and Kotlin sources of Minecraft plugins: event handler methods
//! (`@EventHandler` of Bukkit / BungeeCord, `@Subscribe` of Velocity, `@SubscribeEvent` of
//! NeoForge mods) with the event they
//! listen to, and the plugin's main class (`extends JavaPlugin`), which runs the test server.
//! A line scanner, not a parser: comments and strings are skipped well enough for gutters.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerKind {
    /// An event handler method.
    Listener,
    /// The main class of a Bukkit-style plugin.
    Plugin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    /// 0-based line of the method or class declaration.
    pub line: u32,
    pub kind: MarkerKind,
    pub title: String,
    /// The event class (listeners) or the class name (plugins).
    pub name: String,
}

const HANDLER_ANNOTATIONS: &[&str] = &["@EventHandler", "@Subscribe", "@SubscribeEvent"];
/// How many lines after an annotation the method may start.
const LOOKAHEAD: usize = 6;

fn strip_line_comment(line: &str) -> &str {
    match line.find("//") {
        Some(i) => &line[..i],
        None => line,
    }
}

fn is_annotation_line(line: &str, name: &str) -> bool {
    line.split_whitespace().any(|w| {
        w == name || w.starts_with(&format!("{name}(")) || w.starts_with(&format!("{name}@"))
    })
}

/// The type of the first parameter of a method declared on `line`: Java `void on(Type e`,
/// Kotlin `fun on(e: Type`. Generic and qualified names are reduced to the simple name.
fn first_parameter_type(line: &str) -> Option<String> {
    let open = line.find('(')?;
    let head = &line[..open];
    let params = &line[open + 1..];
    let first = params.split([',', ')']).next()?.trim();
    let ty = if head.contains("fun ") {
        first.split_once(':')?.1.trim()
    } else {
        // Drop annotations (`@NotNull`) and modifiers (`final`); the type is the
        // second-to-last word.
        let words: Vec<&str> = first
            .split_whitespace()
            .filter(|w| !w.starts_with('@') && *w != "final")
            .collect();
        if words.len() < 2 {
            return None;
        }
        words[words.len() - 2]
    };
    let ty = ty.split('<').next()?.trim_end_matches('?');
    let simple = ty.rsplit('.').next()?.trim();
    (!simple.is_empty() && simple.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .then(|| simple.to_string())
}

/// The text after leading annotations (`@EventHandler(priority = HIGH) public void` →
/// `public void`).
fn strip_annotations(mut line: &str) -> &str {
    while let Some(rest) = line.strip_prefix('@') {
        let name_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(rest.len());
        let mut after = &rest[name_end..];
        if after.starts_with('(') {
            let mut depth = 0;
            let mut end = after.len();
            for (k, c) in after.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = k + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            after = &after[end..];
        }
        line = after.trim_start();
    }
    line
}

fn is_method_line(line: &str, kotlin: bool) -> bool {
    if !line.contains('(') {
        return false;
    }
    if kotlin {
        line.contains("fun ")
    } else {
        let head = &line[..line.find('(').unwrap_or(0)];
        let words: Vec<&str> = head.split_whitespace().collect();
        // `public void onJoin(` or `void onJoin(`: a return type and a name.
        words.len() >= 2
            && !matches!(
                words.last().copied(),
                Some("if" | "for" | "while" | "switch" | "catch" | "return" | "new")
            )
    }
}

/// The class a plugin main class extends: `JavaPlugin` (Bukkit, Paper, Spigot) or `Plugin`
/// (BungeeCord), on a class declaration line.
fn plugin_class(line: &str, kotlin: bool) -> Option<String> {
    let words: Vec<&str> = line
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':' || c == '('))
        .filter(|w| !w.is_empty())
        .collect();
    let class_at = words.iter().position(|w| *w == "class")?;
    let name = words.get(class_at + 1)?.trim_end_matches([':', '(']);
    let extends_java_plugin = if kotlin {
        line.contains(": JavaPlugin()") || line.contains(":JavaPlugin()")
    } else {
        line.contains("extends JavaPlugin")
    };
    extends_java_plugin.then(|| name.to_string())
}

/// The markers of a source file; `kotlin` for `.kt` files.
pub fn markers(text: &str, kotlin: bool) -> Vec<Marker> {
    let lines: Vec<&str> = text.lines().map(strip_line_comment).collect();
    let mut out = Vec::new();
    let mut in_block_comment = false;
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if in_block_comment {
            if line.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if line.starts_with("/*") && !line.contains("*/") {
            in_block_comment = true;
            continue;
        }
        if let Some(name) = plugin_class(line, kotlin) {
            out.push(Marker {
                line: i as u32,
                kind: MarkerKind::Plugin,
                title: "Run Paper Server".into(),
                name,
            });
            continue;
        }
        if !HANDLER_ANNOTATIONS
            .iter()
            .any(|a| is_annotation_line(line, a))
        {
            continue;
        }
        // The method is on this line (`@EventHandler public void on(...)`) or below.
        let found = (i..lines.len().min(i + LOOKAHEAD + 1)).find_map(|j| {
            let decl = strip_annotations(lines[j].trim());
            if is_method_line(decl, kotlin) {
                first_parameter_type(decl).map(|event| (j, event))
            } else {
                None
            }
        });
        if let Some((j, event)) = found {
            if out.iter().any(|m: &Marker| m.line == j as u32) {
                continue;
            }
            out.push(Marker {
                line: j as u32,
                kind: MarkerKind::Listener,
                title: format!("Listens to {event}"),
                name: event,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_event_handlers_and_main_class() {
        let src = r#"package demo;

public final class Demo extends JavaPlugin implements Listener {

    @EventHandler(priority = EventPriority.HIGH, ignoreCancelled = true)
    public void onJoin(final PlayerJoinEvent event) {
    }

    // @EventHandler
    public void notAHandler(PlayerQuitEvent event) {}

    @EventHandler
    @SuppressWarnings("unused")
    void onChat(@NotNull io.papermc.paper.event.player.AsyncChatEvent event) {
    }

    /*
    @EventHandler
    public void commented(BlockBreakEvent e) {}
    */

    @Subscribe public void onLogin(PostLoginEvent event) {}
}
"#;
        let m = markers(src, false);
        assert_eq!(
            m.iter()
                .map(|m| (m.line, m.kind, m.name.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (2, MarkerKind::Plugin, "Demo"),
                (5, MarkerKind::Listener, "PlayerJoinEvent"),
                (13, MarkerKind::Listener, "AsyncChatEvent"),
                (21, MarkerKind::Listener, "PostLoginEvent"),
            ]
        );
        assert_eq!(m[1].title, "Listens to PlayerJoinEvent");
    }

    #[test]
    fn kotlin_event_handlers_and_main_class() {
        let src = r#"package demo

class Demo : JavaPlugin() {
}

class JoinListener : Listener {
    @EventHandler
    fun onPlayerJoin(event: PlayerJoinEvent) {
    }

    @Subscribe
    fun onLogin(event: com.velocitypowered.api.event.connection.PostLoginEvent) {}
}
"#;
        let m = markers(src, true);
        assert_eq!(
            m.iter()
                .map(|m| (m.line, m.name.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, "Demo"), (7, "PlayerJoinEvent"), (11, "PostLoginEvent")]
        );
    }

    #[test]
    fn generated_sources_have_markers() {
        let files = crate::generate::generate("demo", &Default::default()).unwrap();
        let main = files
            .iter()
            .find(|f| f.path.ends_with("/Demo.java"))
            .unwrap();
        assert_eq!(markers(&main.contents, false)[0].kind, MarkerKind::Plugin);
        let listener = files
            .iter()
            .find(|f| f.path.ends_with("JoinListener.java"))
            .unwrap();
        assert_eq!(
            markers(&listener.contents, false)[0].name,
            "PlayerJoinEvent"
        );
    }

    #[test]
    fn neoforge_subscribers() {
        let src = "public class Events {\n    @SubscribeEvent\n    public static void onServerStarting(ServerStartingEvent event) {\n    }\n}\n";
        let m = markers(src, false);
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].line, m[0].name.as_str()), (2, "ServerStartingEvent"));
    }
}
