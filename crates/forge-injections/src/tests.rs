use super::*;

fn find_in(path: &str, text: &str) -> Vec<Injection> {
    find(&FindRequest {
        text: text.into(),
        path: path.into(),
        settings: InjectionSettings::default(),
    })
}

/// `(language, injected text pieces)` of every injection.
fn summary(path: &str, text: &str) -> Vec<(String, Vec<String>)> {
    find_in(path, text)
        .into_iter()
        .map(|i| {
            (
                i.language,
                i.ranges
                    .iter()
                    .map(|r| text[r.from..r.to].to_string())
                    .collect(),
            )
        })
        .collect()
}

fn langs(path: &str, text: &str) -> Vec<String> {
    find_in(path, text)
        .into_iter()
        .map(|i| i.language)
        .collect()
}

fn at(text: &str, needle: &str) -> usize {
    text.find(needle).expect(needle)
}

#[test]
fn host_languages_follow_the_file_name() {
    assert_eq!(Host::of_path("src/A.php"), Some(Host::Php));
    assert_eq!(Host::of_path("views/a.blade.php"), None);
    assert_eq!(Host::of_path("src/app.tsx"), Some(Host::Js));
    assert_eq!(Host::of_path("README.md"), None);
    assert_eq!(Host::of_path("Makefile"), None);
    assert!(find_in("notes.txt", "SELECT * FROM users").is_empty());
}

#[test]
fn sql_is_detected_from_the_statement_at_the_start_of_a_php_string() {
    let src = r#"<?php
$a = 'SELECT id, name FROM users WHERE id = ?';
$b = "INSERT INTO orders (total) VALUES (1)";
$c = 'Select a product';
$d = 'UPDATE users SET name = ?';
$e = 'SELECT 1';
$f = 'DELETE FROM sessions';
$g = 'WITH t AS (SELECT 1) SELECT * FROM t';
$h = 'plain text';
"#;
    assert_eq!(
        langs("a.php", src),
        ["sql", "sql", "sql", "sql", "sql", "sql"]
    );
    // "Select a product" is a label, not SQL.
    assert!(!summary("a.php", src)
        .iter()
        .any(|(_, r)| r.iter().any(|t| t.contains("product"))));
}

#[test]
fn code_outside_php_tags_is_not_lexed() {
    let src = "<p>'SELECT * FROM users'</p>\n<?php echo 'SELECT * FROM users'; ?>\n<p>it's</p>";
    assert_eq!(langs("a.php", src), ["sql"]);
}

#[test]
fn sql_in_database_calls_does_not_need_a_keyword() {
    let src = r#"<?php
$pdo->query('SHOW TABLES');
$this->db->prepare('PRAGMA table_info(users)');
mysqli_query($link, 'OPTIMIZE TABLE users');
DB::statement('VACUUM');
User::query()->whereRaw('price > IF(state = "TX", ?, 100)', [200]);
$wpdb->get_results("SELECT * FROM {$wpdb->posts}");
$conn->fetchAllAssociative('EXPLAIN orders');
$em->createNativeQuery('CALL refresh()', $rsm);
$cache->query('flush');
$feedback->query('nope');
"#;
    let found = find_in("a.php", src);
    let rules: Vec<_> = found.iter().map(|i| i.rule.clone().unwrap()).collect();
    assert_eq!(
        rules,
        [
            "php-sql-calls",
            "php-sql-calls",
            "php-sql-calls",
            "php-sql-laravel",
            "php-sql-laravel",
            "php-sql-wpdb",
            "php-sql-calls",
            "php-sql-calls"
        ]
    );
    // `{$wpdb->posts}` is an interpolation, not SQL.
    let wp = &found[5];
    assert_eq!(
        wp.ranges
            .iter()
            .map(|r| &src[r.from..r.to])
            .collect::<Vec<_>>(),
        ["SELECT * FROM "]
    );
}

#[test]
fn doctrine_dql_is_left_to_the_doctrine_plugin() {
    let src = r#"<?php
$em->createQuery('SELECT o FROM App\Entity\Order o');
$q = <<<DQL
SELECT o FROM App\Entity\Order o
DQL;
"#;
    assert!(find_in("a.php", src).is_empty());
}

