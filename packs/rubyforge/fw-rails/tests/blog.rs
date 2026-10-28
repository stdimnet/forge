//! The Rails analysis on the `blog` fixture application (tests/fixtures/blog): routes,
//! controllers, models with their columns, database.yml, locales, Stimulus, and the
//! editor's navigation, completion, gutter marks, inspections and folds.

use std::collections::BTreeMap;
use std::path::Path;

use rubyforge_fw_rails::editor::{complete, goto, marks};
use rubyforge_fw_rails::{Options, Project};

fn read_dir(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            read_dir(root, &p, out);
        } else if let Ok(text) = std::fs::read_to_string(&p) {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if rubyforge_fw_rails::wanted(&rel) {
                out.insert(rel, text);
            }
        }
    }
}

fn blog(options: Options) -> Project {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blog");
    let mut files = BTreeMap::new();
    read_dir(&root, &root, &mut files);
    Project::new(files, options)
}

fn utf16_offset(text: &str, needle: &str, plus: usize) -> usize {
    let byte = text.find(needle).unwrap_or_else(|| panic!("{needle}")) + plus;
    text[..byte].encode_utf16().count()
}

#[test]
fn project_model() {
    let p = blog(Options::default());
    let i = &p.info;
    assert!(i.rails && i.binstub);
    assert_eq!(i.version.as_deref(), Some("7.2.1"));
    assert_eq!(i.app_name.as_deref(), Some("Blog"));
    assert_eq!(i.routes_source, "routes.rb");
    let names: Vec<&str> = i.routes.iter().filter_map(|r| r.name.as_deref()).collect();
    for n in [
        "rails_health_check",
        "root",
        "article_comments",
        "publish_article",
        "search_articles",
        "articles",
        "new_session",
        "about",
        "admin_root",
        "admin_users",
        "admin_user",
    ] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    let show = i
        .routes
        .iter()
        .find(|r| r.name.as_deref() == Some("article"))
        .unwrap();
    assert_eq!(show.sample_path, "/articles/1");
    let action = show.action_location.as_ref().unwrap();
    assert_eq!(
        (action.path.as_str(), action.line),
        ("app/controllers/articles_controller.rb", 7)
    );
    let admin = i
        .routes
        .iter()
        .find(|r| r.name.as_deref() == Some("admin_root"))
        .unwrap();
    assert_eq!(
        admin.action_location.as_ref().unwrap().path,
        "app/controllers/admin/dashboard_controller.rb"
    );

    let ctl: Vec<&str> = i.controllers.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        ctl,
        [
            "admin/dashboard",
            "admin/users",
            "application",
            "articles",
            "comments",
            "pages",
            "sessions"
        ]
    );

    let article = p.model("Article").unwrap();
    assert_eq!(article.table, "articles");
    let cols: Vec<&str> = article.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        cols,
        [
            "id",
            "title",
            "body",
            "status",
            "published_at",
            "user_id",
            "created_at",
            "updated_at",
            "slug"
        ]
    );
    let users = i.tables.iter().find(|t| t.name == "users").unwrap();
    assert_eq!(users.migrations.len(), 1);
    assert_eq!(i.models.len(), 3);

    assert_eq!(i.default_locale, "en");
    assert_eq!(i.locales, ["en", "de"]);
    let stim: Vec<&str> = i.stimulus.iter().map(|s| s.identifier.as_str()).collect();
    assert_eq!(stim, ["clipboard", "hello"]);
    let dbs: Vec<(&str, &str, &str)> = i
        .databases
        .iter()
        .map(|d| {
            (
                d.environment.as_str(),
                d.database.as_str(),
                d.username.as_str(),
            )
        })
        .collect();
    assert_eq!(
        dbs,
        [
            ("development", "blog_development", "blog"),
            ("test", "blog_test", "blog"),
            ("production", "blog_production", "blog")
        ]
    );
    assert!(!i.databases[0].has_password);
    let with_env = blog(Options {
        env: [("DB_PASSWORD".to_string(), "pw".to_string())].into(),
        ..Options::default()
    });
    assert_eq!(with_env.info.databases[0].password.as_deref(), Some("pw"));
}

