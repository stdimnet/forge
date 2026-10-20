//! Print what the analysis finds in a project: `cargo run --example dump -- <root>`.

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let root = std::path::Path::new(&root);
    let started = std::time::Instant::now();
    let mut l = phpforge_fw_laravel::Laravel::load(root);
    println!("loaded in {:?}: {:?}", started.elapsed(), l.info());
    for r in &l.routes {
        println!(
            "{:<10} {:<40} {:<24} {} [{}]",
            r.method_label(),
            r.uri,
            r.name.as_deref().unwrap_or(""),
            r.action,
            r.middleware.join(", ")
        );
    }
    for m in &l.models {
        let attrs: Vec<String> = m
            .attributes
            .iter()
            .map(|a| format!("{}: {}", a.name, a.type_name))
            .collect();
        println!("model {} ({}) {}", m.class, m.table, attrs.join(", "));
    }
    let keys: Vec<&str> = l.config.iter().take(12).map(|c| c.key.as_str()).collect();
    println!("config: {} keys, e.g. {keys:?}", l.config.len());
    println!(
        "views: {:?}",
        l.views.iter().map(|v| &v.name).collect::<Vec<_>>()
    );
    let php = std::env::var("PHP").unwrap_or_else(|_| "php".into());
    if phpforge_fw_laravel::artisan::is_runnable(root) {
        let json = phpforge_fw_laravel::artisan::capture(
            root,
            &php,
            &["route:list", "--json"],
            std::time::Duration::from_secs(30),
        );
        if let Some(json) = json {
            println!("route:list accepted: {}", l.use_route_list_json(&json));
            for r in &l.routes {
                println!(
                    "  {} {} {:?} src={:?}",
                    r.method_label(),
                    r.uri,
                    r.name,
                    r.source.as_ref().map(|s| (&s.path, s.line))
                );
            }
        }
        let (cmds, from_project) = phpforge_fw_laravel::artisan::list_commands(root, &php);
        println!(
            "artisan: {} commands (from project: {from_project})",
            cmds.len()
        );
    }
}
