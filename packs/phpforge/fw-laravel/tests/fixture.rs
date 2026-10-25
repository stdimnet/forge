//! End-to-end analysis of the fixture application in `tests/fixtures/shop`.

use std::path::PathBuf;

use phpforge_fw_laravel::{Laravel, Route};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shop")
}

fn load() -> Laravel {
    Laravel::load(&root())
}

fn route<'a>(l: &'a Laravel, name: &str) -> &'a Route {
    l.routes
        .iter()
        .find(|r| r.name.as_deref() == Some(name))
        .unwrap_or_else(|| {
            panic!(
                "route {name} not found in {:#?}",
                l.routes.iter().map(|r| &r.name).collect::<Vec<_>>()
            )
        })
}

/// Byte offset of `|` in `marked`, and the text without it.
fn caret(marked: &str) -> (String, usize) {
    let at = marked.find('|').unwrap();
    (marked.replacen('|', "", 1), at)
}

#[test]
fn detects_version() {
    assert!(phpforge_fw_laravel::is_laravel(&root()));
    assert_eq!(load().version.as_deref(), Some("12.4.1"));
}

#[test]
fn parses_routes() {
    let l = load();
    let home = route(&l, "home");
    assert_eq!((home.uri.as_str(), home.action.as_str()), ("/", "Closure"));
    assert_eq!(home.methods, vec!["GET", "HEAD"]);
    assert_eq!(home.middleware, vec!["web"]);
    assert_eq!(home.source.as_ref().unwrap().line, 8);

    let about = route(&l, "about");
    assert_eq!(about.action, "view: pages.about");

    let show = route(&l, "orders.show");
    assert_eq!(show.uri, "orders/{order}");
    assert_eq!(show.action, "App\\Http\\Controllers\\OrderController@show");
    assert_eq!(show.middleware, vec!["web", "can:view,order"]);
    let loc = show.action_location.as_ref().unwrap();
    assert_eq!(
        (loc.path.as_str(), loc.line),
        ("app/Http/Controllers/OrderController.php", 14)
    );

    let checkout = route(&l, "checkout");
    assert_eq!(checkout.methods, vec!["GET", "HEAD", "POST"]);

    let profile = l.routes.iter().find(|r| r.uri == "profile").unwrap();
    assert_eq!(
        profile.action,
        "App\\Http\\Controllers\\ShowProfile@__invoke"
    );
    assert_eq!(profile.middleware, vec!["web", "auth", "verified"]);
    assert_eq!(profile.action_location.as_ref().unwrap().line, 6);

    // Group with prefix, name and middleware.
    let dash = route(&l, "admin.dashboard");
    assert_eq!(dash.uri, "admin");
    assert_eq!(dash.middleware, vec!["web", "auth"]);
    assert_eq!(
        dash.controller.as_deref(),
        Some("App\\Http\\Controllers\\Admin\\DashboardController")
    );

    // Resource restricted with only().
    let photos: Vec<&str> = l
        .routes
        .iter()
        .filter(|r| {
            r.name
                .as_deref()
                .is_some_and(|n| n.starts_with("admin.photos."))
        })
        .map(|r| r.uri.as_str())
        .collect();
    assert_eq!(photos, vec!["admin/photos", "admin/photos/{photo}"]);

    // Controller group with string actions.
    let store = route(&l, "orders.store");
    assert_eq!(
        (store.uri.as_str(), store.methods.as_slice()),
        ("shop/orders", &["POST".to_string()][..])
    );
    assert_eq!(store.controller_method.as_deref(), Some("store"));

    // Array group attributes.
    assert_eq!(route(&l, "legacy.report").uri, "legacy/report");

    // Included file.
    let login = route(&l, "login");
    assert_eq!(login.middleware, vec!["web", "guest"]);
    assert_eq!(login.source.as_ref().unwrap().path, "routes/auth.php");

    // API resource with the api prefix and no create/edit.
    let api: Vec<String> = l
        .routes
        .iter()
        .filter(|r| r.uri.starts_with("api/"))
        .map(|r| {
            format!(
                "{} {} {}",
                r.method_label(),
                r.uri,
                r.name.clone().unwrap_or_default()
            )
        })
        .collect();
    assert_eq!(
        api,
        vec![
            "GET|HEAD api/orders orders.index",
            "POST api/orders orders.store",
            "GET|HEAD api/orders/{order} orders.show",
            "PUT|PATCH api/orders/{order} orders.update",
            "DELETE api/orders/{order} orders.destroy",
        ]
    );
    // console.php defines commands, not routes.
    assert!(l
        .routes
        .iter()
        .all(|r| r.source.as_ref().unwrap().path != "routes/console.php"));
}

