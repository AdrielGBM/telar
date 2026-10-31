use super::signals::normalize_closure;
use super::*;

fn make_gen<'a>() -> ViewGen<'a> {
    ViewGen::new(&[], None)
}

#[test]
fn literal_content() {
    let g = make_gen();
    assert!(matches!(
        g.interpolation("hello {{x}}", 0, true),
        super::interp::Interpolated::Literal(text) if text == "hello {x}"
    ));
}

#[test]
fn signal_interpolation() {
    let g = make_gen();
    let super::interp::Interpolated::Formatted {
        format, reactive, ..
    } = g.interpolation("Count: {$count}", 0, false)
    else {
        panic!("a hole formats");
    };
    assert_eq!(
        format,
        r#"format!("Count: {}", /*@RSX@EXPR:9:5@*/count.get())"#
    );
    assert!(reactive);
}

#[test]
fn closure_passthrough() {
    assert_eq!(
        normalize_closure("|| count.update(|n| *n += 1)"),
        "|| count.update(|n| *n += 1)"
    );
}

#[test]
fn a_non_reactive_branch_still_takes_a_widget() {
    let src = "[logic]\nlet icon = make_icon()?;\nlet vertical = true;\n[view]\nrow\n    if vertical\n        widget \"icon\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        !out.rust_code.contains("compile_error!"),
        "a one-shot `if` is not a reactive region:\n{}",
        out.rust_code
    );
}

/// `field: Type = expr` sugar reaches the builder as the attribute that says the same thing. The struct stops deriving `Default` along the way: nothing constructs a `Props` that way any more, and keeping it would demand a `Default` of every field type the builder does not need one for.
#[test]
fn an_inline_field_default_becomes_a_props_attribute() {
    let src = "[logic]\n#[derive(Default)]\npub struct Props {\n    pub gap: f32,\n    pub pad: f32 = 16.0,\n}\n[view]\ntext \"x\"\n";
    let out = crate::transpile_source(src, "card", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("pub pad: f32,"),
        "the `= 16.0` must come off the field — Rust has no field defaults:\n{code}"
    );
    assert!(
        code.contains("#[props(default = 16.0)]"),
        "and land on the attribute that carries it:\n{code}"
    );
    assert!(
        code.contains("#[props(default)]"),
        "a field the struct defaulted wholesale is optional too:\n{code}"
    );
    assert!(
        code.contains("#[derive(::telar::Props)]") && !code.contains("#[derive(Default)]"),
        "the builder replaces `Default` rather than sitting beside it:\n{code}"
    );
}

#[test]
fn unknown_tag_becomes_component_call() {
    let src = "[logic]\n[view]\nmy_card\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code
            .contains("my_card(MyCardProps::props().build())?"),
        "no-attr tag should call fn directly"
    );
}

#[test]
fn class_paint_promotes_container_and_is_consumed() {
    let src = "[style]\n@card\n    fill: #ffffff\n    radius: 12\n    padding: 8\n[view]\ncol @card\n    text \"hi\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("StyledContainer::new(style_card()"),
        "painted col should be a StyledContainer:\n{code}"
    );
    assert!(
        code.contains("with_fill"),
        "class fill should reach the RectStyle:\n{code}"
    );
}

#[test]
fn multiple_classes_compose_layout_and_paint() {
    let src = "[style]\n@a\n    align: center\n@b\n    align: start\n    fill: #ff0000\n[view]\nbox @a @b\n    text \"hi\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("style_a().align_items(AlignItems::START)"),
        "second class's layout should compose on top of the first:\n{code}"
    );
    assert!(
        code.contains("with_fill"),
        "a class's fill should reach the RectStyle:\n{code}"
    );
}

#[test]
fn classed_box_is_a_flex_container() {
    // A `style_*()` class fn is `LayoutStyle::new()` (display:block), so a classed `box` still needs `.flex_column()` for its children to centre.
    let src = "[style]\n@center\n    align: center\n    justify: center\n[view]\nbox @center width:100 height:60\n    text \"hi\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains("style_center().flex_column()"),
        "a classed box must be a flex container so align/justify apply:\n{}",
        out.rust_code
    );
}

#[test]
fn container_on_press_emits_click_handler() {
    let src = "[logic]\nlet n = signal(0i32);\n[view]\ncol\n    box fill:primary on_press:(|| $n.update(|v| *v += 1))\n    col on_press:(|| $n.set(0))\n        text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.matches(".on_press(").count() >= 2,
        "both box and col should emit .on_press:\n{code}"
    );
    assert!(
        code.contains("let n = n.clone();"),
        "on_press closure should clone the captured signal:\n{code}"
    );
    assert!(
        code.contains("n.update(") && code.contains("n.set(0)"),
        "$n should be substituted to the handle:\n{code}"
    );
}

#[test]
fn a_forwarded_on_press_on_a_plain_row_upgrades_to_styled_container() {
    let src = "[logic]\npub struct Props {\n    pub on_press: Box<dyn Fn()>,\n}\n\n[view]\nrow on_press:(props.on_press)\n    text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("StyledContainer::new(") && code.contains(".maybe_on_press("),
        "a forwarded on_press must upgrade a plain row to StyledContainer:\n{code}"
    );
}

// Regression: `lazy` got its column from `set_display(true)` forcing `Display::Flex`; once that call began restoring the declared display, the tag had to declare it.
#[test]
fn lazy_declares_a_flex_column() {
    let src = "[logic]\nlet open = signal(false);\n[view]\nlazy when:$open\n    text \"a\"\n    text \"b\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    let lazy_call = code
        .split_once("Lazy::new(")
        .expect("lazy must emit Lazy::new")
        .1;
    assert!(
        lazy_call
            .trim_start()
            .starts_with("LayoutStyle::new().flex_column()"),
        "lazy's container style must be a flex column, not the Display::Block default:\n{code}"
    );
}

#[test]
fn compound_assign_sugar_rewrites_to_update() {
    let src = "[logic]\nlet count = signal(0i32);\n[view]\ncol\n    button on_press:(|| $count += 1)\n    button on_press:(|| $count -= 2)\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("count.update(|__v| *__v += 1)"),
        "+= should desugar to update:\n{code}"
    );
    assert!(
        code.contains("count.update(|__v| *__v -= 2)"),
        "-= should desugar to update:\n{code}"
    );
}

#[test]
fn toggle_and_update_closures_pass_through() {
    let src = "[logic]\nlet flag = signal(false);\nlet count = signal(0i32);\n[view]\ncol\n    button on_press:(|| $flag.toggle())\n    button on_press:(|| $count.update(|n| *n += 1))\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("flag.toggle()"),
        "toggle passes through:\n{code}"
    );
    assert!(
        code.contains("count.update(|n| *n += 1)"),
        ".update must be left untouched:\n{code}"
    );
}

#[test]
fn quoted_escape_decodes_then_reemits() {
    let src = "[logic]\n[view]\ntext \"say \\\"hi\\\"\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains(r#"say \"hi\""#),
        "escaped quotes should round-trip:\n{code}"
    );
}

#[test]
fn paren_attr_form_captures_nested_and_coexists() {
    let src = "[logic]\nlet count = signal(0i32);\n[view]\nbox fill:primary transition(fill 200ms ease-out) on_press:(|| $count.update(|n| *n += 1))\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains(".on_press("),
        "paren on_press should emit a handler:\n{code}"
    );
    assert!(
        code.contains("count.update(|n| *n += 1)"),
        "nested-paren closure should be captured whole:\n{code}"
    );
    assert!(
        code.contains("motion::Animated::new"),
        "transition should still wire even when it precedes on_press:\n{code}"
    );
}

#[test]
fn overlay_builds_overlay_widget() {
    let src = "[view]\ncol\n    text \"behind\"\n    overlay align:center justify:center\n        text \"on top\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("Overlay::new("),
        "overlay emits an Overlay widget:\n{code}"
    );
    assert!(
        code.contains(".align_items(") && code.contains(".justify_content("),
        "overlay forwards layout attrs for positioning content:\n{code}"
    );
}

