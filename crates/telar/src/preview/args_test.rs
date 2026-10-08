use super::*;

fn state(args: &Args, name: &str) -> ArgState {
    args.states()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no arg `{name}`"))
}

#[test]
fn an_arg_with_no_override_is_its_default() {
    let args = Args::new();
    assert_eq!(args.arg("label", "Save"), "Save");
    assert!(!args.arg("ghost", false));
    assert_eq!(args.remounts(), 0);
}

#[test]
fn an_override_wins_over_the_default() {
    let args = Args::new();
    args.arg("size", 12u32);
    args.set("size", ArgValue::Int(20)).unwrap();
    assert_eq!(args.arg("size", 12u32), 20);
    assert_eq!(args.get("size"), Some(ArgValue::Int(20)));
}

#[test]
fn changing_an_arg_read_at_build_asks_for_a_remount() {
    let args = Args::new();
    args.arg("ghost", false);
    args.set("ghost", ArgValue::Bool(true)).unwrap();
    assert_eq!(args.remounts(), 1);

    args.set("ghost", ArgValue::Bool(true)).unwrap();
    assert_eq!(args.remounts(), 1, "the same value again changes nothing");

    args.set("ghost", ArgValue::Bool(false)).unwrap();
    assert_eq!(args.remounts(), 2, "back to the default is a change too");
    assert!(args.overrides().is_empty(), "and holds no override");
}

/// The remount count is a signal, which is what lets a canvas rebuild the moment it moves.
#[test]
fn the_remount_count_is_tracked() {
    let args = Args::new();
    args.arg("ghost", false);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0));
    let (reader, seen_by) = (args.clone(), std::rc::Rc::clone(&seen));
    effect(move || seen_by.set(reader.remounts()));
    args.set("ghost", ArgValue::Bool(true)).unwrap();
    assert_eq!(seen.get(), 1);
}

#[test]
fn a_live_arg_is_written_through_its_signal_without_a_remount() {
    let args = Args::new();
    let agree = args.signal("agree", false);
    args.set("agree", ArgValue::Bool(true)).unwrap();
    assert!(agree.peek());
    assert_eq!(args.remounts(), 0);
}

#[test]
fn the_trees_writes_reach_the_control() {
    let args = Args::new();
    let count = args.signal("count", 0u32);
    count.set(3);
    assert_eq!(args.get("count"), Some(ArgValue::Int(3)));
    assert_eq!(
        args.overrides(),
        vec![("count".to_string(), ArgValue::Int(3))]
    );
    count.set(0);
    assert!(
        args.overrides().is_empty(),
        "back at its default it is no override"
    );
}

#[test]
fn a_live_arg_keeps_its_signal_across_builds() {
    let args = Args::new();
    let first = args.signal("count", 0u32);
    first.set(5);
    let again = args.signal("count", 0u32);
    assert_eq!(again.peek(), 5);
    again.set(6);
    assert_eq!(first.peek(), 6, "one signal, not two");
}

#[test]
fn a_live_arg_starts_from_an_override_set_before_it_was_read() {
    let args = Args::new();
    args.set("agree", ArgValue::Bool(true)).unwrap();
    assert!(args.signal("agree", false).peek());
}

#[test]
fn reset_puts_every_arg_back() {
    let args = Args::new();
    args.arg("label", "Save");
    let count = args.signal("count", 0u32);
    args.set("label", ArgValue::Text("Send".into())).unwrap();
    count.set(4);
    let before = args.remounts();

    args.reset();
    assert_eq!(args.arg("label", "Save"), "Save");
    assert_eq!(count.peek(), 0);
    assert!(args.overrides().is_empty());
    assert_eq!(args.remounts(), before + 1, "the label was read at build");
}

#[test]
fn resetting_only_live_args_does_not_remount() {
    let args = Args::new();
    args.arg("label", "Save");
    args.signal("count", 0u32).set(4);
    args.reset();
    assert_eq!(args.remounts(), 0);
}

#[test]
fn a_value_the_type_cannot_hold_is_refused() {
    let args = Args::new();
    args.arg("ghost", false);
    args.signal("count", 0u8);
    assert_eq!(
        args.set("ghost", ArgValue::Int(1)),
        Err(ArgError::Rejected {
            name: "ghost".into(),
            value: ArgValue::Int(1)
        })
    );
    assert!(args.set("count", ArgValue::Int(300)).is_err());
    assert_eq!(args.remounts(), 0);
    assert!(args.overrides().is_empty());
}