#[test]
fn reads_route_list_json() {
    let mut l = load();
    let json = r#"[{"domain":null,"method":"GET|HEAD","uri":"orders/{order}","name":"orders.show","action":"App\\Http\\Controllers\\OrderController@show","middleware":["web"]},
                   {"domain":null,"method":"GET|HEAD","uri":"up","name":null,"action":"Closure","middleware":[]}]"#;
    assert!(l.use_route_list_json(json));
    assert_eq!(l.routes_source, "artisan");
    assert_eq!(l.routes.len(), 2);
    let show = route(&l, "orders.show");
    assert_eq!(show.source.as_ref().unwrap().path, "routes/web.php");
    assert_eq!(show.action_location.as_ref().unwrap().line, 14);
    assert!(!l.use_route_list_json("not json"));
}

#[test]
fn lists_views_and_components() {
    let l = load();
    let names: Vec<&str> = l.views.iter().map(|v| v.name.as_str()).collect();
    assert!(
        names.contains(&"orders.show")
            && names.contains(&"layouts.app")
            && names.contains(&"components.forms.input")
    );
    let tags: Vec<&str> = l.components.iter().map(|c| c.tag.as_str()).collect();
    assert_eq!(
        tags,
        vec![
            "livewire:order-counter",
            "x-alert",
            "x-button",
            "x-forms.input"
        ]
    );
    let alert = l.components.iter().find(|c| c.tag == "x-alert").unwrap();
    assert_eq!(alert.props, vec!["type", "message"]);
    let button = l.components.iter().find(|c| c.tag == "x-button").unwrap();
    assert_eq!(
        button.class.as_deref(),
        Some("App\\View\\Components\\Button")
    );
    assert_eq!(
        button.view.as_deref(),
        Some("resources/views/components/button.blade.php")
    );
    assert_eq!(button.props, vec!["variant", "is-disabled"]);
    let names: Vec<(&str, &str)> = l
        .directives
        .iter()
        .map(|d| (d.name.as_str(), d.kind.as_str()))
        .collect();
    assert_eq!(names, vec![("money", "directive"), ("admin", "if")]);
    assert_eq!(
        l.info().directives,
        vec!["money", "admin", "elseadmin", "unlessadmin", "endadmin"]
    );
}

#[test]
fn reads_config_env_and_translations() {
    let l = load();
    let keys: Vec<(&str, &str)> = l
        .config
        .iter()
        .map(|c| (c.key.as_str(), c.value.as_str()))
        .collect();
    assert!(keys.contains(&("app.name", "env('APP_NAME', 'Laravel')")));
    assert!(keys.contains(&("app.maintenance.driver", "'file'")));
    assert!(keys.contains(&("services.stripe.secret", "env('STRIPE_SECRET')")));
    let name = l.config.iter().find(|c| c.key == "app.name").unwrap();
    assert_eq!(
        (
            name.location.path.as_str(),
            name.location.line,
            name.location.character
        ),
        ("config/app.php", 3, 4)
    );

    let env: Vec<(&str, &str)> = l
        .env
        .iter()
        .map(|e| (e.key.as_str(), e.value.as_str()))
        .collect();
    assert_eq!(
        env,
        vec![
            ("APP_NAME", "Shop"),
            ("APP_DEBUG", "true"),
            ("STRIPE_KEY", "pk_test"),
            ("DB_HOST", "127.0.0.1")
        ]
    );

    let t: Vec<(&str, &str)> = l
        .translations
        .iter()
        .map(|t| (t.key.as_str(), t.value.as_str()))
        .collect();
    assert_eq!(
        t,
        vec![
            ("Checkout", "Zur Kasse"),
            ("messages.orders.empty", "You have no orders yet."),
            ("messages.welcome", "Welcome to our shop"),
        ]
    );
    let welcome = l
        .translations
        .iter()
        .find(|t| t.key == "messages.welcome")
        .unwrap();
    assert_eq!(welcome.locales, vec!["en", "de"]);
    assert_eq!(welcome.location.path, "lang/en/messages.php");
}