#[test]
fn layer_builds_a_fixed_layer_laid_out_like_an_overlay() {
    let src = "[view]\ncol\n    layer justify:end pad:16\n        row height:48\n            text \"bar\"\n    text \"page\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("FixedLayer::new(LayoutStyle::new().flex_column()"),
        "a layer is a column, as an overlay is:\n{code}"
    );
    assert!(
        code.contains(".justify_content(") && code.contains(".padding_all("),
        "and takes the layout keys that place its boxes against the surface:\n{code}"
    );
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn box_hover_emits_hover_style() {
    let src = "[view]\nbox fill:#101010 hover_style(fill:#f0f0f0 stroke:#ff0000) radius:10\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains(".hover_style("),
        "hover(...) should wire hover_style:\n{}",
        out.rust_code
    );
}

#[test]
fn on_drag_wires_and_upgrades_container() {
    let src = "[logic]\nlet x = signal(0.0f32);\n[view]\ncol on_drag:(|px, _py| $x.set(px))\n    text \"t\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("StyledContainer::new("),
        "on_drag upgrades a plain col to a StyledContainer:\n{code}"
    );
    assert!(
        code.contains(".on_drag(") && code.contains("move |px, _py| x.set(px)"),
        "on_drag wires the handler (with the $signal cloned in):\n{code}"
    );
}

#[test]
fn plain_col_with_hover_upgrades_to_styled_container() {
    let src = "[view]\ncol hover_style(fill:#f0f0f0)\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("StyledContainer::new("),
        "a col with hover should become a StyledContainer:\n{code}"
    );
    assert!(
        code.contains(".hover_style("),
        "and wire hover_style:\n{code}"
    );
}

#[test]
fn box_active_style_emits_active_style() {
    let src = "[view]\nbox fill:#101010 active_style(fill:#303030) radius:10\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains(".active_style("),
        "active_style(...) should wire active_style:\n{}",
        out.rust_code
    );
}

#[test]
fn plain_col_with_active_style_upgrades_to_styled_container() {
    let src = "[view]\ncol active_style(fill:#303030)\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("StyledContainer::new(") && code.contains(".active_style("),
        "a col with only active_style should become a StyledContainer and wire it:\n{code}"
    );
}

const SLOTTED_PROPS: &str = "[logic]\npub struct Props {\n    #[props(default)]\n    pub children: Children,\n    #[props(default)]\n    pub header: Children,\n}\n";

#[test]
fn a_component_takes_only_its_props() {
    let src =
        format!("{SLOTTED_PROPS}[view]\nbox fill:#101010 pad:16\n    children $props.children\n");
    let code = crate::transpile_source(&src, "card", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("pub fn card(props: CardProps) -> Result<Box<dyn LayoutItem>, LayoutError>"),
        "{code}"
    );
    assert!(!code.contains("children: Children)"), "{code}");
}

#[test]
fn a_placeholder_builds_the_children_prop_it_names_in_place() {
    let src = format!(
        "{SLOTTED_PROPS}[view]\nbox pad:16\n    children $props.header\n    text \"between\"\n    children $props.children\n"
    );
    let code = crate::transpile_source(&src, "panel", None, None)
        .unwrap()
        .rust_code;
    let header = code
        .find("__children.extend(props.header.build()?);")
        .unwrap_or_else(|| panic!("the header is built where it is placed:\n{code}"));
    let between = code
        .find("\"between\"")
        .expect("the text sibling is emitted");
    let body = code
        .find("__children.extend(props.children.build()?);")
        .unwrap_or_else(|| panic!("and so are the bare children:\n{code}"));
    assert!(
        header < between && between < body,
        "in source order:\n{code}"
    );
    if let Err(e) = syn::parse_file(&code) {
        panic!("generated code does not parse: {e}\n{code}");
    }
}

#[test]
fn a_bare_or_named_placeholder_is_an_error_naming_the_prop_it_places() {
    let code = crate::transpile_source("[view]\ncol\n    children\n", "panel", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("compile_error!(\"`children` places a `Children` prop: write `children $props.children`, with `#[props(default)] pub children: Children` in `Props`\")"),
        "{code}"
    );
    assert!(!code.contains(".build()?"), "{code}");

    let code = crate::transpile_source(
        "[view]\ncol\n    children name:\"header\"\n",
        "panel",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(
        code.contains("compile_error!(\"`children` places a `Children` prop: write `children $props.header`, with `#[props(default)] pub header: Children` in `Props`\")"),
        "{code}"
    );
    assert_eq!(
        code.matches("compile_error!").count(),
        1,
        "`name:` is explained once, not also refused as an unknown key:\n{code}"
    );
}

#[test]
fn bare_children_at_a_call_become_the_children_prop() {
    let code = crate::transpile_source("[view]\ncard\n    text \"hi\"\n", "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("let __recipe_children = Children::new("),
        "{code}"
    );
    assert!(
        code.contains("card(CardProps::props().children(__recipe_children).build())?"),
        "{code}"
    );
    assert!(code.contains("Ok(__children)"), "{code}");
}

#[test]
fn a_child_routed_to_a_slot_becomes_that_prop() {
    let code = crate::transpile_source(
        "[view]\npanel\n    text \"T\" slot:\"header\"\n    text \"B\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(
        code.contains(
            "panel(PanelProps::props().header(__recipe_header).children(__recipe_children).build())?"
        ),
        "{code}"
    );
    let header = code.find("let __recipe_header").expect("a header recipe");
    let children = code
        .find("let __recipe_children")
        .expect("a children recipe");
    let title = code.find("\"T\"").expect("the header text");
    let body = code.find("\"B\"").expect("the body text");
    assert!(
        header < title && title < children && children < body,
        "each child sits in its own prop's recipe:\n{code}"
    );
    assert!(!code.contains("slot"), "`slot:` is consumed:\n{code}");
}

#[test]
fn a_call_with_no_children_sets_no_children() {
    let code = crate::transpile_source("[view]\ncard title:\"x\"\n", "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("card(CardProps::props().title(\"x\").build())?"),
        "{code}"
    );
    assert!(
        !code.contains(".children(") && !code.contains("Children::new"),
        "{code}"
    );
}

#[test]
fn a_slot_that_names_no_field_is_an_error_on_its_line() {
    let code = crate::transpile_source(
        "[view]\ncard\n    text \"x\" slot:\"side-bar\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(
        code.contains(
            "compile_error!(\"`slot:\\\"side-bar\\\"` names the prop this child is handed through"
        ),
        "{code}"
    );
}

#[test]
fn a_view_let_reaches_every_recipe_after_it() {
    let code = crate::transpile_source(
        "[view]\ncard\n    let label = \"hi\"\n    text \"{label}\" slot:\"header\"\n    text \"{label}\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert_eq!(
        code.matches("let label = \"hi\";").count(),
        2,
        "both recipes bind it:\n{code}"
    );
}

#[test]
fn component_bool_flag_prop() {
    let src = "[view]\ncard elevated\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("CardProps::props().elevated(true).children(__recipe_children).build()"),
        "a bare flag prop should be bool true:\n{code}"
    );
}

#[test]
fn component_signal_prop_clones_handle() {
    let src = "[logic]\nlet count = signal(0i32);\n[view]\ncard count:$count\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains(".count(count.clone())"),
        "a $signal prop should pass the cloned handle:\n{code}"
    );
}

#[test]
fn component_closure_prop_is_boxed() {
    let src = "[logic]\nlet count = signal(0i32);\n[view]\ncard on_tap:(|| $count += 1)\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains("Box::new("),
        "a closure prop should be boxed:\n{code}"
    );
    assert!(
        code.contains("count.update(|__v| *__v += 1)"),
        "the closure's $ sugar should desugar:\n{code}"
    );
}

#[test]
fn component_free_closure_prop_carries_source_span() {
    let src = "[view]\ncard on_tap:(|| toggle())\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(!out.rust_code.contains("@RSX@"), "markers must be stripped");
    let span = out
        .expr_spans
        .iter()
        .find(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize] == "|| toggle()")
        .expect("closure prop should emit a source span");
    let gen_frag = &out.rust_code[span.gen_start as usize..(span.gen_start + span.len) as usize];
    assert_eq!(
        gen_frag, "|| toggle()",
        "span must map verbatim onto the generated closure"
    );
}

#[test]
fn text_rich_weight_italic_align() {
    let src = "[view]\ntext \"Hi\" font_weight:bold font_style:italic text_align:center\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains(".with_font_weight(700)"),
        "weight keyword:\n{code}"
    );
    assert!(
        code.contains(".with_font_style(FontStyle::Italic)"),
        "slant:\n{code}"
    );
    assert!(
        code.contains(".with_text_align(TextAlign::Center)"),
        "align:\n{code}"
    );
}

#[test]
fn text_rich_numeric_weight_and_align_end() {
    let src = "[view]\ntext \"Hi\" font_weight:600 text_align:right\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(".with_font_weight(600)"),
        "numeric weight:\n{code}"
    );
    assert!(
        code.contains(".with_text_align(TextAlign::End)"),
        "align end:\n{code}"
    );
    assert!(
        !code.contains(".with_font_style"),
        "no slant when absent:\n{code}"
    );
}

#[test]
fn text_lines_and_ellipsis() {
    let src = "[view]\ntext \"Long copy here\" lines:2 ellipsis max_width:200\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(code.contains(".with_clamp(2, true)"), "{code}");

    let unclamped = "[view]\ntext \"Long copy here\" ellipsis\n";
    let code = crate::transpile_source(unclamped, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("with_clamp"), "{code}");
}

#[test]
fn input_binds_value_style_and_submit() {
    let src = "[logic]\nlet name = signal(String::new());\n[view]\ninput value:$name font_size:16 color:$theme.primary width:200 on_submit:(|| $name.set(String::new()))\n";
    let code = crate::transpile_source(src, "demo", Some("SandboxTheme"), None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("Input::declaring(name.clone(),"),
        "binds the value signal (cloned):\n{code}"
    );
    assert!(
        code.contains(".with_font_size(16.0).with_color(theme.get().primary)"),
        "amends what the tree declared rather than building a style from nothing:\n{code}"
    );
    assert!(
        code.contains(".width(200"),
        "forwards layout attrs:\n{code}"
    );
    assert!(code.contains(".on_submit("), "wires on_submit:\n{code}");
}

#[test]
fn reactive_for_emits_reactive_list() {
    let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items key *n\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("fragment(") && !code.contains("ReactiveList"),
        "a reactive for in a container should be a transparent fragment, not a boxed list:\n{code}"
    );
    assert!(
        code.contains("Container::from_slots("),
        "the host container collects slots:\n{code}"
    );
    assert!(
        code.contains("move || items.get()"),
        "source reads the signal:\n{code}"
    );
    assert!(
        code.contains("|n| *n"),
        "key closure from `key *n`:\n{code}"
    );
    assert!(code.contains("move |n|"), "item builder closure:\n{code}");
}

#[test]
fn reactive_for_without_key_uses_positional_reconciliation() {
    let src = "[view]\ncol\n    for n in $items\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("fragment_positional("),
        "a keyless reactive for should reconcile by position:\n{code}"
    );
    assert!(
        !code.contains("compile_error!"),
        "a keyless reactive for must compile, not error:\n{code}"
    );
}

#[test]
fn reactive_for_in_row_is_transparent_between_static_siblings() {
    let src = "[logic]\nlet ws = signal(vec![1i32, 2]);\n[view]\nrow\n    text \"L\"\n    for w in $ws key *w\n        text \"x\"\n    text \"R\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("Container::from_slots(") && code.contains("flex_row"),
        "the row hosts slots and keeps its row axis:\n{code}"
    );
    assert!(
        code.contains("fragment(") && !code.contains("ReactiveList"),
        "the reactive for is a transparent fragment, not a boxed list:\n{code}"
    );
    assert!(
        code.contains("ChildSlot::stat(") && code.contains("__slots.push("),
        "static siblings and the fragment share the row's slot collection:\n{code}"
    );
}