#[test]
fn rails_routes_output_replaces_the_static_routes() {
    let out = "Prefix Verb URI Pattern Controller#Action\n  articles GET /articles(.:format) articles#index\n  turbo_recede_historical_location GET /recede_historical_location(.:format) turbo/native/navigation#recede\n";
    let p = blog(Options {
        routes_output: Some(out.into()),
        ..Options::default()
    });
    assert_eq!(p.info.routes_source, "rails routes");
    assert_eq!(p.info.routes.len(), 2);
    assert_eq!(p.info.routes[0].location.line, 5);
}

#[test]
fn navigation() {
    let p = blog(Options::default());
    let path = "app/views/articles/show.html.erb";
    let text = p.file(path).unwrap();
    // Route helper → routes.rb
    let g = goto(&p, path, text, utf16_offset(text, "edit_article_path", 3));
    assert_eq!((g[0].path.as_str(), g[0].line), ("config/routes.rb", 5));
    // render "header" → the partial
    let g = goto(&p, path, text, utf16_offset(text, "\"header\"", 2));
    assert_eq!(g[0].path, "app/views/articles/_header.html.erb");
    // render partial: "comments/comment"
    let g = goto(&p, path, text, utf16_offset(text, "comments/comment", 10));
    assert_eq!(g[0].path, "app/views/comments/_comment.html.erb");
    // t(".copy_link") → en.yml (lazy lookup)
    let g = goto(&p, path, text, utf16_offset(text, ".copy_link", 2));
    assert_eq!(
        (g[0].path.as_str(), g[0].line),
        ("config/locales/en.yml", 8)
    );
    // data-controller / data-action / target → the Stimulus controller
    let g = goto(&p, path, text, utf16_offset(text, "\"clipboard\"", 3));
    assert_eq!(
        (g[0].path.as_str(), g[0].line),
        ("app/javascript/controllers/clipboard_controller.js", 3)
    );
    let g = goto(&p, path, text, utf16_offset(text, "clipboard#copy", 11));
    assert_eq!(g[0].line, 7);
    let g = goto(&p, path, text, utf16_offset(text, "\"button\"", 2));
    assert_eq!(g[0].line, 4);
    // Turbo frame "comments" → none elsewhere; "article_header" in the controller → the partial's id? (no frame)
    let ctl_path = "app/controllers/articles_controller.rb";
    let ctl = p.file(ctl_path).unwrap();
    let g = goto(&p, ctl_path, ctl, utf16_offset(ctl, "render :new", 9));
    assert_eq!(g[0].path, "app/views/articles/new.html.erb");
    let g = goto(&p, ctl_path, ctl, utf16_offset(ctl, "t(\".updated\")", 4));
    assert_eq!(g[0].line, 14);
    // routes.rb: "articles#index" → ArticlesController#index
    let routes = p.file("config/routes.rb").unwrap();
    let g = goto(
        &p,
        "config/routes.rb",
        routes,
        utf16_offset(routes, "\"articles#index\"", 3),
    );
    assert_eq!(
        (g[0].path.as_str(), g[0].line),
        ("app/controllers/articles_controller.rb", 3)
    );
}

#[test]
fn completion() {
    let p = blog(Options::default());
    let labels = |path: &str, text: &str| -> Vec<String> {
        complete(&p, path, text, text.encode_utf16().count())
            .map(|c| c.items.into_iter().map(|i| i.label).collect())
            .unwrap_or_default()
    };
    let ctl = "app/controllers/articles_controller.rb";
    assert!(labels(ctl, "class X\n  def a\n    Article.where(ti").contains(&"title:".to_string()));
    assert!(labels(ctl, "class X\n  def a\n    Article.order(:pub")
        .contains(&":published_at".to_string()));
    assert!(labels(ctl, "class X\n  def a\n    @article.").contains(&"comments".to_string()));
    assert!(labels(
        ctl,
        "class X\n  def a\n    params.require(:article).permit(:"
    )
    .contains(&":body".to_string()));
    assert!(labels(ctl, "class X\n  def a\n    Article.").contains(&"published".to_string()));
    assert!(labels(ctl, "class X\n  def a\n    redirect_to new_ar")
        .contains(&"new_article_path".to_string()));
    assert!(labels(ctl, "class X\n  def a\n    render \"").contains(&"edit".to_string()));
    let view = "app/views/articles/index.html.erb";
    let l = labels(view, "<%= t(\".");
    assert!(
        l.contains(&".title".to_string()) && l.contains(&".new_article".to_string()),
        "{l:?}"
    );
    assert!(labels(view, "<%= t(\"pages.").contains(&"pages.about.title".to_string()));
    assert!(labels(view, "<%= render \"").contains(&"article".to_string()));
    assert!(labels(view, "<div data-controller=\"hello cl").contains(&"clipboard".to_string()));
    assert!(labels(view, "<button data-action=\"click->").contains(&"clipboard#copy".to_string()));
    assert!(labels(view, "<input data-hello-target=\"").contains(&"output".to_string()));
    assert!(labels(view, "<%= turbo_stream.").contains(&"replace".to_string()));
    assert!(labels(view, "<%= turbo").contains(&"turbo_frame_tag".to_string()));
    assert!(labels(view, "<%= turbo_stream.replace \"").contains(&"articles".to_string()));
    let form = "app/views/articles/_form.html.erb";
    assert!(labels(
        form,
        "<%= form_with(model: article) do |form| %>\n  <%= form.text_field :"
    )
    .contains(&":title".to_string()));
    let model = "app/models/article.rb";
    assert!(
        labels(model, "class Article < ApplicationRecord\n  validates :")
            .contains(&":body".to_string())
    );
    // Text outside ERB tags gets nothing.
    assert!(labels(view, "<p>new_ar").is_empty());
}