#[test]
fn analyses_models() {
    let l = load();
    let user = l.model("App\\Models\\User").unwrap();
    assert_eq!(user.table, "users");
    let attrs: Vec<(&str, &str)> = user
        .attributes
        .iter()
        .map(|a| (a.name.as_str(), a.type_name.as_str()))
        .collect();
    assert_eq!(
        attrs,
        vec![
            ("id", "int"),
            ("name", "string"),
            ("email", "string"),
            ("email_verified_at", "\\Illuminate\\Support\\Carbon"),
            ("password", "string"),
            ("remember_token", "string"),
            ("created_at", "\\Illuminate\\Support\\Carbon"),
            ("updated_at", "\\Illuminate\\Support\\Carbon"),
            ("avatar_url", "string"),
            ("full_name", "mixed"),
            ("initials", "string"),
        ]
    );
    assert!(
        user.attributes
            .iter()
            .find(|a| a.name == "email")
            .unwrap()
            .fillable
    );
    assert_eq!(
        user.attributes
            .iter()
            .find(|a| a.name == "password")
            .unwrap()
            .cast
            .as_deref(),
        Some("hashed")
    );
    assert_eq!(user.relations.len(), 1);
    assert_eq!(
        user.relations[0].related.as_deref(),
        Some("App\\Models\\Order")
    );
    assert_eq!(
        user.scopes
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["active"]
    );

    let order = l.model("Order").unwrap();
    let cols: Vec<&str> = order.attributes.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        cols,
        vec![
            "id",
            "user_id",
            "status",
            "total",
            "created_at",
            "updated_at",
            "deleted_at"
        ]
    );
    assert_eq!(order.attributes[2].type_name, "App\\Models\\OrderStatus");
    assert_eq!(order.attributes[3].type_name, "float");
    let rels: Vec<(&str, &str, Option<&str>)> = order
        .relations
        .iter()
        .map(|r| (r.name.as_str(), r.kind.as_str(), r.related.as_deref()))
        .collect();
    assert_eq!(
        rels,
        vec![
            ("user", "belongsTo", Some("App\\Models\\User")),
            ("lines", "hasMany", Some("App\\Models\\OrderLine"))
        ]
    );

    let line = l.model("OrderLine").unwrap();
    assert_eq!(line.table, "order_items");
    assert_eq!(
        line.attributes
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id", "order_id", "quantity"]
    );
    assert!(l.model("OrderStatus").is_none());
}

fn labels(l: &Laravel, path: &str, marked: &str) -> Vec<String> {
    let (src, at) = caret(marked);
    l.complete(path, &src, at)
        .map(|c| c.items.into_iter().map(|i| i.label).collect())
        .unwrap_or_default()
}

#[test]
fn completes_strings() {
    let l = load();
    let routes = labels(
        &l,
        "app/Http/Controllers/X.php",
        "<?php\nreturn redirect()->route('|",
    );
    assert!(
        routes.contains(&"orders.show".to_string())
            && routes.contains(&"admin.photos.index".to_string())
    );
    let (src, at) = caret("<?php route('orders.s|how');");
    let c = l.complete("x.php", &src, at).unwrap();
    assert_eq!((c.from, c.to), (13, 24));

    assert!(labels(&l, "x.php", "<?php view('|").contains(&"orders.show".to_string()));
    assert!(labels(&l, "x.blade.php", "@extends('|')").contains(&"layouts.app".to_string()));
    assert!(
        labels(&l, "x.php", "<?php config('app.|").contains(&"app.maintenance.driver".to_string())
    );
    assert!(
        labels(&l, "x.php", "<?php Config::get('|").contains(&"services.stripe.key".to_string())
    );
    assert_eq!(
        labels(&l, "x.php", "<?php env('|"),
        vec!["APP_NAME", "APP_DEBUG", "STRIPE_KEY", "DB_HOST"]
    );
    assert!(labels(&l, "x.blade.php", "{{ __('|') }}").contains(&"messages.welcome".to_string()));
    assert!(labels(&l, "x.php", "<?php str_replace('|").is_empty());
}

