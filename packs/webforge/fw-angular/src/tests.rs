use super::*;
use webforge_lang_web::text::byte_to_utf16;
use webforge_lang_web::SourceFile;

const CARD_TS: &str = r#"import { Component, EventEmitter, Input, Output, input } from '@angular/core';
import { User } from '../models/user';

@Component({
  selector: 'app-user-card',
  standalone: true,
  templateUrl: './user-card.component.html',
  styleUrls: ['./user-card.component.scss', '../shared/card.scss'],
})
export class UserCardComponent {
  @Input({ required: true }) user!: User;
  @Input('compact') isCompact = false;
  size = input<'sm' | 'md'>('md');
  @Output() selected = new EventEmitter<User>();
}
"#;

const CARD_HTML: &str = r#"<div class="card" appHighlight="yellow">
  <h3>{{ user.name | titlecase }}</h3>
  <p>{{ user.createdAt | relativeTime }}</p>
</div>
"#;

const LIST_TS: &str = r#"import { Component, output } from '@angular/core';

@Component({
  selector: 'app-user-list',
  template: `
    <ul>
      @for (u of users; track u.id) {
        <app-user-card [user]="u" compact (selected)="pick.emit($event)" />
      }
    </ul>
  `,
  styleUrl: './user-list.component.css',
})
export class UserListComponent {
  users: User[] = [];
  pick = output<User>();
}
"#;

const HIGHLIGHT: &str = r#"import { Directive, Input } from '@angular/core';

@Directive({ selector: '[appHighlight]', standalone: true })
export class HighlightDirective {
  @Input() appHighlight = '';
}
"#;

const PIPE: &str = r#"import { Pipe, PipeTransform } from '@angular/core';

@Pipe({ name: 'relativeTime', standalone: true })
export class RelativeTimePipe implements PipeTransform {
  transform(value: Date): string { return ''; }
}
"#;

const APP_HTML: &str =
    "<main>\n  <app-user-list></app-user-list>\n  <app-user-card [user]=\"me\" />\n</main>\n";

fn sources() -> Sources {
    let f = |path: &str, content: &str| SourceFile {
        path: path.into(),
        content: content.into(),
    };
    Sources::new(
        vec![
            f("src/app/users/user-card.component.ts", CARD_TS),
            f("src/app/users/user-card.component.html", CARD_HTML),
            f("src/app/users/user-list.component.ts", LIST_TS),
            f("src/app/shared/highlight.directive.ts", HIGHLIGHT),
            f("src/app/shared/relative-time.pipe.ts", PIPE),
            f("src/app/app.component.ts", "@Component({ selector: 'app-root', templateUrl: './app.component.html' })\nexport class AppComponent {}\n"),
            f("src/app/app.component.html", APP_HTML),
        ],
        [
            "src/app/users/user-card.component.scss".to_string(),
            "src/app/shared/card.scss".to_string(),
        ],
    )
}

fn at(content: &str, needle: &str, delta: usize) -> usize {
    byte_to_utf16(content, content.find(needle).unwrap() + delta)
}

#[test]
fn scans_components_directives_and_pipes() {
    let d = scan_file("src/app/users/user-card.component.ts", CARD_TS).remove(0);
    let c = &d.component;
    assert_eq!(
        (c.name.as_str(), c.selector.as_deref()),
        ("UserCardComponent", Some("app-user-card"))
    );
    assert_eq!(
        c.template.as_ref().unwrap().path,
        "src/app/users/user-card.component.html"
    );
    let styles: Vec<_> = c.styles.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        styles,
        [
            "src/app/users/user-card.component.scss",
            "src/app/shared/card.scss"
        ]
    );
    let inputs: Vec<_> = c
        .props
        .iter()
        .map(|m| (m.name.as_str(), m.public_name(), m.required))
        .collect();
    assert_eq!(
        inputs,
        [
            ("user", "user", true),
            ("isCompact", "compact", false),
            ("size", "size", false)
        ]
    );
    assert_eq!(c.props[2].type_text.as_deref(), Some("'sm' | 'md'"));
    assert_eq!(c.events[0].name, "selected");

    let list = scan_file("src/app/users/user-list.component.ts", LIST_TS).remove(0);
    assert!(list.inline_template.is_some());
    assert_eq!(list.component.events[0].name, "pick");
    assert_eq!(
        list.component.styles[0].path,
        "src/app/users/user-list.component.css"
    );

    let pipe = scan_file("src/app/shared/relative-time.pipe.ts", PIPE).remove(0);
    assert_eq!(
        (pipe.component.kind, pipe.component.selector.as_deref()),
        (ComponentKind::Pipe, Some("relativeTime"))
    );
}

