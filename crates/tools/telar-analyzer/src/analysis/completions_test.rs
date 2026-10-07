use super::*;
use std::path::PathBuf;

fn context_at_end(view_line: &str) -> Option<CompletionKind> {
    let source = format!("[view]\n{view_line}\n");
    completion_context(&source, 1, view_line.len() as u32)
}

fn project(library: bool, theme_fields: &[&str]) -> ProjectInfo {
    ProjectInfo {
        root: PathBuf::from("/nowhere"),
        component_root: PathBuf::from("/nowhere"),
        theme_type: (!library).then(|| "Palette".to_string()),
        theme_fields: theme_fields.iter().map(|f| f.to_string()).collect(),
        library,
        i18n_keys: HashSet::new(),
    }
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|item| item.label.as_str()).collect()
}

fn ra_item(label: &str, kind: CompletionItemKind) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        ..Default::default()
    }
}

fn component(name: &str, module: &str) -> PreludeComponent {
    PreludeComponent {
        name: name.to_string(),
        module: module.to_string(),
        detail: Some(format!("fn {name}(…)")),
        documentation: Some(Documentation::String(format!("The {name}."))),
    }
}

#[test]
fn a_theme_read_is_completed_wherever_markup_can_hold_one() {
    for line in [
        "box fill:$theme.",
        "box fill:$theme.pri",
        "box radius:$theme.",
        "box pad:($theme.",
        "text \"drawn in {$theme.",
        "text \"drawn in {$theme.na",
    ] {
        assert_eq!(
            context_at_end(line),
            Some(CompletionKind::ThemeToken),
            "{line}"
        );
    }
}

#[test]
fn theme_text_that_is_not_a_theme_read_is_left_alone() {
    assert_eq!(context_at_end("text \"costs $theme."), None);
    assert_eq!(context_at_end("text \"done {$x} $theme."), None);
    assert_eq!(
        context_at_end("box fill:my$theme."),
        Some(CompletionKind::ColorValue)
    );
    assert_eq!(
        context_at_end("box fill:$theme"),
        Some(CompletionKind::ColorValue)
    );
}

#[test]
fn a_library_reads_the_shared_theme_tokens() {
    let items = theme_items(Some(&project(true, &[])));
    let expected: Vec<&str> = telar_project::theme_tokens::all().collect();
    assert_eq!(labels(&items), expected);
    assert!(
        items
            .iter()
            .all(|item| item.detail.as_deref() == Some("ThemeTokens"))
    );
}

#[test]
fn an_application_reads_its_own_theme_fields() {
    let items = theme_items(Some(&project(false, &["surface", "brand", "ink"])));
    assert_eq!(labels(&items), ["brand", "ink", "surface"]);
    assert!(
        items
            .iter()
            .all(|item| item.detail.as_deref() == Some("Palette"))
    );
}

#[test]
fn outside_a_project_the_theme_offers_nothing() {
    assert!(theme_items(None).is_empty());
}

#[test]
fn a_component_is_a_function_exported_beside_its_props() {
    let items = [
        ra_item("button(…)", CompletionItemKind::FUNCTION),
        ra_item("ButtonProps", CompletionItemKind::STRUCT),
        ra_item("text_field", CompletionItemKind::FUNCTION),
        ra_item("TextFieldProps {…}", CompletionItemKind::STRUCT),
        ra_item("greeting_card", CompletionItemKind::MODULE),
        ra_item("greeting_card", CompletionItemKind::FUNCTION),
        ra_item("GreetingCardProps", CompletionItemKind::STRUCT),
        ra_item("helper", CompletionItemKind::FUNCTION),
        ra_item("OrphanProps", CompletionItemKind::STRUCT),
        ra_item("Navigator", CompletionItemKind::STRUCT),
    ];
    let found: Vec<String> = component_items(&items)
        .into_iter()
        .map(|item| PreludeComponent::from_item("widgets", item).name)
        .collect();
    assert_eq!(found, ["button", "text_field", "greeting_card"]);
}

#[test]
fn a_crate_with_no_components_contributes_no_tags() {
    let items = [
        ra_item("NavHost", CompletionItemKind::STRUCT),
        ra_item("Navigator", CompletionItemKind::STRUCT),
        ra_item("transition", CompletionItemKind::MODULE),
    ];
    assert!(component_items(&items).is_empty());
}

#[test]
fn a_prelude_component_is_offered_with_where_it_came_from() {
    let items = element_name_items(None, &[component("button", "telar_components")]);
    let button = items
        .iter()
        .find(|item| item.label == "button")
        .expect("the prelude component is offered");
    assert_eq!(
        button
            .label_details
            .as_ref()
            .and_then(|d| d.description.as_deref()),
        Some("telar_components")
    );
    assert_eq!(button.detail.as_deref(), Some("fn button(…)"));
    assert_eq!(
        button.documentation,
        Some(Documentation::String("The button.".to_string()))
    );
    assert!(items.iter().any(|item| item.label == "col"));
}

#[test]
fn every_tag_name_is_offered_once() {
    let dir = std::env::temp_dir().join(format!("telar-tag-names-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("badge.rsx"), "[view]\ncol\n").unwrap();
    std::fs::write(dir.join("src").join("card.rsx"), "[view]\ncol\n").unwrap();

    let items = element_name_items(
        Some(&dir),
        &[component("badge", "widgets"), component("text", "widgets")],
    );
    std::fs::remove_dir_all(&dir).ok();

    let badges: Vec<&CompletionItem> = items.iter().filter(|i| i.label == "badge").collect();
    assert_eq!(badges.len(), 1, "{:?}", labels(&items));
    assert!(
        badges[0].label_details.is_some(),
        "the prelude entry wins over the bare stem"
    );
    let texts: Vec<&CompletionItem> = items.iter().filter(|i| i.label == "text").collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(
        texts[0].kind,
        Some(CompletionItemKind::KEYWORD),
        "a built-in shadows a component of the same name"
    );
    assert!(items.iter().any(|i| i.label == "card"));
}