#[test]
fn reactive_for_with_gap_in_row_is_transparent_gap_fragment() {
    let src = "[logic]\nlet ws = signal(vec![1i32, 2]);\n[view]\nrow\n    for w in $ws key *w gap:6\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("fragment(") && !code.contains("ReactiveList"),
        "a reactive for with gap in a slot host is a transparent gap fragment, not a boxed list:\n{code}"
    );
    assert!(
        code.contains("(6) as f32"),
        "the gap value is passed to fragment:\n{code}"
    );
    assert!(
        code.contains("Container::from_slots(") && code.contains("flex_row"),
        "the host row keeps its row axis and hosts slots:\n{code}"
    );
}

#[test]
fn reactive_for_with_gap_keyless_uses_positional_gap_fragment() {
    let src = "[view]\nrow\n    for w in $ws gap:4\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("fragment_positional(") && code.contains("(4) as f32"),
        "a keyless reactive for with gap is a positional gap fragment:\n{code}"
    );
}

#[test]
fn static_for_stays_construction_loop() {
    let src = "[view]\ncol\n    for n in 0..3\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        !code.contains("ReactiveList"),
        "a plain for must not be reactive:\n{code}"
    );
    assert!(
        code.contains("for n in 0..3"),
        "construction loop preserved:\n{code}"
    );
}

#[test]
fn reactive_if_emits_reactive_list() {
    let src = "[logic]\nlet show = signal(true);\n[view]\ncol\n    if $show\n        text \"yes\"\n    else\n        text \"no\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("fragment(") && !code.contains("ReactiveList"),
        "a reactive if in a container should be a transparent fragment:\n{code}"
    );
    assert!(
        code.contains("move || vec![show.get()]"),
        "condition read as source:\n{code}"
    );
    assert!(
        code.contains("|__cond: &bool| *__cond"),
        "bool key:\n{code}"
    );
    assert!(code.contains("if __cond"), "branch selector:\n{code}");
}

#[test]
fn static_if_stays_construction() {
    let src = "[view]\ncol\n    if some_flag\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        !code.contains("ReactiveList"),
        "a plain if must not be reactive:\n{code}"
    );
    assert!(
        code.contains("some_flag"),
        "condition emitted verbatim:\n{code}"
    );
}

// An injected `Container::column` wrapper would trap a `row align:stretch` root at content height.
#[test]
fn root_static_if_returns_branch_directly_without_wrapper() {
    let src =
        "[view]\nif vertical\n    col\n        text \"a\"\nelse\n    row\n        text \"b\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        !code.contains("Container::column(children![__"),
        "a single-if root must not wrap its branch in an injected column:\n{code}"
    );
    assert!(
        code.contains("Ok(Box::new(__col_0))") && code.contains("Ok(Box::new(__row_0))"),
        "each branch's element is returned directly as the component root:\n{code}"
    );
    assert!(
        code.contains("if vertical {") && code.contains("} else {"),
        "the conditional drives which branch becomes the root:\n{code}"
    );
}

#[test]
fn reactive_for_single_child_item_is_not_wrapped() {
    let src = "[logic]\nlet items = signal(vec![1i32, 2]);\n[view]\nrow align:stretch\n    for n in $items key *n\n        box fill:primary\n            text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        !code.contains("Ok(box_item(Container::new(LayoutStyle::new().flex_column()"),
        "a single-widget for item must not be wrapped in a collapsing flex-column cell:\n{code}"
    );
    assert!(
        code.contains("Ok(box_item(__sbox_0))"),
        "the item's styled box is returned bare so the row can stretch it:\n{code}"
    );
}

#[test]
fn transform_attrs_emit_with_transform() {
    let src = "[view]\ncol\n    box fill:primary rotate:30 scale:1.2\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(".with_transform("),
        "emits with_transform:\n{code}"
    );
    assert!(
        code.contains("box_transform(__r,"),
        "calls the helper:\n{code}"
    );
    assert!(
        code.contains("(30) as f32"),
        "rotate arg cast to f32:\n{code}"
    );
    assert!(
        code.matches("(1.2) as f32").count() >= 2,
        "scale fills both axes:\n{code}"
    );
}

#[test]
fn transform_promotes_plain_container() {
    let src = "[view]\ncol\n    col rotate:5\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("StyledContainer"),
        "promoted to styled:\n{code}"
    );
    assert!(
        code.contains(".with_transform("),
        "carries the transform:\n{code}"
    );
}

#[test]
fn no_transform_no_call() {
    let src = "[view]\ncol\n    box fill:primary\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        !code.contains(".with_transform("),
        "no transform, no call:\n{code}"
    );
}

#[test]
fn event_callbacks_emit_on_hover_and_on_key() {
    let src = "[logic]\nlet hot = signal(false);\nlet n = signal(0i32);\n[view]\ncol\n    box fill:primary on_hover:(|h| $hot.set(h)) on_key:(|_k| $n += 1)\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(code.contains(".on_hover("), "emits on_hover:\n{code}");
    assert!(code.contains(".on_key("), "emits on_key:\n{code}");
}

#[test]
fn on_pointer_move_emits_the_container_method() {
    let src = "[logic]\nlet at = signal((0.0f32, 0.0f32));\n[view]\ncol\n    box on_pointer_move:(|x, y| $at.set((x, y)))\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(".on_pointer_move("),
        "emits on_pointer_move:\n{code}"
    );
    assert!(
        code.contains("move |x, y| at.set((x, y))"),
        "with the $signal cloned in:\n{code}"
    );
}

/// `drag_threshold` is how far a press must travel before it is a drag rather than a click on what sits under it — the knob a viewport needs and a slider must not have.
#[test]
fn drag_threshold_emits_the_slop_distance() {
    let src = "[view]\ncol\n    box drag_threshold:4 on_drag:(|_x, _y| ())\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(".drag_threshold(4.0)"),
        "the threshold reaches the widget:\n{code}"
    );
}

#[test]
fn drag_button_emits_the_extra_buttons() {
    let src = "[view]\ncol\n    box drag_button:secondary,auxiliary on_drag:(|_x, _y| ())\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(
            ".drag_button(PointerButton::Secondary).drag_button(PointerButton::Auxiliary)"
        ),
        "emits one call per button:\n{code}"
    );
}

#[test]
fn on_hover_promotes_plain_container() {
    let src = "[view]\ncol\n    col on_hover:(|_h| ())\n        text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("StyledContainer"),
        "promoted to styled:\n{code}"
    );
    assert!(code.contains(".on_hover("), "carries the callback:\n{code}");
}

#[test]
fn heading_resolves_as_widget_component() {
    let src = "[view]\ncol\n    heading text:\"Title\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("heading(HeadingProps::props().text(\"Title\").build())?"),
        "heading is a component call carrying its text:\n{code}"
    );
}

#[test]
/// A `$signal` on a reactive prop hands over the *handle*, and the prop's own type turns it into a reading. The emitter used to build that closure itself, which is why it needed a table saying which props were reactive; `From<RwSignal<T>> for Reactive<T>` is where the `.get()` lives now.
fn btn_signal_color_reaches_the_prop_as_a_handle() {
    let src = "[logic]\nlet c = signal(Color::WHITE);\n[view]\nbutton label:\"x\" fill:$c\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(".fill(c.clone())"),
        "the signal reaches the prop as a handle, not as a sampled colour:\n{code}"
    );
    assert!(
        !code.contains("Box::new(move || c.get())"),
        "and the emitter no longer hand-builds the reading:\n{code}"
    );
}

#[test]
fn box_fill_computed_expression_is_reactive() {
    let src = "[logic]\nlet snap = signal(0i32);\nlet ids = signal(vec![1i32]);\n[view]\nrow\n    for id in $ids key id\n        box fill:(chip_fill($snap, id)) radius:6\n            text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("chip_fill(snap.get(), id)"),
        "the fill expr reads the signal reactively and keeps the loop var verbatim:\n{code}"
    );
    assert!(
        code.contains("let snap = snap.clone();"),
        "the captured signal is cloned into the paint closure:\n{code}"
    );
}

#[test]
fn text_color_computed_expression_is_reactive() {
    let src = "[logic]\nlet snap = signal(0i32);\n[view]\ntext \"hi\" color:(text_color($snap))\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("text_color(snap.get())"),
        "the color expr reads the signal reactively:\n{code}"
    );
    assert!(
        code.contains("let snap = snap.clone();"),
        "the captured signal is cloned into the style closure:\n{code}"
    );
}

/// D2's bug: a layout value that reads state through a *call* was frozen, because the reactivity test was `contains('$')` and a call carries no sigil. `styled_by` is what re-resolves a `LayoutStyle`, and the runtime tracks whatever the closure reads — so the closure is all the emitter has to produce.
#[test]
fn a_computed_layout_value_is_re_resolved_even_without_a_sigil() {
    let src = "[logic]\nlet dock = signal(0.0f32);\nfn measured() -> f32 { 12.0 }\n[view]\ncol gap:measured() pad:8\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains(
            ".styled_by(move || LayoutStyle::new().flex_column().gap(measured()).padding_all(8.0))"
        ),
        "a call is a reading:\n{}",
        out.rust_code
    );

    let flat = crate::transpile_source(
        "[view]\ncol gap:8 pad:12 align:center\n    text \"x\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(!flat.contains(".styled_by("), "{flat}");
}

#[test]
fn a_fraction_of_the_surface_needs_no_effect_of_its_own() {
    let out = crate::transpile_source(
        "[view]\ncol width:50sw height:100sh pad:2smin\n    text \"x\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(
        out.contains(
            ".width(SizeDimension::SurfaceWidth(0.5)).height(SizeDimension::SurfaceHeight(1.0)).padding_all(SizeDimension::SurfaceMin(0.02))"
        ),
        "{out}"
    );
    assert!(
        !out.contains(".styled_by("),
        "the layout engine re-resolves it on a resize, so nothing in the view has to:\n{out}"
    );
}

/// A re-resolving style names whatever the author had in scope, and takes it the way every closure does: moved when nothing else reads it, copied in when something does — a `props` field by itself, so `Props` need not be `Clone`.
#[test]
fn a_re_resolving_style_copies_only_what_another_reader_needs() {
    let src = "[logic]\nlet side = 4.0f32;\n[view]\ncol gap:side pad:props.pad\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains(
            ".styled_by(move || LayoutStyle::new().flex_column().gap(side).padding_all(props.pad))"
        ),
        "{}",
        out.rust_code
    );

    let shared = "[logic]\nlet side = 4.0f32;\n[view]\ncol gap:side pad:props.pad\n    text \"{side} {props.pad}\"\n";
    let out = crate::transpile_source(shared, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains(
            ".styled_by({ let side = side.clone(); let __props_pad = props.pad.clone(); move || LayoutStyle::new().flex_column().gap(side).padding_all(__props_pad) })"
        ),
        "{}",
        out.rust_code
    );
}