#[test]
fn a_read_only_arg_refuses_every_value() {
    let args = Args::new();
    args.read_only("children", Some("Children(2)".into()));
    assert_eq!(
        args.set("children", ArgValue::Unset),
        Err(ArgError::ReadOnly {
            name: "children".into()
        })
    );
    let row = state(&args, "children");
    assert_eq!(row.control, ControlKind::ReadOnly);
    assert_eq!(row.value, None);
    assert_eq!(row.shown.as_deref(), Some("Children(2)"));
}

#[test]
fn states_list_declared_args_first_then_the_ones_read() {
    static SPECS: &[ArgSpec] = &[ArgSpec::new("agree")
        .binding(ArgBinding::Live)
        .default("false")
        .control(|| ControlKind::Toggle)
        .doc("Whether the terms are accepted.")];
    let args = Args::new();
    args.declare(SPECS);
    let declared = state(&args, "agree");
    assert_eq!(declared.binding, ArgBinding::Live);
    assert_eq!(declared.control, ControlKind::Toggle);
    assert_eq!(declared.default, Some(ArgValue::Bool(false)));
    assert_eq!(declared.value, Some(ArgValue::Bool(false)));
    assert_eq!(declared.doc, "Whether the terms are accepted.");

    args.arg("label", "Save");
    args.signal("agree", false);
    let names: Vec<_> = args.states().iter().map(|s| s.name).collect();
    assert_eq!(names, ["agree", "label"]);
    assert_eq!(state(&args, "agree").doc, "Whether the terms are accepted.");
}

#[test]
fn a_row_says_whether_it_was_edited() {
    let args = Args::new();
    args.arg("size", 12u32);
    assert!(!state(&args, "size").edited);
    args.set("size", ArgValue::Int(14)).unwrap();
    let row = state(&args, "size");
    assert!(row.edited);
    assert_eq!(row.value, Some(ArgValue::Int(14)));
    assert_eq!(row.default, Some(ArgValue::Int(12)));
}

#[test]
fn a_new_arg_reaches_a_panel_listing_them() {
    let args = Args::new();
    let rows = std::rc::Rc::new(std::cell::Cell::new(0));
    let (reader, rows_seen) = (args.clone(), std::rc::Rc::clone(&rows));
    effect(move || rows_seen.set(reader.states().len()));
    args.arg("label", "Save");
    assert_eq!(rows.get(), 1);
    args.arg("ghost", false);
    assert_eq!(rows.get(), 2);
}

/// What a hot reload does: the outgoing build's overrides are snapshotted by key and handed to the next one's args.
#[cfg(any(feature = "dev", feature = "prerender"))]
#[test]
fn persisted_overrides_survive_a_reload() {
    let before = Args::persisted("args-test--reload");
    before.arg("label", "Save");
    before.signal("count", 0u32).set(2);
    before.set("label", ArgValue::Text("Send".into())).unwrap();

    crate::hot_state::hot_restore_json(&crate::hot_state::hot_snapshot_json());

    let after = Args::persisted("args-test--reload");
    assert_eq!(after.arg("label", "Save"), "Send");
    assert_eq!(after.signal("count", 0u32).peek(), 2);
}

#[test]
fn an_override_the_type_no_longer_holds_falls_back_to_the_default() {
    let args = Args::new();
    args.set("size", ArgValue::Text("big".into())).unwrap();
    assert_eq!(args.arg("size", 12u32), 12);
}

static SIZE_SPEC: &[ArgSpec] = &[ArgSpec::new("size")
    .default("4")
    .control(|| ControlKind::INTEGER.range(0.0, 10.0))
    .doc("How big, declared.")];

#[test]
fn reading_a_declared_arg_keeps_what_was_declared() {
    let args = Args::new();
    args.declare(SIZE_SPEC);
    assert_eq!(args.arg("size", 2u32), 4, "the declared default stands");
    let row = state(&args, "size");
    assert_eq!(row.control, ControlKind::INTEGER.range(0.0, 10.0));
    assert_eq!(row.doc, "How big, declared.");
    assert_eq!(row.default, Some(ArgValue::Int(4)));
    assert!(!row.edited);
}

