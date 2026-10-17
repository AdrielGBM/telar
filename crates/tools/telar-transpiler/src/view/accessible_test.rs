fn transpile(view: &str) -> String {
    let src = format!("[logic]\nlet name = signal(String::from(\"Ada\"));\n[view]\n{view}");
    crate::transpile_source(&src, "demo", None, None)
        .unwrap()
        .rust_code
}

/// The case the attributes exist for: a word drawn one letter per box reads as the word, once, rather than as five letters.
#[test]
fn split_letters_are_named_by_their_parent_and_hidden_one_by_one() {
    let code = transpile(
        "row label:\"Hola\" lang:\"es\"\n    text \"H\" a11y:hidden\n    text \"o\" a11y:hidden\n",
    );
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".a11y_label(|| \"Hola\")"),
        "the row carries the word:\n{code}"
    );
    assert!(
        code.contains(".a11y_lang(|| \"es\")"),
        "and its language:\n{code}"
    );
    assert_eq!(
        code.matches(".a11y_hidden()").count(),
        2,
        "each letter is hidden:\n{code}"
    );
}

/// A name that reads state is re-read, so it can follow a signal or a locale rather than freezing at construction.
#[test]
fn a_reactive_label_is_a_closure_that_reads() {
    let code = transpile("col\n    svg label:$name width:16 height:16\n");
    let call = code.split(".a11y_label(").nth(1).expect("the svg is named");
    assert!(
        call.starts_with("{ let name = name.clone(); move || ") && call.contains("name.get()"),
        "the signal is cloned in and read:\n{code}"
    );
}

#[test]
fn a11y_takes_only_the_words_it_knows() {
    let code = transpile("text \"x\" a11y:invisible\n");
    assert!(
        code.contains("compile_error!"),
        "an unknown a11y value is refused:\n{code}"
    );
}

/// A `scroll` in markup is never a control, so its name is the one a reader hears while it is a Tab stop of its own, and a region that fits is not announced inside the box that already names it.
#[test]
fn a_scroll_is_named_as_a_stop() {
    let code = transpile("scroll label:$name max_height:40\n    text \"x\"\n");
    assert!(!code.contains("compile_error!"), "{code}");
    let call = code
        .split(".stop_label(")
        .nth(1)
        .expect("the scroll names its stop");
    assert!(
        call.contains("name.get()"),
        "the name follows its signal:\n{code}"
    );
    assert!(!code.contains("a11y_label"), "{code}");
}

/// On a component `label` is one of its props, and taking it would steal the button's own text.
#[test]
fn a_component_keeps_its_label_prop() {
    let code = transpile("button label:\"Save\"\n");
    assert!(code.contains(".label(\"Save\")"), "{code}");
    assert!(!code.contains("a11y_label"), "{code}");
}

/// Any built-in box can be a place on the page, named by a literal or by what it reads.
#[test]
fn an_anchor_names_a_place_a_link_reaches() {
    let code = transpile("col anchor:\"contact\"\n    text \"Hi\"\n    box anchor:$name\n");
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".page_anchor(|| \"contact\")"),
        "a literal name:\n{code}"
    );
    let call = code
        .split(".page_anchor(")
        .nth(1)
        .expect("the box is named");
    assert!(
        call.contains("name.get()"),
        "a name read from state follows it:\n{code}"
    );
}

/// A box's passage through the view lands in a signal the author owns, over the range they name.
#[test]
fn view_progress_writes_a_boxs_passage_into_a_signal() {
    let src = "[logic]\nlet p = signal(0.0f32);\nlet q = signal(0.0f32);\n[view]\ncol view_progress:$p view_range:contain\n    box view_progress:$q\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".view_progress(ViewRange::Contain, p.clone())"),
        "the named range:\n{code}"
    );
    assert!(
        code.contains(".view_progress(ViewRange::Cover, q.clone())"),
        "cover when none is named:\n{code}"
    );
}