#[test]
fn marks_problems_and_folds() {
    let p = blog(Options::default());
    let ctl = "app/controllers/articles_controller.rb";
    let m = marks(&p, ctl, p.file(ctl).unwrap());
    let index: Vec<(&str, &str)> = m
        .gutter
        .iter()
        .filter(|g| g.line == 3)
        .map(|g| (g.kind.as_str(), g.tooltip.as_str()))
        .collect();
    assert_eq!(
        index,
        [
            ("view", "Go to view: index.html.erb"),
            ("route", "Routes: GET /, GET /articles")
        ]
    );
    let missing: Vec<&str> = m.problems.iter().map(|x| x.message.as_str()).collect();
    assert_eq!(
        missing,
        ["Missing translation 'articles.destroy.destroyed'"]
    );
    assert_eq!(m.folds.len(), 2);
    assert_eq!(m.folds[0].text, "Article was successfully created.");

    let show = "app/views/articles/show.html.erb";
    let m = marks(&p, show, p.file(show).unwrap());
    assert_eq!(m.gutter[0].tooltip, "Go to ArticlesController#show");
    assert!(m.gutter.iter().any(|g| g.kind == "stimulus" && g.line == 2));
    let msgs: Vec<&str> = m.problems.iter().map(|x| x.message.as_str()).collect();
    assert_eq!(
        msgs,
        ["Translation 'articles.show.copy_link' is missing for de"]
    );

    let about = "app/views/pages/about.html.erb";
    let m = marks(&p, about, p.file(about).unwrap());
    assert_eq!(
        m.problems[0].message,
        "Missing translation 'pages.about.intro'"
    );
    assert_eq!(m.folds[0].text, "About this blog");

    let partial = "app/views/articles/_form.html.erb";
    let m = marks(&p, partial, p.file(partial).unwrap());
    assert_eq!(m.gutter[0].tooltip, "Rendered in 2 places");

    let model = "app/models/article.rb";
    let m = marks(&p, model, p.file(model).unwrap());
    assert_eq!(m.gutter[0].kind, "table");
    assert!(m.gutter[0]
        .tooltip
        .starts_with("Table articles: id, title, body"));
    assert_eq!(m.gutter[0].targets.len(), 3);
    assert_eq!(m.gutter[1].tooltip, "belongs_to User");

    let routes = "config/routes.rb";
    let m = marks(&p, routes, p.file(routes).unwrap());
    assert!(m
        .gutter
        .iter()
        .any(|g| g.line == 5 && g.tooltip.starts_with("articles#index")));

    let js = "app/javascript/controllers/hello_controller.js";
    let m = marks(&p, js, p.file(js).unwrap());
    assert_eq!(m.gutter[0].tooltip, "Used in 2 places");

    let bad = "<div data-controller=\"nope\"><b data-action=\"hello#nope\"></b></div>\n<%= render \"missing\" %>\n";
    let m = marks(&p, "app/views/articles/x.html.erb", bad);
    let msgs: Vec<&str> = m.problems.iter().map(|x| x.message.as_str()).collect();
    assert_eq!(
        msgs,
        [
            "Unknown Stimulus controller 'nope'",
            "Unknown action 'nope' of Stimulus controller 'hello'",
            "Cannot find partial 'missing'"
        ]
    );
}