#[test]
fn building_again_lists_nothing_new() {
    let args = Args::new();
    args.declare(SIZE_SPEC);
    let listed = std::rc::Rc::new(std::cell::Cell::new(0));
    let (reader, counted) = (args.clone(), std::rc::Rc::clone(&listed));
    effect(move || {
        reader.states();
        counted.set(counted.get() + 1);
    });
    let before = listed.get();
    for _ in 0..3 {
        args.arg("size", 2u32);
        args.signal("count", 0u32);
    }
    assert_eq!(
        listed.get(),
        before + 1,
        "only the first read of `count` adds a row"
    );
}

static FIELDS: &[PropField] = &[
    PropField::new("size", "f32", || ControlKind::FLOAT.range(0.0, 1.0))
        .doc("How big, from the prop.")
        .default(PropDefault::Expr("0.5")),
    PropField::new("hint", "Option<String>", || {
        ControlKind::Optional(&ControlKind::MULTILINE)
    })
    .takes_some(),
    PropField::new("label", "Reactive<String>", || ControlKind::TEXT).doc("What it says."),
];
static PROPS: PropsSchema = PropsSchema::new("DotProps", "").fields(FIELDS);

fn linked() -> Args {
    let args = Args::new();
    args.link_props(&PROPS);
    args
}

#[test]
fn an_arg_named_after_a_prop_takes_its_doc_control_and_default() {
    let args = linked();
    args.declare(&[ArgSpec::new("size")]);
    let row = state(&args, "size");
    assert_eq!(row.doc, "How big, from the prop.");
    assert_eq!(row.control, ControlKind::FLOAT.range(0.0, 1.0));
    assert_eq!(row.default, Some(ArgValue::Float(0.5)));
    assert!(std::ptr::eq(row.prop.expect("linked by name"), &FIELDS[0]));
}

#[test]
fn the_prop_refines_the_control_and_the_preview_s_value_is_the_default() {
    let args = linked();
    assert_eq!(args.arg("size", 0.25f32), 0.25);
    let row = state(&args, "size");
    assert_eq!(row.control, ControlKind::FLOAT.range(0.0, 1.0));
    assert_eq!(row.default, Some(ArgValue::Float(0.25)));
    assert_eq!(row.doc, "How big, from the prop.");
}

#[test]
fn a_some_prop_refines_the_control_inside_its_option() {
    let args = linked();
    args.arg("hint", String::from("Tip"));
    assert_eq!(state(&args, "hint").control, ControlKind::MULTILINE);
}

#[test]
fn a_prop_of_another_kind_leaves_the_type_s_control() {
    let args = linked();
    args.arg("label", false);
    let row = state(&args, "label");
    assert_eq!(row.control, ControlKind::Toggle);
    assert_eq!(row.doc, "What it says.");
}

#[test]
fn a_declaration_wins_over_the_prop() {
    let args = linked();
    args.declare(SIZE_SPEC);
    args.arg("size", 0.25f32);
    let row = state(&args, "size");
    assert_eq!(row.doc, "How big, declared.");
    assert_eq!(row.control, ControlKind::INTEGER.range(0.0, 10.0));
    assert_eq!(row.default, Some(ArgValue::Int(4)));
}

#[test]
fn an_arg_with_no_prop_of_its_name_has_none() {
    let args = linked();
    args.arg("other", 1u8);
    assert!(state(&args, "other").prop.is_none());
}

fn empty(_: &crate::preview::PreviewCtx) -> Result<Box<dyn crate::LayoutItem>, crate::LayoutError> {
    Ok(Box::new(crate::Container::new(
        crate::LayoutStyle::new(),
        Vec::new(),
    )?))
}

#[test]
fn an_entry_s_args_are_declared_and_linked_to_its_props() {
    let entry = PreviewEntry::new("args-test--dot--linked", "dot", "Linked", empty)
        .props(|| &PROPS)
        .args(SIZE_SPEC);
    let args = Args::for_entry(&entry);
    let names: Vec<_> = args.states().iter().map(|s| s.name).collect();
    assert_eq!(names, ["size"]);
    args.arg("label", "Hi");
    let label = state(&args, "label");
    assert_eq!(label.doc, "What it says.");
    assert!(std::ptr::eq(label.prop.unwrap(), &FIELDS[2]));
}