#[test]
fn completes_columns_relations_and_properties() {
    let l = load();
    let path = "app/Http/Controllers/OrderController.php";
    let src = std::fs::read_to_string(root().join(path)).unwrap();

    let cols = labels(
        &l,
        path,
        "<?php\nuse App\\Models\\Order;\nOrder::query()->where('|",
    );
    assert!(cols.contains(&"status".to_string()) && cols.contains(&"user_id".to_string()));
    // Relation chain: $user->orders()->where(...) completes Order columns.
    let marked = src.replace("where('total'", "where('|total'");
    let cols = labels(&l, path, &marked);
    assert!(cols.contains(&"total".to_string()) && !cols.contains(&"email".to_string()));
    // with('…') completes relations.
    assert_eq!(
        labels(&l, path, "<?php\nuse App\\Models\\Order;\nOrder::with('|"),
        vec!["user", "lines"]
    );
    // Property access on a typed parameter, through a relation.
    let props = labels(
        &l,
        path,
        "<?php\nuse App\\Models\\Order;\nfunction f(Order $order) { $order->user->|",
    );
    assert!(
        props.contains(&"email".to_string())
            && props.contains(&"orders".to_string())
            && props.contains(&"full_name".to_string())
    );
    // Blade: variable named like a model.
    let props = labels(
        &l,
        "resources/views/orders/show.blade.php",
        "{{ $order->st| }}",
    );
    assert!(props.contains(&"status".to_string()));
    // Local scopes.
    assert_eq!(
        labels(&l, path, "<?php\nuse App\\Models\\User;\nUser::ac|"),
        vec!["active"]
    );
    // Controller action methods.
    let m = labels(&l, "routes/web.php", "<?php\nuse App\\Http\\Controllers\\OrderController;\nRoute::get('/', [OrderController::class, '|']);");
    assert!(m.contains(&"checkout".to_string()) && m.contains(&"report".to_string()));
}

#[test]
fn completes_blade_components_and_directives() {
    let l = load();
    assert_eq!(
        labels(&l, "a.blade.php", "<x-|"),
        vec![
            "livewire:order-counter",
            "x-alert",
            "x-button",
            "x-forms.input"
        ]
    );
    assert_eq!(
        labels(&l, "a.blade.php", "@mo|"),
        vec![
            "@money",
            "@admin",
            "@elseadmin",
            "@unlessadmin",
            "@endadmin"
        ]
    );
}

fn goto(l: &Laravel, path: &str, marked: &str) -> Vec<(String, u32)> {
    let (src, at) = caret(marked);
    l.goto(path, &src, at)
        .into_iter()
        .map(|g| (g.path, g.line))
        .collect()
}

#[test]
fn goes_to_declarations() {
    let l = load();
    assert_eq!(
        goto(&l, "x.php", "<?php route('orders.sh|ow');"),
        vec![
            ("routes/web.php".into(), 12),
            ("app/Http/Controllers/OrderController.php".into(), 14),
            ("routes/api.php".into(), 5)
        ]
    );
    assert_eq!(
        goto(&l, "x.php", "<?php view('orders.|show');"),
        vec![("resources/views/orders/show.blade.php".into(), 0)]
    );
    assert_eq!(
        goto(&l, "x.php", "<?php config('app.na|me');"),
        vec![("config/app.php".into(), 3)]
    );
    assert_eq!(
        goto(&l, "x.php", "<?php env('STRIPE|_KEY');"),
        vec![(".env".into(), 3)]
    );
    assert_eq!(
        goto(&l, "x.php", "<?php __('messages.orders.em|pty');"),
        vec![("lang/en/messages.php".into(), 5)]
    );
    assert_eq!(
        goto(&l, "x.blade.php", "<x-al|ert />"),
        vec![("resources/views/components/alert.blade.php".into(), 0)]
    );
    assert_eq!(
        goto(&l, "x.blade.php", "<x-button>Hi</x-but|ton>"),
        vec![
            ("app/View/Components/Button.php".into(), 0),
            ("resources/views/components/button.blade.php".into(), 0)
        ]
    );
    assert_eq!(
        goto(
            &l,
            "x.php",
            "<?php\nuse App\\Models\\Order;\nfunction f(Order $order) { $order->us|er; }"
        ),
        vec![("app/Models/Order.php".into(), 10)]
    );
    assert_eq!(
        goto(
            &l,
            "x.php",
            "<?php\nuse App\\Models\\Order;\nfunction f(Order $order) { $order->tot|al; }"
        ),
        vec![(
            "database/migrations/2026_01_01_000000_create_orders_table.php".into(),
            14
        )]
    );
    assert_eq!(
        goto(&l, "routes/web.php", "<?php\nuse App\\Http\\Controllers\\OrderController;\nRoute::get('/', [OrderController::class, 'chec|kout']);"),
        vec![("app/Http/Controllers/OrderController.php".into(), 24)]
    );
    assert_eq!(
        goto(&l, "a.blade.php", "@endad|min"),
        vec![("app/Providers/AppServiceProvider.php".into(), 12)]
    );
    assert!(goto(&l, "x.php", "<?php strlen('ab|c');").is_empty());
}