#[test]
fn concatenated_literals_form_one_injection() {
    let src =
        "<?php\n$sql = 'SELECT * FROM ' . $table . ' WHERE id = ' . (int) $id . ' LIMIT 1';\n";
    assert_eq!(
        summary("a.php", src),
        [(
            "sql".to_string(),
            vec![
                "SELECT * FROM ".to_string(),
                " WHERE id = ".to_string(),
                " LIMIT 1".to_string()
            ]
        )]
    );
    let i = &find_in("a.php", src)[0];
    assert_eq!(i.text, "SELECT * FROM  WHERE id =  LIMIT 1");
    assert_eq!(i.literals.len(), 3);
}

#[test]
fn heredoc_and_nowdoc_labels_name_the_language() {
    let src = "<?php\n$q = <<<SQL\n    SELECT * FROM users WHERE id = $id\n    SQL;\n$j = <<<'JSON'\n{\"a\": 1}\nJSON;\n$h = <<<HTML\n<div>$name</div>\nHTML;\n$t = <<<EOT\nSELECT nothing\nEOT;\n";
    let s = summary("a.php", src);
    assert_eq!(
        s.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
        ["sql", "json", "html", "sql"]
    );
    assert_eq!(s[0].1, ["    SELECT * FROM users WHERE id = "]);
    assert_eq!(s[1].1, ["{\"a\": 1}"]);
    assert_eq!(s[2].1, ["<div>", "</div>"]);
    let found = find_in("a.php", src);
    assert_eq!(found[0].rule.as_deref(), Some("php-heredoc"));
    // `<<<EOT` has no language; its text starts with SELECT but has no FROM.
    assert_eq!(found[3].rule.as_deref(), Some("php-sql-keywords"));
}

#[test]
fn language_comments_override_detection() {
    let src = r#"<?php
// language=JSON
$config = '{"debug": true}';
/* language=HTML */ $x = foo('<b>bold</b>');
# language=RegExp
$re = '\d+';
/** @lang SQL */
$q = 'users';
// language=SQL
if ($a) { $n = 'not this one'; }
// language=Klingon
$k = 'SELECT * FROM unknown';
"#;
    let found = find_in("a.php", src);
    assert_eq!(
        found
            .iter()
            .map(|i| i.language.as_str())
            .collect::<Vec<_>>(),
        ["json", "html", "regexp", "sql", "sql"]
    );
    assert!(found[..4].iter().all(|i| i.source == Source::Comment));
    let c = found[0].comment.unwrap();
    assert_eq!(&src[c.from..c.to], "// language=JSON");
    // An unknown language falls back to detection.
    assert_eq!(found[4].source, Source::Rule);
}