/// A component call names, tags and hides the item the component returns, under spellings that leave its own `label` and `lang` props alone.
#[test]
fn a_component_call_is_annotated_with_its_own_spelling() {
    let code = transpile(
        "button label:\"Open\" a11y_label:\"Open {$name} in the canvas\" a11y_lang:\"es\" a11y:hidden\n",
    );
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".label(\"Open\")"),
        "the prop is the button's:\n{code}"
    );
    assert!(
        code.contains("ButtonProps::props().label(\"Open\").build()"),
        "not passed as a prop:\n{code}"
    );
    let call = code
        .split(".a11y_label(")
        .nth(1)
        .expect("the returned item is named");
    assert!(
        call.starts_with("{ let name = name.clone(); move || format!(\"Open {} in the canvas\""),
        "a quoted name interpolates, reading its signal:\n{code}"
    );
    assert!(code.contains(".a11y_lang(|| \"es\")"), "{code}");
    assert!(code.contains(".a11y_hidden()"), "{code}");
}

/// The value check covers a component call's `a11y:` as it does a built-in's.
#[test]
fn a_component_calls_a11y_takes_only_the_words_it_knows() {
    let code = transpile("button label:\"x\" a11y:invisible\n");
    assert!(code.contains("compile_error!"), "{code}");
    assert!(!code.contains(".a11y_hidden()"), "{code}");
}

/// A call says what the item it gets back is with a role that only describes it, chained as an annotation rather than passed as a prop.
#[test]
fn a_component_call_says_a_describing_role_of_its_item() {
    let code = transpile("col\n    heading text:\"Recent\" role:h2\n    card role:nav\n");
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains(".a11y_role(::telar::Role::Heading(2))"),
        "{code}"
    );
    assert!(
        code.contains(".a11y_role(::telar::Role::Navigation)"),
        "an alias is the role it means:\n{code}"
    );
    assert!(!code.contains(".role("), "not passed as a prop:\n{code}");
}

fn transpiled(view: &str) -> crate::TranspiledSource {
    crate::transpile_source(&format!("[view]\n{view}"), "demo", None, None).unwrap()
}

/// The `.rsx` line the generated line holding `needle` was written for, counted from 1.
fn rsx_line_of(out: &crate::TranspiledSource, needle: &str) -> Option<u32> {
    let generated = out
        .rust_code
        .lines()
        .position(|line| line.contains(needle))?;
    out.source_map[generated].map(|line| line + 1)
}

/// A role a person operates, or one that promises what the item holds, is the component's to say: a call writing one is an error on its own line, and nothing is chained.
#[test]
fn a_component_call_cannot_say_a_role_that_would_lie_about_its_item() {
    for (role, why) in [
        ("button", "a role a person operates"),
        ("switch", "a role a person operates"),
        ("list", "promises what the boxes inside the item are"),
        ("dialog", "promises what the boxes inside the item are"),
    ] {
        let out = transpiled(&format!("col\n    card role:{role}\n"));
        let message = format!("`role:{role}` is the component's to say, not the call's");
        assert!(
            out.rust_code.contains(&message),
            "{role}:\n{}",
            out.rust_code
        );
        assert!(out.rust_code.contains(why), "{role}:\n{}", out.rust_code);
        assert!(
            !out.rust_code.contains(".a11y_role("),
            "{role}:\n{}",
            out.rust_code
        );
        assert_eq!(rsx_line_of(&out, &message), Some(3), "{role}");
    }
}

/// A word no role answers to is the ordinary value error, listing what a call may say.
#[test]
fn a_component_calls_role_takes_only_the_words_it_knows() {
    let out = transpiled("card role:landmark\n");
    assert!(
        out.rust_code
            .contains("`role:landmark` is not a value of `role`: expected `group`"),
        "{}",
        out.rust_code
    );
    assert!(
        !out.rust_code.contains("`button`"),
        "only the roles a call may say are offered:\n{}",
        out.rust_code
    );
    assert!(!out.rust_code.contains(".a11y_role("), "{}", out.rust_code);
}