#[test]
fn gutter_marks() {
    let l = load();
    let path = "app/Http/Controllers/OrderController.php";
    let src = std::fs::read_to_string(root().join(path)).unwrap();
    let marks: Vec<(u32, String)> = l
        .gutter(path, &src)
        .into_iter()
        .map(|m| (m.line, m.kind))
        .collect();
    assert_eq!(
        marks,
        vec![
            (9, "route".to_string()),
            (14, "route".into()),
            (16, "view".into()),
            (19, "route".into()),
            (24, "route".into()),
            (29, "route".into()),
            (35, "route".into()),
            (39, "route".into()),
        ]
    );
    let show = l
        .gutter(path, &src)
        .into_iter()
        .find(|m| m.line == 14)
        .unwrap();
    assert_eq!(
        show.tooltip,
        "GET|HEAD /orders/{order} (orders.show)\nGET|HEAD /api/orders/{order} (orders.show)"
    );

    let routes_src = std::fs::read_to_string(root().join("routes/web.php")).unwrap();
    let marks = l.gutter("routes/web.php", &routes_src);
    let first = marks.iter().find(|m| m.kind == "controller").unwrap();
    assert_eq!(first.line, 12);
    assert_eq!(first.tooltip, "Navigate to OrderController::show()");
}

#[test]
fn ide_json_completions() {
    let l = load();
    assert_eq!(l.ide.files, vec!["ide.json"]);
    let (src, at) = caret("<?php\norder_status('sh|');");
    let c = l.complete("app/x.php", &src, at).unwrap();
    let labels: Vec<&str> = c.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, vec!["paid", "shipped", "cancelled"]);
    assert_eq!(c.items[0].kind, "value");
    let (src, at) = caret("<?php\nuse App\\Models\\Order;\nOrder::linkTo('|');");
    let c = l.complete("app/x.php", &src, at).unwrap();
    assert!(c.items.iter().any(|i| i.label == "orders.show"));
}

#[test]
fn dashboard_overview() {
    let l = load();
    let o = phpforge_fw_laravel::dashboard::overview(&l);
    assert_eq!(o.version.as_deref(), Some("12.4.1"));
    assert!(o.models.iter().any(|m| m.name == "Order" && m.table == "orders"));
    let names: Vec<&str> = o.commands.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["inspire", "orders:prune"]);
    assert_eq!(o.commands[1].description, "Delete abandoned orders");
    assert_eq!(o.commands[1].location.path, "app/Console/Commands/PruneOrders.php");
    assert_eq!(o.ide_json.code_generations[0].name, "Create DTO");

    let generation = &o.ide_json.code_generations[0];
    let root = root();
    let read = |p: &str| std::fs::read_to_string(root.join(p)).ok();
    let class_path = |fqn: &str| format!("app/{}.php", fqn.trim_start_matches("App\\").replace('\\', "/"));
    let files = phpforge_fw_laravel::ide_json::generate(generation, "Invoice", &read, &class_path);
    assert_eq!(files[0].path, "app/Data/InvoiceData.php");
    assert!(files[0].content.contains("final readonly class InvoiceData"));
    assert!(!files[0].exists);
}
