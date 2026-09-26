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

/// On a component `label` is one of its props, and taking it would steal the button's own text.
#[test]
fn a_component_keeps_its_label_prop() {
    let code = transpile("button label:\"Save\"\n");
    assert!(code.contains(".label(\"Save\")"), "{code}");
    assert!(!code.contains("a11y_label"), "{code}");
}