#[test]
fn goto_from_templates() {
    let s = sources();
    let index = AngularIndex::new(&s);
    // Element selector in an HTML template.
    let card = index.goto(
        "src/app/app.component.html",
        APP_HTML,
        at(APP_HTML, "<app-user-card", 3),
    );
    assert_eq!(
        (card[0].path.as_str(), card[0].line),
        ("src/app/users/user-card.component.ts", 9)
    );
    // Bindings in an inline template: input, aliased input and output.
    let goto = |needle: &str, delta: usize| {
        index.goto(
            "src/app/users/user-list.component.ts",
            LIST_TS,
            at(LIST_TS, needle, delta),
        )
    };
    assert_eq!(goto("[user]", 2)[0].line, 10);
    assert_eq!(goto("compact (", 2)[0].line, 11);
    assert_eq!(goto("(selected)", 3)[0].line, 13);
    assert_eq!(
        goto("<app-user-card", 5)[0].path,
        "src/app/users/user-card.component.ts"
    );
    // Attribute directive and pipe in the external template.
    let dir = index.goto(
        "src/app/users/user-card.component.html",
        CARD_HTML,
        at(CARD_HTML, "appHighlight", 3),
    );
    assert_eq!(
        (dir[0].path.as_str(), dir[0].line),
        ("src/app/shared/highlight.directive.ts", 4)
    );
    let pipe = index.goto(
        "src/app/users/user-card.component.html",
        CARD_HTML,
        at(CARD_HTML, "relativeTime", 2),
    );
    assert_eq!(
        (pipe[0].path.as_str(), pipe[0].line),
        ("src/app/shared/relative-time.pipe.ts", 3)
    );
    // Built-in pipe: nothing to go to.
    assert!(index
        .goto(
            "src/app/users/user-card.component.html",
            CARD_HTML,
            at(CARD_HTML, "titlecase", 2)
        )
        .is_empty());
}

#[test]
fn template_and_style_urls_open_files() {
    let s = sources();
    let index = AngularIndex::new(&s);
    let path = "src/app/users/user-card.component.ts";
    assert_eq!(
        index.goto(path, CARD_TS, at(CARD_TS, "user-card.component.html", 3)),
        [Location::file("src/app/users/user-card.component.html")]
    );
    assert_eq!(
        index.goto(path, CARD_TS, at(CARD_TS, "../shared/card.scss", 4)),
        [Location::file("src/app/shared/card.scss")]
    );
}

#[test]
fn usages_from_selector_and_class_name() {
    let s = sources();
    let index = AngularIndex::new(&s);
    let path = "src/app/users/user-card.component.ts";
    let from_selector = index.goto(path, CARD_TS, at(CARD_TS, "app-user-card", 2));
    let files: Vec<_> = from_selector.iter().map(|l| l.path.as_str()).collect();
    assert_eq!(
        files,
        [
            "src/app/app.component.html",
            "src/app/users/user-list.component.ts"
        ]
    );
    let from_name = index.goto(path, CARD_TS, at(CARD_TS, "UserCardComponent", 4));
    assert_eq!(from_name, from_selector);
    let counts: Vec<_> = index
        .components()
        .into_iter()
        .map(|c| (c.name, c.usages))
        .collect();
    assert!(counts.contains(&("UserCardComponent".to_string(), 2)));
    assert!(counts.contains(&("HighlightDirective".to_string(), 1)));
    assert!(counts.contains(&("RelativeTimePipe".to_string(), 1)));
}

#[test]
fn gutter_links_component_template_and_styles() {
    let s = sources();
    let index = AngularIndex::new(&s);
    let marks = index.gutter("src/app/users/user-card.component.ts", CARD_TS);
    let kinds: Vec<_> = marks
        .iter()
        .map(|m| (m.line, m.kind, m.targets.len()))
        .collect();
    assert_eq!(
        kinds,
        [(9, GutterKind::Template, 1), (9, GutterKind::Styles, 2)]
    );
    let back = index.gutter("src/app/users/user-card.component.html", CARD_HTML);
    assert_eq!(back[0].kind, GutterKind::Component);
    assert_eq!(back[0].targets[0].line, 9);
    let from_style = index.gutter("src/app/shared/card.scss", "");
    assert_eq!(from_style[0].title, "Go to component UserCardComponent");
    // Inline templates get no template marker; the stylesheet still does.
    let list = index.gutter("src/app/users/user-list.component.ts", LIST_TS);
    assert_eq!(
        list.iter().map(|m| m.kind).collect::<Vec<_>>(),
        [GutterKind::Styles]
    );
}

#[test]
fn selectors() {
    let parts = selector_parts("button[appRipple], [appRipple]:not(a), app-x");
    assert_eq!(
        parts[0],
        SelectorPart {
            element: Some("button".into()),
            attrs: vec!["appRipple".into()]
        }
    );
    assert_eq!(parts[1].element, None);
    assert_eq!(parts[2].element.as_deref(), Some("app-x"));
    assert_eq!(binding_name("[(value)]"), Some("value"));
    assert_eq!(binding_name("*ngIf"), Some("ngIf"));
    assert_eq!(binding_name("(click.prevent)"), Some("click"));
    assert_eq!(binding_name("#ref"), None);
}