#[test]
fn php_patterns_are_injected_without_delimiters_and_flags() {
    let src = r#"<?php
preg_match('/^(\d+)-(\w+)$/i', $s, $m);
preg_replace('#<b>(.*?)</b>#', '$1', $s);
preg_split("{\s*,\s*}", $csv);
$data = json_decode('{"id": 1}', true);
"#;
    assert_eq!(
        summary("a.php", src),
        [
            ("regexp".to_string(), vec![r"^(\d+)-(\w+)$".to_string()]),
            ("regexp".to_string(), vec!["<b>(.*?)</b>".to_string()]),
            ("regexp".to_string(), vec![r"\s*,\s*".to_string()]),
            ("json".to_string(), vec![r#"{"id": 1}"#.to_string()]),
        ]
    );
}

#[test]
fn html_strings_start_with_a_tag() {
    let src = "<?php\n$a = '<div class=\"x\">hi</div>';\n$b = '< 3';\n$c = '<?xml';\n";
    assert_eq!(langs("a.php", src), ["html"]);
}

#[test]
fn javascript_rules_cover_calls_templates_and_assignments() {
    let src = r#"
const rows = await db.query('SHOW TABLES');
const q = sql`SELECT * FROM users WHERE id = ${id}`;
const all = await prisma.$queryRaw`SELECT 1`;
el.innerHTML = '<li>' + name + '</li>';
const Button = styled.button`color: red;`;
const C = styled(Link)`padding: 0;`;
const data = JSON.parse('{"a": [1, 2]}');
const re = new RegExp('^\\d+$', 'g');
const lit = /SELECT * FROM x/g;
const t = html`<p>${text}</p>`;
const s = `UPDATE users SET name = ${name}`;
const n = a / b / 'SELECT * FROM nothing';
"#;
    let found = find_in("app.ts", src);
    let got: Vec<_> = found
        .iter()
        .map(|i| (i.language.as_str(), i.rule.as_deref().unwrap()))
        .collect();
    assert_eq!(
        got,
        [
            ("sql", "js-sql-calls"),
            ("sql", "js-sql-calls"),
            ("sql", "js-sql-calls"),
            ("html", "js-html"),
            ("css", "js-css"),
            ("css", "js-css"),
            ("json", "js-json"),
            ("regexp", "js-regexp"),
            ("html", "js-html"),
            ("sql", "js-sql-keywords"),
            ("sql", "js-sql-keywords"),
        ]
    );
    // `${id}` is left out of the injected ranges.
    assert_eq!(
        found[1]
            .ranges
            .iter()
            .map(|r| &src[r.from..r.to])
            .collect::<Vec<_>>(),
        ["SELECT * FROM users WHERE id = "]
    );
    // `'<li>' + name + '</li>'` is one injection.
    assert_eq!(found[3].ranges.len(), 2);
}

#[test]
fn literals_inside_template_expressions_are_not_hosts() {
    let src = "db.query(`SELECT * FROM t ${cond ? 'WHERE a' : ''}`);\n";
    let found = find_in("a.js", src);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].literals.len(), 1);
}

#[test]
fn rules_can_be_disabled_and_places_un_injected() {
    let src = "<?php\n$a = 'SELECT * FROM users';\n$b = 'SELECT * FROM orders';\n";
    let all = find_in("a.php", src);
    assert_eq!(all.len(), 2);
    let off = find(&FindRequest {
        text: src.into(),
        path: "a.php".into(),
        settings: InjectionSettings {
            disabled: vec!["php-sql-keywords".into()],
            ..Default::default()
        },
    });
    assert!(off.is_empty());
    let one = find(&FindRequest {
        text: src.into(),
        path: "a.php".into(),
        settings: InjectionSettings {
            disabled: vec![],
            uninjected: vec![format!("a.php#{}", all[0].key), "b.php#x".into()],
            ..Default::default()
        },
    });
    assert_eq!(one.len(), 1);
    assert!(one[0].text.contains("orders"));
}

#[test]
fn offsets_are_utf16() {
    let src = "<?php\n// 😀 ünïcode\n$a = 'SELECT * FROM users WHERE name = \"Jürgen\"';\n";
    let i = &find_in("a.php", src)[0];
    let units: Vec<u16> = src.encode_utf16().collect();
    let r = i.ranges[0];
    assert_eq!(
        String::from_utf16(&units[r.from..r.to]).unwrap(),
        "SELECT * FROM users WHERE name = \"Jürgen\""
    );
}

