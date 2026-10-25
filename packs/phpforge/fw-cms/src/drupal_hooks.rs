//! A small list of Drupal core hooks for completion: the hook names and parameters are
//! Drupal's API, the one-line summaries are Forge's own. Upper-case parts (`FORM_ID`,
//! `HOOK`, `ENTITY_TYPE`) stand for a name the implementation fills in.

/// (hook without the `hook_` prefix, parameters, summary).
pub const HOOKS: &[(&str, &str, &str)] = &[
    ("help", "$route_name, \\Drupal\\Core\\Routing\\RouteMatchInterface $route_match", "Help text for the module's pages."),
    ("theme", "$existing, $type, $theme, $path", "Register theme hooks (templates) of the module."),
    ("theme_suggestions_HOOK", "array $variables", "Template suggestions for a theme hook."),
    ("theme_suggestions_HOOK_alter", "array &$suggestions, array $variables", "Change the template suggestions of a theme hook."),
    ("preprocess", "&$variables, $hook", "Prepare variables for every template."),
    ("preprocess_HOOK", "&$variables", "Prepare variables for one theme hook's template."),
    ("form_alter", "&$form, \\Drupal\\Core\\Form\\FormStateInterface $form_state, $form_id", "Change any form before it is rendered."),
    ("form_FORM_ID_alter", "&$form, \\Drupal\\Core\\Form\\FormStateInterface $form_state, $form_id", "Change one form, by its form id."),
    ("form_BASE_FORM_ID_alter", "&$form, \\Drupal\\Core\\Form\\FormStateInterface $form_state, $form_id", "Change all forms that share a base form id."),
    ("entity_presave", "\\Drupal\\Core\\Entity\\EntityInterface $entity", "An entity is about to be saved."),
    ("entity_insert", "\\Drupal\\Core\\Entity\\EntityInterface $entity", "A new entity was saved."),
    ("entity_update", "\\Drupal\\Core\\Entity\\EntityInterface $entity", "An existing entity was saved."),
    ("entity_delete", "\\Drupal\\Core\\Entity\\EntityInterface $entity", "An entity was deleted."),
    ("entity_view", "array &$build, \\Drupal\\Core\\Entity\\EntityInterface $entity, \\Drupal\\Core\\Entity\\Display\\EntityViewDisplayInterface $display, $view_mode", "Add to the render array of an entity."),
    ("entity_access", "\\Drupal\\Core\\Entity\\EntityInterface $entity, $operation, \\Drupal\\Core\\Session\\AccountInterface $account", "Grant or deny access to an entity operation."),
    ("ENTITY_TYPE_presave", "\\Drupal\\Core\\Entity\\EntityInterface $entity", "An entity of one type is about to be saved."),
    ("ENTITY_TYPE_insert", "\\Drupal\\Core\\Entity\\EntityInterface $entity", "A new entity of one type was saved."),
    ("ENTITY_TYPE_update", "\\Drupal\\Core\\Entity\\EntityInterface $entity", "An existing entity of one type was saved."),
    ("ENTITY_TYPE_delete", "\\Drupal\\Core\\Entity\\EntityInterface $entity", "An entity of one type was deleted."),
    ("ENTITY_TYPE_view", "array &$build, \\Drupal\\Core\\Entity\\EntityInterface $entity, \\Drupal\\Core\\Entity\\Display\\EntityViewDisplayInterface $display, $view_mode", "Add to the render array of one entity type."),
    ("entity_type_alter", "array &$entity_types", "Change entity type definitions."),
    ("entity_base_field_info", "\\Drupal\\Core\\Entity\\EntityTypeInterface $entity_type", "Add base fields to an entity type."),
    ("node_access", "\\Drupal\\node\\NodeInterface $node, $operation, \\Drupal\\Core\\Session\\AccountInterface $account", "Control access to a node."),
    ("user_login", "\\Drupal\\user\\UserInterface $account", "A user logged in."),
    ("user_logout", "\\Drupal\\Core\\Session\\AccountInterface $account", "A user logged out."),
    ("cron", "", "Periodic work when cron runs."),
    ("mail", "$key, &$message, $params", "Prepare an email the module sends."),
    ("mail_alter", "&$message", "Change any email before it is sent."),
    ("page_attachments", "array &$attachments", "Add libraries and head elements to every page."),
    ("page_attachments_alter", "array &$attachments", "Change the attachments of every page."),
    ("library_info_alter", "&$libraries, $extension", "Change the asset libraries of an extension."),
    ("menu_links_discovered_alter", "&$links", "Change discovered menu links."),
    ("token_info", "", "Describe the tokens the module provides."),
    ("tokens", "$type, $tokens, array $data, array $options, \\Drupal\\Core\\Render\\BubbleableMetadata $bubbleable_metadata", "Replace the module's tokens."),
    ("schema", "", "Database tables of the module (in .install files)."),
    ("install", "$is_syncing", "The module was installed (in .install files)."),
    ("uninstall", "$is_syncing", "The module is uninstalled (in .install files)."),
    ("requirements", "$phase", "Check requirements for the status report (in .install files)."),
    ("update_N", "&$sandbox", "A database update (in .install files)."),
    ("modules_installed", "$modules, $is_syncing", "Other modules were installed."),
    ("query_TAG_alter", "\\Drupal\\Core\\Database\\Query\\AlterableInterface $query", "Change database queries with a tag."),
    ("views_data", "", "Describe the module's tables to Views."),
    ("views_query_alter", "\\Drupal\\views\\ViewExecutable $view, \\Drupal\\views\\Plugin\\views\\query\\QueryPluginBase $query", "Change the query of a view."),
    ("cache_flush", "", "Caches are being cleared."),
    ("rebuild", "", "Rebuild data after caches are cleared."),
];