/// The parens a value needs to hold a space are the markup's delimiters, not part of the expression: emitted, they warn `unused_parens` in code the author cannot edit.
#[test]
fn a_values_own_parens_do_not_reach_the_output() {
    let src =
        "[view]\ncol gap:(space::lg() * 2.0) pad:(::ui::scale::md())\n    box fill:(tint(on))\n";
    let out = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(out.contains(".gap(space::lg() * 2.0)"), "{out}");
    assert!(out.contains(".padding_all(::ui::scale::md())"), "{out}");
    assert!(out.contains("with_fill(tint(on))"), "{out}");
}

/// A theme read is a `$` read: the view binds `theme` as a handle, so the same sugar that reads a signal reads the theme, and it re-reads inside the paint closure instead of freezing at construction.
#[test]
fn a_theme_read_is_a_dollar_read_like_any_other() {
    let src = "[view]\nbox fill:$theme.primary\n    text \"x\"\n";
    let code = crate::transpile_source(src, "demo", Some("MyTheme"), None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("let theme = telar::Theme::<MyTheme>::default();"),
        "the view binds the handle:\n{code}"
    );
    assert!(
        code.contains("theme.get().primary"),
        "and the read happens where it is written:\n{code}"
    );
}

// A tag with no dispatch arm falls through to `emit_component_call` and is emitted as a bare component call, so this guards the registry table and the dispatch `match` against drift.
#[test]
fn every_builtin_tag_has_a_dispatch_arm() {
    for &(tag, _ctor) in crate::registry::builtin_tags() {
        let src = format!("[view]\n{tag}\n");
        // A real emit arm may still reject a bare instance; that is not a fall-through, since `emit_component_call` returns unconditionally.
        let Ok(out) = crate::transpile_source(&src, "demo", None, None) else {
            continue;
        };
        assert!(
            !out.rust_code.contains(&format!("{tag}()?")),
            "builtin tag `{tag}` fell through to emit_component_call (no dispatch arm):\n{}",
            out.rust_code
        );
    }
}

/// The language used to be strict about keys and silent about values: a misspelled `align` laid out unaligned, with the attribute simply skipped and nothing to point at.
#[test]
fn a_misspelled_keyword_stops_the_build_the_way_a_misspelled_key_does() {
    let src = "[view]\ncol align:centre\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains("is not a value of `align`"),
        "{}",
        out.rust_code
    );
}

/// `img fit:covr` used to be *replaced by a default* rather than skipped, so the picture came out fitted the other way with nothing anywhere saying so.
#[test]
fn a_media_keyword_is_rejected_instead_of_defaulted() {
    let src = "[view]\nimg src:\"a.png\" fit:covr\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains("is not a value of `fit`"),
        "{}",
        out.rust_code
    );
}

/// A `text`'s `align` is a different property from a container's `align` of the same name, so it takes a different closed set: `justify` is a paragraph, and a typo under either must not pass as the other.
#[test]
fn align_is_checked_against_the_set_its_own_tag_has() {
    let paragraph = "[view]\ntext \"x\" text_align:justify\n";
    let out = crate::transpile_source(paragraph, "demo", None, None).unwrap();
    assert!(
        !out.rust_code.contains("compile_error!"),
        "{}",
        out.rust_code
    );
    assert!(
        out.rust_code.contains("TextAlign::Justify"),
        "{}",
        out.rust_code
    );

    let container = "[view]\ncol align:justify\n    text \"x\"\n";
    let out = crate::transpile_source(container, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains("is not a value of `align`"),
        "the error must name the attribute: {}",
        out.rust_code
    );
}

/// Six keys that meant nothing apart become one value, and the three that sat among the colour keys stop reading as properties a box has.
#[test]
fn a_gradient_is_a_value_of_fill_and_not_six_keys_of_its_own() {
    let code = crate::transpile_source(
        "[view]\nbox fill:linear(horizontal, #ff0000, #0000ff)\n    text \"x\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains("Paint::Gradient(Gradient::linear(Point::new(r.x, r.y + r.height * 0.5)"),
        "the axis leads:\n{code}"
    );

    let gone = crate::transpile_source(
        "[view]\nbox gradient:horizontal from:#ff0000 to:#0000ff\n    text \"x\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(
        gone.contains("`gradient` is not an attribute of `box`"),
        "{gone}"
    );
    assert!(
        gone.contains("`from` is not an attribute of `box`"),
        "{gone}"
    );
}

/// A gradient's *shape* is the one thing about it this crate can judge — its stops are Rust expressions, reported by rustc against the attribute's own line.
#[test]
fn a_gradient_that_is_not_one_is_reported_on_the_fill_that_holds_it() {
    let code = crate::transpile_source(
        "[view]\nbox fill:linear(#ff0000)\n    text \"x\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(code.contains("is not a gradient"), "{code}");
}

/// A synonym is cost with nothing bought, and every one of these had a shorter or plainer spelling that stays. They are errors rather than silences, which is what makes deleting them safe.
#[test]
fn a_second_spelling_of_a_value_is_not_a_value() {
    for (src, expected) in [
        ("[view]\ntext \"x\" font_weight:demibold\n", "`semibold`"),
        ("[view]\ntext \"x\" font_weight:black\n", "`heavy`"),
        (
            "[view]\ncol wrap:wrap\n    text \"x\"\n",
            "`wrap` takes no value",
        ),
        (
            "[view]\ntext \"x\" lines:2 ellipsis:true\n",
            "`ellipsis` takes no value",
        ),
        (
            "[view]\ngrid cols(240px 1fr)\n    text \"x\"\n",
            "is not a track list",
        ),
    ] {
        let code = crate::transpile_source(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(expected),
            "expected {expected:?} for {src:?}:\n{code}"
        );
    }
}

/// `$props.label` reads the field, and a read in two places copies the field into each the way a plain `props.label` read does.
#[test]
fn a_prop_read_with_a_dollar_reaches_through_props_to_the_field() {
    let src = "[logic]\npub struct Props {\n    #[props(into, default)]\n    pub label: Reactive<String>,\n}\n\n[view]\ncol\n    text \"{$props.label}\"\n    text \"again {$props.label}\" color:#3d78fa\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert_eq!(
        code.matches("let __props_label = props.label.clone();")
            .count(),
        2,
        "{code}"
    );
    assert_eq!(code.matches("__props_label.get()").count(), 2, "{code}");
    assert!(!code.contains("__props_label.clone()"), "{code}");
    assert!(!code.contains("props.get()"), "{code}");
    if let Err(e) = syn::parse_file(&code) {
        panic!("generated code does not parse: {e}\n{code}");
    }
}

/// Only `props` reaches through to a field: anywhere else `$` binds to the identifier, and the field is taken from what it reads.
#[test]
fn a_dollar_on_any_other_binding_still_reads_the_binding() {
    let src = "[logic]\nlet user = signal(User::default());\n[view]\ntext \"{$user.name}\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(code.contains("user.get().name"), "{code}");
    assert!(!code.contains("__props_"), "{code}");
}

/// A weight is a number, so it takes what a number takes: a read, a constant, an expression. Only a bare word is held to the spellings.
#[test]
fn a_weight_may_be_any_expression_that_yields_one() {
    for (src, expected) in [
        (
            "[logic]\nlet w = signal(700u16);\n[view]\ntext \"x\" font_weight:$w\n",
            ".with_font_weight(w.get())",
        ),
        (
            "[view]\ntext \"x\" font_weight:TITLE_WEIGHT\n",
            ".with_font_weight(TITLE_WEIGHT)",
        ),
        (
            "[view]\ntext \"x\" font_weight:(base + 100)\n",
            ".with_font_weight(base + 100)",
        ),
        (
            "[view]\ncol font_weight:$theme.font_weight_strong\n    text \"x\"\n",
            "font_weight_strong",
        ),
    ] {
        let code = crate::transpile_source(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(expected) && !code.contains("compile_error!"),
            "expected {expected:?} for {src:?}:\n{code}"
        );
    }
}

/// `white` and `black` stop being colours, so the name falls through to what a bare ident means — a `[style]` constant — instead of quietly painting one.
#[test]
fn a_named_colour_is_only_transparent_now() {
    let code = crate::transpile_source("[view]\ntext \"x\" color:white\n", "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("Color::WHITE"), "{code}");

    let kept =
        crate::transpile_source("[view]\ntext \"x\" color:transparent\n", "demo", None, None)
            .unwrap()
            .rust_code;
    assert!(kept.contains("Color::TRANSPARENT"), "{kept}");
}

/// The spellings that stay, so the trim is a trim and not an amputation.
#[test]
fn the_one_spelling_of_each_still_works() {
    for (src, expected) in [
        (
            "[view]\ntext \"x\" font_weight:semibold\n",
            ".with_font_weight(600)",
        ),
        (
            "[view]\ntext \"x\" font_weight:heavy\n",
            ".with_font_weight(900)",
        ),
        (
            "[view]\ntext \"x\" font_weight:600\n",
            ".with_font_weight(600)",
        ),
        ("[view]\ncol wrap\n    text \"x\"\n", ".flex_wrap()"),
        (
            "[view]\ntext \"x\" lines:2 ellipsis\n",
            ".with_clamp(2, true)",
        ),
        (
            "[view]\ngrid cols(240 1fr)\n    text \"x\"\n",
            "TemplateTrack::px(240.0)",
        ),
    ] {
        let code = crate::transpile_source(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(!code.contains("compile_error!"), "{src:?}:\n{code}");
        assert!(code.contains(expected), "{src:?}:\n{code}");
    }
}

/// `@heading { font_size: 22 }` used to compile and do nothing: a class reached a container's paint and a container's layout, and a `text` never looked at its classes at all.
#[test]
fn a_class_carries_text_properties_to_a_text() {
    let src =
        "[style]\n@heading\n    font_size: 22\n    color: #ff0000\n\n[view]\ntext \"x\" @heading\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".with_font_size(22.0)") && code.contains(".with_color("),
        "the class's size and colour reach the leaf:\n{code}"
    );
}

/// An inline attribute is nearer than the class it sits beside, and wins.
#[test]
fn an_inline_attribute_beats_the_class_beside_it() {
    let src = "[style]\n@heading\n    font_size: 22\n\n[view]\ntext \"x\" @heading font_size:11\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(code.contains(".with_font_size(11.0)"), "{code}");
    assert!(!code.contains(".with_font_size(22.0)"), "{code}");
}

/// The same class on a container declares for everything below it instead — one spelling, two reaches.
#[test]
fn a_class_on_a_container_declares_for_the_subtree() {
    let src = "[style]\n@heading\n    font_size: 22\n\n[view]\ncol @heading\n    text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(".declaring(move || Declared::default().with_font_size(22.0))"),
        "{code}"
    );
}

/// The line this whole document exists for: a container that draws no text says what the text under it looks like, and `font_size:` on a `col` used to be a `compile_error!`.
#[test]
fn a_container_declares_the_text_below_it() {
    let src = "[view]\ncol font_size:11 color:#ff0000 raster:pixel\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".declaring(move || Declared::default().with_font_size(11.0).with_color("),
        "size and colour reach the declaration:\n{code}"
    );
    assert!(
        code.contains(".with_raster(Raster::Pixel))"),
        "and so does the raster:\n{code}"
    );
}

/// A clamp is one paragraph's business, so it stays where it was and never reaches a container.
#[test]
fn a_container_cannot_clamp_the_text_below_it() {
    let src = "[view]\ncol lines:2\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code
            .contains("`lines` is not an attribute of `col`"),
        "{}",
        out.rust_code
    );
}

/// A container naming nothing inheritable declares nothing, so the map the cascade walks stays empty for every tree that does not use the feature.
#[test]
fn a_container_that_says_nothing_declares_nothing() {
    let src = "[view]\ncol gap:8\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(!out.rust_code.contains(".declaring("), "{}", out.rust_code);
}

/// The names the transpiler cannot enumerate — a `[logic]` binding, a `const`, a props field — still reach rustc, which names them against this `.rsx` line through the source map.
#[test]
fn a_name_the_author_has_in_scope_is_still_carried_through() {
    let src = "[logic]\nlet gutter = 8.0;\n[view]\ncol gap:gutter pad:props.pad\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        !out.rust_code.contains("compile_error!"),
        "{}",
        out.rust_code
    );
    assert!(
        out.rust_code.contains(".gap(gutter)"),
        "a name in scope is passed through untouched: {}",
        out.rust_code
    );
    assert!(
        out.rust_code.contains(".padding_all(props.pad)"),
        "and so is a field access: {}",
        out.rust_code
    );
}

/// B1: the family was a process-wide global, so a `text` could not name one at all — the whole reason "one font here and another in the window around it" was inexpressible.
#[test]
fn a_text_can_name_the_face_it_shapes_in() {
    let src = "[view]\ntext \"x\" font_family:\"LanaPixel\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains(r#".with_font_family("LanaPixel")"#),
        "{}",
        out.rust_code
    );
}

/// The families an application has are its own vocabulary, so an unquoted value is carried through the way a `theme.` read or a `[logic]` binding is everywhere else.
#[test]
fn an_unquoted_family_is_the_authors_own_expression() {
    let src = "[logic]\nlet face = \"LanaPixel\";\n[view]\ntext \"x\" font_family:face\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains(".with_font_family(face)"),
        "{}",
        out.rust_code
    );
}

/// A `text` naming no family must emit no family call at all, so it keeps shaping in the configured sans-serif face exactly as it did before the axis existed.
#[test]
fn a_text_naming_no_family_is_untouched() {
    let src = "[view]\ntext \"x\" font_size:12\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        !out.rust_code.contains("with_font_family"),
        "a text naming no family must not ask for one: {}",
        out.rust_code
    );
}