#[test]
fn escaping_round_trips_per_literal_kind() {
    let cases = [
        (
            LiteralKind::PhpSingle,
            r"it\'s a \\ and \d",
            "it's a \\ and \\d",
        ),
        (
            LiteralKind::PhpDouble,
            r#"say \"hi\"\n\$x \t\x41\101\u{1F600}"#,
            "say \"hi\"\n$x \tAA😀",
        ),
        (LiteralKind::PhpHeredoc, r#"a "b" \$c"#, "a \"b\" $c"),
        (LiteralKind::PhpNowdoc, r"raw \n \$", r"raw \n \$"),
        (LiteralKind::JsSingle, r"it\'s\nA\x42\`", "it's\nAB`"),
        (LiteralKind::JsTemplate, r"a \` \${x}", "a ` ${x}"),
    ];
    for (kind, src, value) in cases {
        let (text, map) = unescape(kind, src, 0, src.len());
        assert_eq!(text, value, "{kind:?}");
        assert_eq!(map.len(), text.len() + 1);
        assert_eq!(*map.last().unwrap(), src.len());
        // Escaping the value gives a literal with the same value.
        let back = escape(kind, &text);
        assert_eq!(unescape(kind, &back, 0, back.len()).0, value, "{kind:?}");
    }
    assert_eq!(escape(LiteralKind::JsDouble, "a\"\nb"), r#"a\"\nb"#);
    assert_eq!(escape(LiteralKind::PhpSingle, "it's"), r"it\'s");
    assert_eq!(escape(LiteralKind::PhpDouble, "$id"), r"\$id");
}

#[test]
fn fragments_map_value_offsets_to_the_source() {
    let src = "<?php\n$q = 'SELECT \\'a\\' FROM t';\n";
    let off = at(src, "FROM");
    let f = fragment(&OffsetRequest {
        text: src.into(),
        path: "a.php".into(),
        offset: off,
        settings: InjectionSettings::default(),
    })
    .unwrap();
    assert_eq!(f.text, "SELECT 'a' FROM t");
    assert_eq!(f.label, "SQL");
    assert_eq!(f.kind, LiteralKind::PhpSingle);
    assert_eq!(&src[f.from..f.to], "SELECT \\'a\\' FROM t");
    // `'` at value offset 7 comes from `\'` in the source.
    assert_eq!(&src[f.map[7]..f.map[8]], "\\'");
    assert_eq!(f.map[f.text.len()], f.to);
    // Not injected: no fragment.
    let none = fragment(&OffsetRequest {
        text: "<?php $a = 'hello';".into(),
        path: "a.php".into(),
        offset: 13,
        settings: InjectionSettings::default(),
    });
    assert!(none.is_none());
}

#[test]
fn pattern_fragments_leave_out_the_delimiters() {
    let src = "<?php preg_match('/\\d+/u', $s);";
    let f = fragment(&OffsetRequest {
        text: src.into(),
        path: "a.php".into(),
        offset: at(src, "d+"),
        settings: InjectionSettings::default(),
    })
    .unwrap();
    assert_eq!(f.text, "\\d+");
    assert_eq!(f.language, "regexp");
}

#[test]
fn inject_writes_a_language_comment() {
    let src = "<?php\nfunction a() {\n    $config = '{\"a\": 1}';\n    foo('x', '{}');\n}\n";
    let edit = inject(&InjectRequest {
        text: src.into(),
        path: "a.php".into(),
        offset: at(src, "\"a\""),
        language: "json".into(),
    })
    .unwrap();
    assert_eq!(edit.text, "    // language=JSON\n");
    assert_eq!(edit.from, at(src, "    $config"));
    let mut out = src.to_string();
    out.replace_range(edit.from..edit.to, &edit.text);
    assert_eq!(langs("a.php", &out), ["json"]);

    // Another literal before it on the line: an inline comment.
    let edit = inject(&InjectRequest {
        text: src.into(),
        path: "a.php".into(),
        offset: at(src, "{}") + 1,
        language: "JSON".into(),
    })
    .unwrap();
    assert_eq!(edit.text, "/* language=JSON */ ");
    assert_eq!(edit.from, at(src, "'{}'"));
    let mut out = src.to_string();
    out.replace_range(edit.from..edit.to, &edit.text);
    assert_eq!(
        summary("a.php", &out),
        [("json".into(), vec!["{}".to_string()])]
    );
}

#[test]
fn inject_replaces_an_existing_comment() {
    let src = "<?php\n// language=JSON\n$a = 'x';\n";
    let edit = inject(&InjectRequest {
        text: src.into(),
        path: "a.php".into(),
        offset: at(src, "x"),
        language: "html".into(),
    })
    .unwrap();
    assert_eq!(&src[edit.from..edit.to], "// language=JSON");
    assert_eq!(edit.text, "// language=HTML");
}

#[test]
fn context_actions_offer_inject_edit_and_un_inject() {
    let src = "<?php\n$a = 'plain';\n// language=SQL\n$b = 'users';\n$c = 'SELECT * FROM t';\n";
    let titles = |offset: usize| {
        context_actions(&OffsetRequest {
            text: src.into(),
            path: "a.php".into(),
            offset,
            settings: InjectionSettings::default(),
        })
    };
    let plain = titles(at(src, "plain"));
    assert_eq!(plain[0].title, "Inject language or reference");
    assert_eq!(plain[0].action.as_deref(), Some("InjectLanguageAction"));

    let commented = titles(at(src, "users"));
    assert_eq!(commented[0].title, "Edit SQL Fragment");
    assert_eq!(commented[1].title, "Un-inject Language");
    // Un-inject removes the comment line.
    let edit = commented[1].edit.as_ref().unwrap();
    let mut out = src.to_string();
    let e = &edit.edits_of("a.php")[0];
    let from = forge_codeintel::utf16_offset(src, e.range.start);
    let to = forge_codeintel::utf16_offset(src, e.range.end);
    out.replace_range(from..to, &e.new_text);
    assert_eq!(
        out,
        "<?php\n$a = 'plain';\n$b = 'users';\n$c = 'SELECT * FROM t';\n"
    );

    let detected = titles(at(src, "FROM"));
    assert_eq!(
        detected[1].action.as_deref(),
        Some("UnInjectLanguageAction")
    );
    // Outside strings: nothing.
    assert!(titles(at(src, "$a")).is_empty());
    assert!(titles(0).is_empty());
}

#[test]
fn injection_at_finds_the_literal_under_the_caret() {
    let src = "<?php\n$c = 'SELECT * FROM t' . $x . ' WHERE 1';\n";
    let req = |offset| OffsetRequest {
        text: src.into(),
        path: "a.php".into(),
        offset,
        settings: InjectionSettings::default(),
    };
    let i = injection_at(&req(at(src, "WHERE"))).unwrap();
    assert_eq!(i.language, "sql");
    assert_eq!(i.text, "SELECT * FROM t WHERE 1");
    assert!(injection_at(&req(at(src, "$x"))).is_none());
}

#[test]
fn every_rule_is_listed_once() {
    let infos = rule_infos();
    assert_eq!(infos.len(), RULES.len());
    let mut ids: Vec<_> = infos.iter().map(|r| r.id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), RULES.len());
    assert!(infos.iter().all(|r| !r.places.is_empty()));
    assert_eq!(languages().len(), LANGUAGES.len());
    assert_eq!(languages::language("MySQL").unwrap().id, "sql");
    assert_eq!(languages::language("regex").unwrap().label, "RegExp");
}

#[test]
fn language_attribute_places_inject_their_language() {
    let text = "<?php\n$repo->filter('id > 3 AND paid = 1');\nmatches('[a-z]+');\n$other->matches('[a-z]+');\n";
    let settings = InjectionSettings {
        places: vec![
            rules::InjectionPlace {
                name: "filter".into(),
                argument: 0,
                method: true,
                language: "SQL".into(),
            },
            rules::InjectionPlace {
                name: "App\\matches".into(),
                argument: 0,
                method: false,
                language: "RegExp".into(),
            },
        ],
        ..Default::default()
    };
    let found = find(&FindRequest {
        text: text.into(),
        path: "a.php".into(),
        settings: settings.clone(),
    });
    let got: Vec<(String, Option<String>)> = found
        .iter()
        .map(|i| (i.language.clone(), i.rule.clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "sql".to_string(),
                Some("php-language-attribute".to_string())
            ),
            (
                "regexp".to_string(),
                Some("php-language-attribute".to_string())
            ),
        ]
    );
    // The rule can be turned off like the built-in ones.
    let off = find(&FindRequest {
        text: text.into(),
        path: "a.php".into(),
        settings: InjectionSettings {
            disabled: vec!["php-language-attribute".into()],
            ..settings
        },
    });
    assert!(off
        .iter()
        .all(|i| i.rule.as_deref() != Some("php-language-attribute")));
}