/// Whether `hook` (without `hook_`) is a pattern with upper-case placeholders.
pub fn is_pattern(hook: &str) -> bool {
    hook.split('_')
        .any(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_uppercase()))
}

/// The hook `function_suffix` implements (`form_user_login_form_alter` → `form_FORM_ID_alter`):
/// an exact name first, then a pattern whose placeholders take at least one word each.
pub fn hook_for(suffix: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    HOOKS
        .iter()
        .find(|(h, ..)| !is_pattern(h) && *h == suffix)
        .or_else(|| {
            HOOKS
                .iter()
                .find(|(h, ..)| is_pattern(h) && pattern_matches(h, suffix))
        })
}

fn pattern_matches(pattern: &str, name: &str) -> bool {
    let upper = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_ascii_uppercase());
    // Consecutive upper-case words are one placeholder (`FORM_ID`, `ENTITY_TYPE`).
    let mut p: Vec<&str> = Vec::new();
    for w in pattern.split('_') {
        if upper(w) && p.last().is_some_and(|l| upper(l)) {
            continue;
        }
        p.push(w);
    }
    let n: Vec<&str> = name.split('_').collect();
    fn go(p: &[&str], n: &[&str]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (None, _) | (_, None) => false,
            (Some(pp), _) if pp.chars().all(|c| c.is_ascii_uppercase()) && !pp.is_empty() => {
                // The placeholder takes one or more words (`N` of update_N: digits).
                (1..=n.len()).any(|k| {
                    (*pp != "N" || n[..k].iter().all(|w| w.chars().all(|c| c.is_ascii_digit())))
                        && go(&p[1..], &n[k..])
                })
            }
            (Some(pp), Some(nn)) => pp == nn && go(&p[1..], &n[1..]),
        }
    }
    go(&p, &n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_hook_patterns() {
        assert_eq!(hook_for("form_alter").unwrap().0, "form_alter");
        assert_eq!(
            hook_for("form_user_login_form_alter").unwrap().0,
            "form_FORM_ID_alter"
        );
        assert_eq!(hook_for("node_presave").unwrap().0, "ENTITY_TYPE_presave");
        assert_eq!(hook_for("preprocess_page").unwrap().0, "preprocess_HOOK");
        assert_eq!(hook_for("update_10001").unwrap().0, "update_N");
        assert!(hook_for("update_x").is_none());
        assert!(hook_for("helper").is_none());
        assert!(is_pattern("form_FORM_ID_alter") && !is_pattern("form_alter"));
    }
}
