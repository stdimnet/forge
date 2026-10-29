//! Frequently used WordPress core hooks, offered by completion when the project does not
//! contain WordPress itself (a plugin or theme on its own). The names are WordPress's; the
//! one-line summaries are Forge's own. A project with WordPress core files indexes the real
//! `do_action` / `apply_filters` calls instead.

use crate::hooks::HookKind;

/// (name, kind, summary).
pub const CORE_HOOKS: &[(&str, HookKind, &str)] = &[
    (
        "muplugins_loaded",
        HookKind::Action,
        "Must-use plugins are loaded.",
    ),
    (
        "plugins_loaded",
        HookKind::Action,
        "All active plugins are loaded.",
    ),
    (
        "after_setup_theme",
        HookKind::Action,
        "The theme's functions.php ran; declare theme supports here.",
    ),
    (
        "init",
        HookKind::Action,
        "WordPress is loaded; register post types and taxonomies here.",
    ),
    (
        "widgets_init",
        HookKind::Action,
        "Register sidebars and widgets.",
    ),
    (
        "wp_loaded",
        HookKind::Action,
        "WordPress, plugins and the theme are fully loaded.",
    ),
    (
        "rest_api_init",
        HookKind::Action,
        "Register REST API routes.",
    ),
    (
        "admin_init",
        HookKind::Action,
        "An admin screen starts loading.",
    ),
    (
        "admin_menu",
        HookKind::Action,
        "Add pages to the admin menu.",
    ),
    (
        "admin_notices",
        HookKind::Action,
        "Print notices at the top of admin screens.",
    ),
    (
        "admin_enqueue_scripts",
        HookKind::Action,
        "Enqueue scripts and styles for admin screens.",
    ),
    (
        "wp_enqueue_scripts",
        HookKind::Action,
        "Enqueue scripts and styles for the front end.",
    ),
    (
        "enqueue_block_editor_assets",
        HookKind::Action,
        "Enqueue assets for the block editor.",
    ),
    (
        "wp_head",
        HookKind::Action,
        "Print markup inside the front end <head>.",
    ),
    (
        "wp_footer",
        HookKind::Action,
        "Print markup before the closing </body>.",
    ),
    (
        "template_redirect",
        HookKind::Action,
        "Before the template for a request is chosen.",
    ),
    (
        "save_post",
        HookKind::Action,
        "A post was created or updated.",
    ),
    (
        "before_delete_post",
        HookKind::Action,
        "A post is about to be deleted.",
    ),
    (
        "transition_post_status",
        HookKind::Action,
        "A post changes its status.",
    ),
    ("user_register", HookKind::Action, "A user was created."),
    ("wp_login", HookKind::Action, "A user logged in."),
    (
        "add_meta_boxes",
        HookKind::Action,
        "Add meta boxes to the edit screen.",
    ),
    (
        "pre_get_posts",
        HookKind::Action,
        "Change a WP_Query before it runs.",
    ),
    ("shutdown", HookKind::Action, "PHP is about to shut down."),
    (
        "the_content",
        HookKind::Filter,
        "The post content before it is printed.",
    ),
    (
        "the_title",
        HookKind::Filter,
        "A post title before it is printed.",
    ),
    (
        "the_excerpt",
        HookKind::Filter,
        "The post excerpt before it is printed.",
    ),
    (
        "excerpt_length",
        HookKind::Filter,
        "Number of words in generated excerpts.",
    ),
    (
        "body_class",
        HookKind::Filter,
        "CSS classes of the <body> element.",
    ),
    (
        "post_class",
        HookKind::Filter,
        "CSS classes of a post's container.",
    ),
    (
        "wp_nav_menu_items",
        HookKind::Filter,
        "The HTML of a navigation menu's items.",
    ),
    (
        "query_vars",
        HookKind::Filter,
        "Public query variables WordPress recognizes.",
    ),
    (
        "upload_mimes",
        HookKind::Filter,
        "File types allowed for uploads.",
    ),
    (
        "login_redirect",
        HookKind::Filter,
        "Where users go after logging in.",
    ),
    (
        "cron_schedules",
        HookKind::Filter,
        "Recurrence intervals for scheduled events.",
    ),
    (
        "plugin_action_links",
        HookKind::Filter,
        "Links below a plugin in the Plugins screen.",
    ),
    (
        "manage_posts_columns",
        HookKind::Filter,
        "Columns of the posts list table.",
    ),
    (
        "script_loader_tag",
        HookKind::Filter,
        "The <script> tag of an enqueued script.",
    ),
    (
        "wp_mail",
        HookKind::Filter,
        "Arguments of an email before it is sent.",
    ),
    (
        "rest_prepare_post",
        HookKind::Filter,
        "A post's REST API response.",
    ),
];

/// The summary of a core hook.
pub fn core_hook(name: &str) -> Option<(HookKind, &'static str)> {
    CORE_HOOKS
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|&(_, k, s)| (k, s))
}