/// T-5.1: a bare generic names its `FontFamily` variant, the fix for the family that used to shape in `SansSerif` no matter which generic was asked for.
#[test]
fn a_bare_generic_names_its_variant() {
    let src = "[view]\ntext \"x\" font_family:monospace\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code
            .contains(".with_font_family(FontFamily::Monospace)"),
        "{}",
        out.rust_code
    );
}

/// Quoting a generic (`font_family:"monospace"`) is the bug this task starts from: the family named a CSS keyword and still came out `Named`, which the DOM then quoted, so the generic never applied. The DSL's names are Telar's own vocabulary regardless of quoting, so both spellings resolve to the same variant.
#[test]
fn a_quoted_generic_still_names_its_variant() {
    let src = "[view]\ntext \"x\" font_family:\"monospace\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code
            .contains(".with_font_family(FontFamily::Monospace)"),
        "{}",
        out.rust_code
    );
}

/// A quoted comma list is an ordered fallback: the face first, then the generic it degrades to, exactly the order CSS `font-family` takes.
#[test]
fn a_quoted_comma_list_becomes_an_ordered_stack() {
    let src = "[view]\ntext \"x\" font_family:\"Iosevka, monospace\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains(
            r#".with_font_family(FontFamily::stack(["Iosevka".into(), FontFamily::Monospace]))"#
        ),
        "{}",
        out.rust_code
    );
}

/// A `Canvas` is a `ui-core` primitive that had no tag, so the only way to place one was to build it in `[logic]` and splice the binding through `widget` — which is also why a canvas could never sit inside anything that rebuilds. Named as a tag, it is constructed where it is placed.
#[test]
fn canvas_tag_builds_the_primitive_where_it_is_placed() {
    let src = "[logic]\nlet paint = |rect: Rect| RenderNode::group([]);\n\n[view]\ncol\n    canvas paint:paint width:196 height:56\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("Canvas::new(LayoutStyle::new().width(196.0).height(56.0), paint)?"),
        "the layout attrs shape the leaf and the closure paints it:\n{code}"
    );
}

/// Without one there is nothing to draw, and the leaf would be a silent empty rect.
#[test]
fn a_canvas_without_paint_says_so() {
    let src = "[view]\ncol\n    canvas width:10 height:10\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(code.contains("compile_error!"), "{code}");
}

/// Axes and features are a four-letter tag and a value each, the value free to read state, and an axis can be animated like a colour.
#[test]
fn font_settings_are_tags_and_values_that_may_read_state() {
    let src = "[logic]\nlet w = signal(400.0f32);\n[view]\ncol font_features:(tnum, liga 0)\n    text \"Hola\" font_variation:(wght $w, wdth 110) transition(font_variation 300ms)\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".with_font_features(FontFeatures::new().with(\"tnum\", (1) as u32).with(\"liga\", (0) as u32))"),
        "a container declares features its text inherits:\n{code}"
    );
    assert!(
        code.contains(
            "FontVariations::new().with(\"wght\", (w.get()) as f32).with(\"wdth\", (110) as f32)"
        ),
        "axes read state:\n{code}"
    );
    assert!(
        code.contains("motion::Animated::new(FontVariations::new()"),
        "the axes animate:\n{code}"
    );
}

#[test]
fn a_font_setting_that_is_not_a_tag_is_a_build_error() {
    let src = "[view]\ntext \"Hola\" font_variation:(weight 700)\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(
            "compile_error!(\"`font_variation:` names `weight`, which is not a four-letter tag\")"
        ),
        "{code}"
    );
}

/// A paragraph written as runs: the text's own content first, then each span with what it restyles and where it links.
#[test]
fn a_text_with_spans_is_one_paragraph_of_runs() {
    let src = "[logic]\nlet name = signal(String::from(\"Ada\"));\n[view]\ntext \"Hi \" font_size:14\n    span \"{$name}\" font_weight:700\n    span \", meet \"\n    span \"Telar\" to:external(\"https://example.com\") color:#3d78fa\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(code.contains("Text::runs("), "{code}");
    assert_eq!(code.matches("TextRun::new(").count(), 4, "{code}");
    assert!(
        code.contains(".declaring(move || Declared::default().with_font_weight(700))"),
        "{code}"
    );
    assert!(
        code.contains(".to(move || external(\"https://example.com\"))"),
        "{code}"
    );
}

#[test]
fn a_span_outside_a_text_and_a_box_inside_one_are_build_errors() {
    let stray = crate::transpile_source("[view]\ncol\n    span \"x\"\n", "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        stray.contains("a `span` is a run of a paragraph"),
        "{stray}"
    );
    let boxed = crate::transpile_source("[view]\ntext\n    box\n", "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        boxed.contains("a `text` holds `span`s and nothing else"),
        "{boxed}"
    );
    let bad = crate::transpile_source(
        "[view]\ntext\n    span \"x\" to:external(\"nowhere\")\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(bad.contains("not an absolute URI"), "{bad}");
}

/// A size or a tracking with a unit resolves where it stands: a `text` against the size it inherits and the surface it reads, a container's declaration in the tree below it.
#[test]
fn text_lengths_take_the_units_a_box_does() {
    let src = "[view]\ncol font_size:1.5em letter_spacing:-0.04em\n    text \"ADRIEL\" font_size:22sw letter_spacing:-0.02em\n    text \"x\" font_size:18\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".with_font_size_in(TextLength::SurfaceWidth(0.22), use_surface_size())"),
        "a text reads the surface for a fraction of it:\n{code}"
    );
    assert!(
        code.contains(".with_letter_spacing_in(TextLength::Em(-0.02), Size::ZERO)"),
        "and not for em:\n{code}"
    );
    assert!(
        code.contains("Declared::default().with_font_size(TextLength::Em(1.5)).with_letter_spacing(TextLength::Em(-0.04))"),
        "a container declares the unit for the tree to resolve:\n{code}"
    );
    assert!(
        code.contains(".with_font_size(18.0)"),
        "pixels stay pixels:\n{code}"
    );
}

/// A size fitted to a width is the text's own, built into it rather than into its style, which is handed each size layout tries.
#[test]
fn a_fitted_size_builds_a_fitting_text() {
    let src = "[logic]\nlet share = signal(FitWidth::Containing(0.6));\n[view]\ncol\n    text \"ADRIEL\" font_size:fit(60%, max_height:56sh) letter_spacing:0.04em\n    text \"B\" font_size:fit(480)\n    text \"C\" font_size:fit(40sw, max_height:3em)\n    text \"D\" font_size:fit($share)\n    text font_size:fit(100%)\n        span \"E\"\n        span \"F\" font_weight:bold\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains("Text::fitting(")
            && code.contains(
                "move || FontFit::containing(0.6).with_max_height(TextLength::SurfaceHeight(0.56))"
            ),
        "{code}"
    );
    assert!(
        code.contains("move || FontFit::new(TextLength::Px(480.0))"),
        "{code}"
    );
    assert!(
        code.contains(
            "move || FontFit::new(TextLength::SurfaceWidth(0.4)).with_max_height(TextLength::Em(3.0))"
        ),
        "{code}"
    );
    assert!(code.contains("FontFit::new(share.get())"), "{code}");
    assert!(
        code.contains("Text::runs_fitting(") && code.contains("move || FontFit::containing(1.0)"),
        "{code}"
    );
    assert!(
        !code.contains("with_font_size"),
        "the fit owns the size, so the style never sets one:\n{code}"
    );
    assert!(
        code.contains(".with_letter_spacing_in(TextLength::Em(0.04), Size::ZERO)"),
        "tracking in em resolves against whichever size layout tries:\n{code}"
    );
}

#[test]
fn a_fit_is_refused_off_a_text_and_when_it_names_nothing() {
    for (src, says) in [
        (
            "[view]\ncol font_size:fit(60%)\n    text \"x\"\n",
            "goes on a `text`",
        ),
        ("[view]\ntext \"x\" font_size:fit()\n", "names no width"),
        (
            "[view]\ntext \"x\" font_size:fit(60%, max:3)\n",
            "takes a width and then `max_height:`",
        ),
        (
            "[view]\ntext \"x\" font_size:fit(sixty%)\n",
            "is not a percentage",
        ),
        (
            "[logic]\nlet v = signal(String::new());\n[view]\ninput value:$v font_size:fit(50%)\n",
            "an `input` takes a size",
        ),
    ] {
        let code = crate::transpile_source(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(code.contains(says), "{src}\n{code}");
    }
}

/// A mask is its box, then the source its content is seen through, then the content.
#[test]
fn a_mask_is_a_source_and_a_content_in_one_box() {
    let src = "[view]\nmask width:100% height:200 label:\"SIMULACION\"\n    text \"SIMULACION\" font_size:18sw\n    canvas paint:(|_rect| RenderNode::Empty)\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(code.contains("Mask::new(LayoutStyle::new()"), "{code}");
    assert!(
        code.contains("box_item(__text_0), box_item(__canvas_0))?"),
        "{code}"
    );
    assert!(
        code.contains(".a11y_label(|| \"SIMULACION\")"),
        "a reader is told what it says:\n{code}"
    );
    let one = crate::transpile_source("[view]\nmask\n    text \"x\"\n", "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(one.contains("a `mask` holds two children"), "{one}");
}

/// A theme on a box is the theme of the box and of everything under it, however many children it has, and one read from state follows it.
#[test]
fn a_theme_is_provided_to_a_subtree() {
    let src = "[logic]\nlet dark = signal(false);\n[view]\ncol\n    col theme:Palette::for_act($dark)\n        text \"a\"\n        text \"b\"\n    box theme:(Palette::default())\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains("provide_theme(follow_theme({ let dark = dark.clone(); move || Palette::for_act(dark.get()) }), || {"),
        "a theme read from state is followed:\n{code}"
    );
    assert!(
        code.contains("provide_theme(Palette::default(), || {"),
        "{code}"
    );
    assert!(
        code.contains("Ok(Box::new(__col_1) as Box<dyn LayoutItem>)"),
        "{code}"
    );
}

fn transpiled(attrs: &str) -> String {
    let src = format!(
        "[logic]\nlet x = signal(0.25f32);\n[view]\ncol\n    box fill:primary {attrs}\n        text \"x\"\n"
    );
    crate::transpile_source(&src, "demo", None, None)
        .unwrap()
        .rust_code
}

#[test]
fn a_lone_origin_keyword_pivots_on_the_inline_axis() {
    let code = transpiled("scale:1.06 transform_origin:start");
    assert!(
        code.contains("box_transform_about(__r, TransformOrigin::new(0.0, 0.5),"),
        "{code}"
    );
    let code = transpiled("scale:1.06 transform_origin:end");
    assert!(code.contains("TransformOrigin::new(1.0, 0.5)"), "{code}");
}

#[test]
fn an_origin_pair_takes_keywords_and_reactive_fractions() {
    let code = transpiled("rotate:10 transform_origin:(end $x)");
    assert!(
        code.contains("TransformOrigin::new(1.0, (x.get()) as f32)"),
        "{code}"
    );
    assert!(
        code.contains("let x = x.clone();"),
        "the signal is captured:\n{code}"
    );
}

#[test]
fn without_an_origin_the_pivot_stays_the_centre() {
    let code = transpiled("scale:2");
    assert!(code.contains("box_transform(__r,"), "{code}");
    assert!(!code.contains("box_transform_about"), "{code}");
}

#[test]
fn a_malformed_origin_is_a_build_error() {
    let code = transpiled("scale:2 transform_origin:(start)");
    assert!(code.contains("compile_error!"), "{code}");
    let code = transpiled("scale:2 transform_origin:left");
    assert!(code.contains("compile_error!"), "{code}");
}

/// A case, an underline and its longhands are text properties like any other: a container declares them for what lies beneath, a text amends what it inherits, and the underline may follow state.
#[test]
fn a_case_and_an_underline_cascade_and_follow_state() {
    let src = "[logic]\nlet hovered = signal(false);\n[view]\ncol text_case:upper underline_offset:0.18em underline_thickness:2 underline_color:#ff0000\n    text \"Menu\" underline:$hovered\n    text \"Docs\" underline text_case:none underline_offset:4\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(
            ".declaring(move || Declared::default().with_text_case(TextCase::Upper).with_underline_offset(TextLength::Em(0.18)).with_underline_thickness(2.0).with_underline_color("
        ),
        "a container declares them for its subtree:\n{code}"
    );
    assert!(
        code.contains(".with_underline(hovered.get())"),
        "an underline reads state:\n{code}"
    );
    assert!(
        code.contains(
            ".with_text_case(TextCase::AsWritten).with_underline(true).with_underline_offset(4.0)"
        ),
        "a bare flag draws it and a text takes the case back:\n{code}"
    );
}

/// A backdrop blur is a number the box re-reads, from state or animated, and it makes a plain `col` a styled box.
#[test]
fn a_backdrop_blur_is_read_and_animated_like_opacity() {
    let src = "[logic]\nlet frost = signal(20.0f32);\n[view]\ncol backdrop_blur:20 fill:#ffffff80\n    box backdrop_blur:$frost transition(backdrop_blur 200ms)\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(code.contains(".with_backdrop_blur(|| 20.0)"), "{code}");
    assert!(
        code.contains("motion::Animated::new(frost.get(),"),
        "a transition animates it:\n{code}"
    );
    let promoted = crate::transpile_source(
        "[view]\ncol backdrop_blur:12\n    text \"Hi\"\n",
        "demo",
        None,
        None,
    )
    .unwrap()
    .rust_code;
    assert!(promoted.contains("StyledContainer::"), "{promoted}");
}

/// A preview finds its root component call by the same rule the emitter dispatches on, so a tag the view builds itself is never taken for one.
#[test]
fn every_tag_the_view_builds_is_told_from_a_component_call() {
    for tag in BUILT_TAGS.iter().chain(&["svg", "img", "image"]) {
        assert!(!is_component_tag(tag), "`{tag}` is built by the view");
        let element = Element {
            tag: tag.to_string(),
            classes: Vec::new(),
            attributes: Vec::new(),
            content: None,
            children: Vec::new(),
            line: 1,
            content_start: 0,
            content_i18n: false,
        };
        let code = match make_gen().emit_element_inner(&element) {
            ChildEmit::Simple { code, .. }
            | ChildEmit::Dynamic { code }
            | ChildEmit::Fragment { code, .. } => code,
        };
        assert!(
            !code.contains("::props()"),
            "`{tag}` emitted a call:\n{code}"
        );
    }
    assert!(is_component_tag("button"));
    assert!(is_component_tag("forms::checkbox"));
}

fn transpile_view(source: &str) -> String {
    crate::transpile_source(source, "demo", None, None)
        .expect("transpiles")
        .rust_code
}

/// The block a `text` reading `needle` is built in.
fn text_block<'c>(code: &'c str, needle: &str) -> &'c str {
    let at = code
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}`:\n{code}"));
    let start = code[..at].rfind("let __text_").expect("a text block");
    &code[start..at + needle.len()]
}

/// A binding another snippet still reads is copied into the text's closure, so the text no longer consumes what the next reader needs; one read alone is moved, as it always was.
#[test]
fn text_copies_in_a_binding_another_reader_still_needs() {
    let code = transpile_view(
        "[logic]\nlet project = std::rc::Rc::new(|| String::from(\"telar\"));\nlet once = String::from(\"x\");\n[view]\nrow label:(project())\n    text \"{project()}\"\n    text \"{once}\"\n",
    );
    assert!(
        text_block(&code, "format!(\"{}\", project())").contains("let project = project.clone();"),
        "{code}"
    );
    assert!(
        !text_block(&code, "format!(\"{}\", once)").contains("let once = once.clone();"),
        "{code}"
    );
}

/// `props` is copied by the place a hole reads, which is what a closure captures: a field `[logic]` moved out stays moved, and the rest is not cloned whole.
#[test]
fn a_props_place_two_readers_share_is_copied_by_itself() {
    let code = transpile_view(
        "[logic]\npub struct Props {\n    pub detail: String,\n    pub state: String,\n}\nlet state = props.state;\n[view]\ncol\n    text \"{props.detail}\"\n    text \"{props.detail.len()}\"\n",
    );
    assert!(
        code.contains("let __props_detail = props.detail.clone();"),
        "{code}"
    );
    assert!(
        code.contains("move || format!(\"{}\", __props_detail)"),
        "{code}"
    );
    assert!(!code.contains("let props = props.clone();"), "{code}");
}

/// What an `if let` binds is often a borrow of the value it matched, and a closure needs an owned copy of it.
#[test]
fn an_if_let_binding_is_copied_into_a_text() {
    let code = transpile_view(
        "[logic]\nlet location: Option<String> = None;\n[view]\ncol\n    if let Some(place) = &location\n        text \"{place}\"\n",
    );
    assert!(
        text_block(&code, "format!(\"{}\", place)").contains("let place = place.clone();"),
        "{code}"
    );
}

/// A quoted prop reads as text does: a literal while it has no hole, formatted once while its holes read nothing that moves, and followed while one does.
#[test]
fn a_quoted_prop_interpolates() {
    let code = transpile_view(
        "[logic]\nlet name = String::from(\"Ada\");\nlet count = signal(1);\n[view]\ncol\n    button label:\"Plain {{braces}}\"\n    button label:\"Hi {name}\"\n    button label:\"Count {$count}\"\n",
    );
    assert!(code.contains(".label(\"Plain {braces}\")"), "{code}");
    assert!(code.contains(".label(format!(\"Hi {}\", "), "{code}");
    assert!(
        code.contains(
            ".label(Reactive::of({ let count = count.clone(); move || format!(\"Count {}\""
        ),
        "{code}"
    );
}

/// A constant cannot change, so a layout value made of constants needs nothing to re-read it; a call can answer differently, and is followed.
#[test]
fn a_constant_layout_value_is_static() {
    let code = transpile_view(
        "[logic]\nconst GRID: f32 = 4.0;\nfn gutter() -> f32 { GRID }\n[view]\ncol gap:GRID pad:(GRID * 2.0)\n    col gap:(gutter())\n        text \"x\"\n",
    );
    assert_eq!(code.matches(".styled_by(").count(), 1, "{code}");
    assert!(
        code.contains(".styled_by(move || LayoutStyle::new().flex_column().gap(gutter()))"),
        "{code}"
    );
}

/// A prop read from a hook or a signal follows it as a `$` read does; a hook's handle passed on, and a plain value, are left as written.
#[test]
fn a_prop_reading_reactive_context_follows_it() {
    let code = transpile_view(
        "[logic]\nlet open = signal(false);\n[view]\ncol\n    button label:\"a\" color:(use_tokens().primary)\n    button label:\"b\" scroll:(use_primary_scroll())\n    button label:\"c\" disabled:(open.get())\n    button label:\"d\" width:(WIDTH * 2.0)\n",
    );
    for expected in [
        ".color(Reactive::of(move || use_tokens().primary))",
        ".scroll(use_primary_scroll())",
        ".disabled(Reactive::of(move || open.get()))",
        ".width(WIDTH * 2.0)",
    ] {
        assert!(code.contains(expected), "`{expected}`:\n{code}");
    }
}

/// A hole of a quoted value maps back to the text it was written as, wherever the value is: a cursor on it reaches the Rust it became.
#[test]
fn a_quoted_values_holes_keep_their_source_spans() {
    let src = "[logic]\nlet name = String::from(\"Ada\");\n[view]\ncol\n    button label:\"Hi {name.len()}\" a11y_label:\"Open {name.len()}\"\n    box label:\"Box {name.len()}\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let mapped: Vec<&str> = out
        .expr_spans
        .iter()
        .map(|span| &src[span.rsx_start as usize..(span.rsx_start + span.len) as usize])
        .filter(|text| text.contains("name"))
        .collect();
    assert_eq!(
        mapped,
        ["name.len()", "name.len()", "name.len()"],
        "{}",
        out.rust_code
    );
}

/// A `text` says it is a heading by level name or, when the level is worked out, by an expression yielding the role; either only describes it.
#[test]
fn a_text_describes_itself_as_a_heading_by_name_or_by_expression() {
    for (src, expected) in [
        (
            "[view]\ntext \"x\" role:h2\n",
            ".a11y_role(::telar::Role::Heading(2))",
        ),
        (
            "[logic]\nlet level = 3u8;\n[view]\ntext \"x\" role:(Role::Heading(level))\n",
            ".a11y_role(Role::Heading(level))",
        ),
    ] {
        let code = crate::transpile_source(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(expected) && !code.contains("compile_error!"),
            "expected {expected:?} for {src:?}:\n{code}"
        );
    }
}

/// A paragraph can only describe itself, so a role it would have to be operated or hold boxes for is refused by name.
#[test]
fn a_text_refuses_a_role_that_is_not_a_heading() {
    let code = crate::transpile_source("[view]\ntext \"x\" role:button\n", "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(code.contains("is not a value of `role`"), "{code}");
}

/// A box's role worked out from state only describes the box: a control's role is written by name, where the build can check what goes with it.
#[test]
fn a_box_role_given_as_an_expression_describes_the_box() {
    let src =
        "[logic]\nlet level = 2u8;\n[view]\ncol role:(Role::Heading(level))\n    text \"x\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(code.contains(".role(Role::Heading(level))"), "{code}");
    assert!(!code.contains(".control("), "{code}");
}

/// A length's number may be worked out: written before the unit in parentheses, it means what the literal there would.
#[test]
fn a_text_length_takes_a_worked_out_number_before_its_unit() {
    for (src, expected) in [
        (
            "[logic]\nlet scale = 1.5f32;\n[view]\ntext \"x\" font_size:(scale)em\n",
            ".with_font_size_in(TextLength::Em(scale), Size::ZERO)",
        ),
        (
            "[logic]\nlet share = signal(20.0f32);\n[view]\ntext \"x\" font_size:($share)sw\n",
            "TextLength::SurfaceWidth((share.get()) / 100.0)",
        ),
    ] {
        let code = crate::transpile_source(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(expected) && !code.contains("compile_error!"),
            "expected {expected:?} for {src:?}:\n{code}"
        );
    }
}

/// A class on a component call says what its text looks like over what the component says itself, while an inline attribute there stays a prop.
#[test]
fn a_class_on_a_component_authors_its_text() {
    let src = "[style]\n@quiet\n    font_weight: normal\n    font_size: 1.25em\n    radius: 0\n[view]\nheading @quiet text:\"Account\" level:3\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(".authored_text(move || Declared::default()"),
        "{code}"
    );
    assert!(code.contains(".with_font_weight(400)"), "{code}");
    assert!(code.contains("TextLength::Em(1.25)"), "{code}");
    assert!(code.contains(".level(::telar::Int(3))"), "{code}");
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn a_class_with_no_text_property_authors_no_text() {
    let src = "[style]\n@squared\n    radius: 0\n[view]\nmenu @squared\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains(".authored_text("), "{code}");
}

/// An `else if` at the root is the root too: every branch of the chain returns its own element, where the later ones used to come back inside a column.
#[test]
fn every_branch_of_a_root_else_if_chain_is_the_root() {
    let src = "[logic]\nlet level = 2u8;\n[view]\nif level == 1\n    text \"a\"\nelse if level == 2\n    text \"b\"\nelse\n    text \"c\"\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("Container"), "{code}");
    assert_eq!(code.matches("Ok(Box::new(__text_").count(), 3, "{code}");
    if let Err(e) = syn::parse_file(&code) {
        panic!("generated code does not parse: {e}\n{code}");
    }
}

fn prop_code(logic: &str, value: &str) -> String {
    let src = format!("[logic]\n{logic}\n[view]\ncard value:{value}\n    text \"x\"\n");
    let code = crate::transpile_source(&src, "demo", None, None)
        .unwrap()
        .rust_code;
    syn::parse_file(&code).unwrap_or_else(|e| panic!("{e}:\n{code}"));
    code
}

#[test]
fn a_reactive_of_closure_reading_a_signal_is_not_wrapped_again() {
    let code = prop_code(
        "let sig = signal(0.0f32);",
        "(Reactive::of(move || sig.get()))",
    );
    assert!(
        code.contains("Reactive::of(move || sig.get())") && !code.contains("move || Reactive::of"),
        "a closure the callee keeps reads later, so it must not be wrapped:\n{code}"
    );
    assert!(
        !code.contains("let sig = sig.clone();"),
        "the one reader of the binding takes it:\n{code}"
    );
}

#[test]
fn a_reactive_of_closure_reading_a_dollar_signal_is_not_wrapped_again() {
    let code = prop_code("let s = signal(0.0f32);", "(Reactive::of(move || $s))");
    assert!(
        code.contains("Reactive::of(move || s.get())") && !code.contains("move || Reactive::of"),
        "{code}"
    );
    assert!(code.contains("let s = s.clone();"), "{code}");
}

#[test]
fn an_unknown_helper_taking_a_closure_is_a_read_and_is_wrapped() {
    let code = prop_code(
        "let sig = signal(0.0f32);",
        "(tweened(move || sig.get() * 2.0))",
    );
    assert!(code.contains("Reactive::of(move || tweened("), "{code}");
}

#[test]
fn a_closure_run_by_std_on_the_spot_keeps_the_prop_reactive() {
    let code = prop_code(
        "let values = signal(0.0f32);",
        "(std::array::from_fn(|i| $values[i]))",
    );
    assert!(code.contains("Reactive::of("), "{code}");
}

#[test]
fn a_closure_boxed_or_shared_is_kept_for_later_and_not_wrapped() {
    for callee in ["Rc::new", "Arc::new", "Box::new"] {
        let code = prop_code(
            "let x = signal(0.0f32);",
            &format!("({callee}(move || $x))"),
        );
        assert!(
            code.contains(&format!("{callee}(move || x.get())"))
                && !code.contains("move || Rc")
                && !code.contains("move || Arc")
                && !code.contains("move || Box"),
            "{code}"
        );
    }
}

#[test]
fn a_closure_handed_to_a_method_is_a_read_at_the_call_site() {
    let code = prop_code(
        "let sig = signal(0.0f32);",
        "(items.iter().map(|i| i + sig.get()).sum::<f32>())",
    );
    assert!(code.contains("Reactive::of("), "{code}");
}

#[test]
fn a_direct_signal_read_in_a_prop_still_follows_the_signal() {
    let code = prop_code("let sig = signal(0.0f32);", "(sig.get())");
    assert!(code.contains("Reactive::of(move || sig.get())"), "{code}");
    let code = prop_code("let sig = signal(0.0f32);", "$sig");
    assert!(code.contains(".value(sig.clone())"), "{code}");
}

#[test]
fn a_turbofished_closure_keeping_callee_is_not_wrapped_again() {
    for callee in [
        "Reactive::<f32>::of",
        "Reactive::of::<f32>",
        "Rc::<dyn Fn() -> f32>::new",
        "Box::<dyn Fn() -> f32>::new",
    ] {
        let code = prop_code(
            "let x = signal(0.0f32);",
            &format!("({callee}(move || $x))"),
        );
        assert!(!code.contains("Reactive::of({"), "{callee}:\n{code}");
        assert!(code.contains("move || x.get()"), "{callee}:\n{code}");
    }
}

#[test]
fn a_reactive_constructor_that_stores_its_closure_is_not_wrapped_again() {
    for call in [
        "memo(move || $x + 1.0)",
        "derive(x, |v| v + 1.0)",
        "derive_pair(x, x, |a, b| a + b)",
    ] {
        let code = prop_code("let x = signal(0.0f32);", &format!("({call})"));
        assert!(
            !code.contains("move || memo") && !code.contains("move || derive"),
            "{call}:\n{code}"
        );
    }
}

#[test]
fn a_method_named_like_a_closure_keeping_callee_is_not_one() {
    let code = prop_code("let x = signal(0.0f32);", "(table.memo(move || $x))");
    assert!(code.contains("Reactive::of("), "{code}");
}

const SURFACE_PROPS: &str = "[logic]\npub struct Props {\n    #[props(some, default)]\n    pub style: Option<SurfaceAmendment>,\n}\n";

/// `surface:` runs the caller's amendment over the finished style after the box's own paint, in the base and in each state that swaps it, each closure taking its own copy of the prop.
#[test]
fn a_surface_amends_the_base_paint_and_every_state_after_its_own() {
    let src = format!(
        "{SURFACE_PROPS}[view]\nbox fill:#ffffff hover_style(fill:#eeeeee) active_style(fill:#dddddd) focus_style(stroke:#0000ff) on_press:(|| {{}}) surface:$props.style\n    text \"x\"\n"
    );
    let out = crate::transpile_source(&src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    let spans: Vec<(&str, &str)> = out
        .expr_spans
        .iter()
        .map(|span| {
            let (gen_at, rsx_at, len) = (
                span.gen_start as usize,
                span.rsx_start as usize,
                span.len as usize,
            );
            (&code[gen_at..gen_at + len], &src[rsx_at..rsx_at + len])
        })
        .collect();
    assert!(
        spans.contains(&("props.style", "props.style")),
        "the marker on the place moves onto its copy, where the place still is: {spans:?}"
    );
    assert!(
        spans
            .iter()
            .all(|(generated, written)| generated == written),
        "{spans:?}"
    );
    assert_eq!(
        code.matches("let __props_style = props.style.clone();")
            .count(),
        3,
        "{code}"
    );
    let base = code.split("StyledContainer::new(").nth(1).unwrap();
    assert!(
        base.starts_with(
            "LayoutStyle::new().flex_column(), { let __props_style = props.style.clone(); move |_| amend_surface(RectStyle::default().with_fill("
        ),
        "{code}"
    );
    for state in [".hover_style(", ".active_style("] {
        let call = code.split(state).nth(1).unwrap();
        assert!(
            call.starts_with("{ let __props_style = props.style.clone(); move |_| amend_surface("),
            "{state} is amended too:\n{code}"
        );
    }
    let ring = code.split(".focus_style(").nth(1).unwrap();
    assert!(
        ring.starts_with("move |_| RectStyle {"),
        "the ring is composed over an amended state, so amending it as well would apply the caller twice:\n{code}"
    );
    if let Err(e) = syn::parse_file(code) {
        panic!("generated code does not parse: {e}\n{code}");
    }
}

/// A `col` carries no paint of its own, but one handed its caller's surface has something to paint.
#[test]
fn a_surface_makes_a_plain_container_a_styled_one() {
    let src = format!("{SURFACE_PROPS}[view]\ncol surface:$props.style\n    text \"x\"\n");
    let code = crate::transpile_source(&src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("move |_| amend_surface(RectStyle::default(), &props.style)"),
        "its one paint closure takes the prop itself:\n{code}"
    );
    assert!(!code.contains("__props_style"), "{code}");
}

#[test]
fn a_surface_on_a_tag_that_paints_no_box_is_an_error_naming_those_that_do() {
    let src = format!("{SURFACE_PROPS}[view]\ntext \"x\" surface:$props.style\n");
    let code = crate::transpile_source(&src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains(
            "compile_error!(\"`surface:` amends the paint of a `box`, `row`, `col` or `grid`, and `text` is none of them\")"
        ),
        "{code}"
    );
    assert!(!code.contains("amend_surface"), "{code}");
}

/// The base paint and each state of one box are closures of their own, so a prop they all read is copied into each: moved, the first would take it from the rest.
#[test]
fn a_prop_read_by_several_paint_closures_of_one_box_is_copied_into_each() {
    let src = "[logic]\npub struct Props {\n    #[props(into, default)]\n    pub tint: Reactive<Color>,\n}\n[view]\nbox fill:$props.tint stroke:$props.tint hover_style(fill:$props.tint) opacity:$props.tint.a\n    text \"x\"\n";
    let out = crate::transpile_source(src, "demo", None, None).unwrap();
    let code = &out.rust_code;
    assert!(
        code.contains(
            "{ let __props_tint = props.tint.clone(); move |_| RectStyle { fill: Some(Paint::Solid(__props_tint.get())), border: Some(Border { paint: Paint::Solid(__props_tint.get())"
        ),
        "the base takes one copy for its fill and its stroke:\n{code}"
    );
    assert!(
        code.contains(".hover_style({ let __props_tint = props.tint.clone(); move |_|"),
        "{code}"
    );
    assert!(
        code.contains(
            ".with_opacity({ let __props_tint = props.tint.clone(); move || __props_tint.get().a })"
        ),
        "{code}"
    );
    assert!(!code.contains("(props.tint.get())"), "{code}");
    if let Err(e) = syn::parse_file(code) {
        panic!("generated code does not parse: {e}\n{code}");
    }
}

/// Two elements reading one prop are two closures, and each takes a copy of its own.
#[test]
fn a_prop_read_by_two_elements_is_copied_into_each_closure() {
    let src = "[logic]\npub struct Props {\n    #[props(into, default)]\n    pub tint: Reactive<Color>,\n}\n[view]\ncol\n    text \"x\" color:$props.tint\n    box fill:$props.tint\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(
        code.contains("{ let __props_tint = props.tint.clone(); move |__inherited: TextStyle| __inherited.with_color(__props_tint.get()) }"),
        "{code}"
    );
    assert!(
        code.contains("{ let __props_tint = props.tint.clone(); move |_| RectStyle::default().with_fill(__props_tint.get())"),
        "{code}"
    );
    assert!(!code.contains("(props.tint.get())"), "{code}");
}

/// A `[logic]` binding the base paint and a state of one box both read is copied into each, and so is a loop variable, which every item's closures read anew.
#[test]
fn a_local_and_a_loop_variable_read_by_two_closures_of_one_box_are_copied_into_each() {
    let src = "[logic]\nlet accent = Color::rgb(1.0, 0.0, 0.0);\nlet rows = vec![Color::rgb(0.0, 0.0, 1.0)];\n[view]\ncol\n    box fill:accent hover_style(stroke:accent)\n    for row in rows.clone()\n        box fill:row hover_style(stroke:row)\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    for name in ["accent", "row"] {
        let copy = format!("{{ let {name} = {name}.clone(); move |_|");
        assert_eq!(
            code.matches(&copy).count(),
            2,
            "the base paint and the hover each copy `{name}`:\n{code}"
        );
    }
    if let Err(e) = syn::parse_file(&code) {
        panic!("generated code does not parse: {e}\n{code}");
    }
}

/// A binding that is a reference is copied two ways, each for its receiver. Into a `move` closure it becomes the value it points at, since the closure must be `'static`; handed on by value it stays the reference, since the prop declares that type.
#[test]
fn a_reference_binding_is_owned_by_each_closure_and_handed_on_as_the_reference() {
    let src = "[logic]\nlet location: Option<String> = None;\nlet schema: Option<&'static PropsSchema> = None;\n[view]\ncol\n    if let Some(location) = &location\n        text \"{location}\"\n        text \"at {location}\"\n    if let Some(schema) = schema\n        props_table schema:schema\n        props_list schema:schema\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert_eq!(
        code.matches("let location = location.clone();").count(),
        2,
        "each text closure owns a `String`, not the `&String` the pattern binds:\n{code}"
    );
    for call in ["props_table(PropsTableProps", "props_list(PropsListProps"] {
        assert!(
            code.contains(&format!(
                "{call}::props().schema(::core::clone::Clone::clone(&schema)).build())"
            )),
            "each call is handed the `&PropsSchema` itself:\n{code}"
        );
    }
    if let Err(e) = syn::parse_file(&code) {
        panic!("generated code does not parse: {e}\n{code}");
    }
}
