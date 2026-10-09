use crate::{TranspiledSource, transpile_source};

const PROPS: &str =
    "[logic]\npub struct Props {\n    pub label: String,\n}\n\n[view]\ntext \"x\"\n\n";

fn transpiled(previews: &str) -> TranspiledSource {
    let source = format!("{PROPS}{previews}");
    let out = transpile_source(&source, "checkbox", None, None).expect("the fixture transpiles");
    if let Err(e) = syn::parse_file(&out.rust_code) {
        panic!("generated code does not parse: {e}\n{}", out.rust_code);
    }
    out
}

fn code(previews: &str) -> String {
    transpiled(previews).rust_code
}

/// The 1-based `.rsx` line the generated line holding `needle` maps back to.
fn rsx_line_of(out: &TranspiledSource, needle: &str) -> Option<u32> {
    let line = out
        .rust_code
        .lines()
        .position(|line| line.contains(needle))?;
    out.source_map[line].map(|line| line + 1)
}

/// The `.rsx` line a preview header sits on, past the [`PROPS`] prefix.
fn line_in(previews: &str, header: &str) -> u32 {
    let source = format!("{PROPS}{previews}");
    source
        .lines()
        .position(|line| line.starts_with(header))
        .unwrap() as u32
        + 1
}

#[test]
fn previews_compile_only_where_previews_are_on() {
    let code = code("[preview \"Default\"]\ncheckbox\n");
    let (component, previews) = code
        .split_once("::telar::__previews! {\n")
        .expect("the previews sit in the gate");
    assert!(
        component.contains("pub fn checkbox(props: CheckboxProps"),
        "{code}"
    );
    assert!(previews.contains("pub fn checkbox_preview_0("), "{code}");
    assert!(
        previews.contains("pub const CHECKBOX_PREVIEW_ENTRIES"),
        "{code}"
    );
    assert!(previews.trim_end().ends_with("];\n}"), "{code}");
}

/// The root's literal attributes are what its controls change; a value reading state or a closure is wired as written.
#[test]
fn the_roots_literals_are_read_through_the_args() {
    let code = code(
        "[preview \"Default\"]\ncheckbox label:\"I agree\" size:12 count:3u32 ghost on:true tone:Tone::Calm fill:#ff8800 checked:$agree on_change:(|v| log(v)) width:(1 + 2)\n",
    );
    for (key, value) in [
        ("label", "\"I agree\""),
        ("size", "12.0"),
        ("count", "3u32"),
        ("ghost", "true"),
        ("on", "true"),
        ("tone", "Tone::Calm"),
        (
            "fill",
            "Color::rgba(255.0 / 255.0, 136.0 / 255.0, 0.0 / 255.0, 255.0 / 255.0)",
        ),
    ] {
        let read = format!(".{key}(::telar::__preview_arg!(__preview, \"{key}\", {value}))");
        assert!(code.contains(&read), "`{key}` is an arg: {read}\n{code}");
    }
    assert!(code.contains(".checked(agree.clone())"), "{code}");
    assert!(code.contains(".on_change(std::rc::Rc::new("), "{code}");
    assert!(code.contains(".width(1 + 2)"), "{code}");
    assert!(
        code.contains(".__preview_actions(&__preview.actions()).build()"),
        "the root logs its callbacks:\n{code}"
    );
    assert!(
        code.contains(
            ".args(&[::telar::preview::ArgSpec::new(\"label\").default(\"\\\"I agree\\\"\"), ::telar::preview::ArgSpec::new(\"size\").default(\"12.0\"), ::telar::preview::ArgSpec::new(\"count\").default(\"3\"), ::telar::preview::ArgSpec::new(\"ghost\").default(\"true\"), ::telar::preview::ArgSpec::new(\"on\").default(\"true\"), ::telar::preview::ArgSpec::new(\"tone\"), ::telar::preview::ArgSpec::new(\"fill\").default(\"#ff8800\")])"
        ),
        "each arg is declared, with the default its literal says:\n{code}"
    );
}

#[test]
fn args_none_keeps_the_roots_literals_fixed() {
    let code = code("[preview \"Fixed\" args:none]\ncheckbox label:\"I agree\"\n");
    assert!(code.contains(".label(\"I agree\")"), "{code}");
    assert!(!code.contains("__preview_arg!"), "{code}");
    assert!(!code.contains(".args(&["), "{code}");
    assert!(
        code.contains(".__preview_actions(&__preview.actions())"),
        "it still logs its callbacks:\n{code}"
    );
}

/// Only the root is the component under preview: a call nested in a layout is part of what the preview builds around it.
#[test]
fn a_root_that_is_no_component_call_reads_no_literals() {
    let code = code(
        "[preview \"Stacked\"]\ncol gap:8\n    checkbox label:\"One\"\n    checkbox label:\"Two\"\n",
    );
    assert!(!code.contains("__preview_arg!"), "{code}");
    assert!(!code.contains("__preview_actions"), "{code}");
    assert!(code.contains(".gap(8.0)"), "{code}");
}

#[test]
fn declared_args_are_live_signals_the_body_reads() {
    let previews = "[preview \"Bound\" args(agree:false label:\"I agree\" tint:#102030 count:3u32 items:Vec::<u8>::new())]\ncol gap:8\n    checkbox checked:$agree label:$label\n    text \"agree · {$agree}\"\n";
    let out = transpiled(previews);
    let code = &out.rust_code;
    for (name, default) in [
        ("agree", "false"),
        ("label", "\"I agree\""),
        (
            "tint",
            "Color::rgba(16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0, 255.0 / 255.0)",
        ),
        ("count", "3u32"),
        ("items", "Vec::<u8>::new()"),
    ] {
        let signal =
            format!("let {name} = ::telar::__preview_signal!(__preview, \"{name}\", {default});");
        assert!(code.contains(&signal), "{signal}\n{code}");
    }
    assert!(code.contains(".checked(agree.clone())"), "{code}");
    assert!(code.contains("agree.get()"), "{code}");
    let header = line_in(previews, "[preview \"Bound\"");
    assert_eq!(rsx_line_of(&out, "let agree ="), Some(header));
    assert!(
        code.contains(".args(&[::telar::preview::ArgSpec::new(\"agree\").binding(::telar::preview::ArgBinding::Live).default(\"false\"), ::telar::preview::ArgSpec::new(\"label\").binding(::telar::preview::ArgBinding::Live).default(\"\\\"I agree\\\"\"), ::telar::preview::ArgSpec::new(\"tint\").binding(::telar::preview::ArgBinding::Live).default(\"#102030\"), ::telar::preview::ArgSpec::new(\"count\").binding(::telar::preview::ArgBinding::Live).default(\"3\"), ::telar::preview::ArgSpec::new(\"items\").binding(::telar::preview::ArgBinding::Live)])"),
        "{code}"
    );
}

/// A default written as Rust is copied byte for byte, so an error rustc reports on it lands on the text in the header.
#[test]
fn a_declared_default_maps_back_to_the_header() {
    let previews = "[preview \"Bound\" args(items:Vec::<u8>::new())]\ncol\n";
    let out = transpiled(previews);
    let source = format!("{PROPS}{previews}");
    let span = out
        .expr_spans
        .iter()
        .find(|span| {
            let start = span.rsx_start as usize;
            &source[start..start + span.len as usize] == "Vec::<u8>::new()"
        })
        .expect("the default is a verbatim span");
    let generated = &out.rust_code[span.gen_start as usize..(span.gen_start + span.len) as usize];
    assert_eq!(generated, "Vec::<u8>::new()");
}

#[test]
fn a_declared_arg_the_root_also_sets_is_refused_on_the_header() {
    let previews = "[preview \"Twice\" args(label:\"A\")]\ncheckbox label:\"B\"\n";
    let out = transpiled(previews);
    let error = "declares `label` in `args(…)` and also sets it as a literal on its root";
    assert!(out.rust_code.contains(error), "{}", out.rust_code);
    assert_eq!(
        rsx_line_of(&out, error),
        Some(line_in(previews, "[preview \"Twice\""))
    );
}

#[test]
fn the_meta_section_titles_every_variant_and_sets_what_they_share() {
    let code = code(
        "[previews \"Forms/Checkbox\" layout:centered tags:[forms] mode:dark]\nA checkbox.\n\n[preview \"Default\"]\ncheckbox\n\n[preview \"Wide\" layout:fullscreen tags:[slow] viewport:390x844 bg:#202020 locale:ar dir:rtl decorator:frames::card fixture:seed]\ncheckbox\n",
    );
    let (first, second) = code.split_once("--checkbox--wide").unwrap();
    let first = first.split_once("CHECKBOX_PREVIEW_ENTRIES").unwrap().1;
    assert!(first.contains(".title(\"Forms/Checkbox\")"), "{first}");
    assert!(first.contains(".docs(\"A checkbox.\")"), "{first}");
    assert!(
        second.contains(".docs(\"A checkbox.\")"),
        "every variant carries the prose:\n{second}"
    );
    assert!(
        first.contains(".layout(::telar::preview::Layout::Centered)"),
        "{first}"
    );
    assert!(first.contains(".mode(\"dark\")"), "{first}");
    assert!(first.contains(".tags(&[\"forms\"])"), "{first}");
    assert!(second.contains(".title(\"Forms/Checkbox\")"), "{second}");
    assert!(
        second.contains(".layout(::telar::preview::Layout::Fullscreen)"),
        "{second}"
    );
    assert!(second.contains(".tags(&[\"forms\", \"slow\"])"), "{second}");
    assert!(second.contains(".viewport(390.0, 844.0)"), "{second}");
    assert!(
        second
            .contains(".bg(Color::rgba(32.0 / 255.0, 32.0 / 255.0, 32.0 / 255.0, 255.0 / 255.0))"),
        "{second}"
    );
    assert!(second.contains(".locale(\"ar\")"), "{second}");
    assert!(second.contains(".dir(::telar::Direction::Rtl)"), "{second}");
    assert!(second.contains(".decorate(frames::card)"), "{second}");
    let build = code.split_once("fn checkbox_preview_1").unwrap().1;
    assert!(build.contains("    seed();\n"), "{build}");
}

#[test]
fn a_matrix_is_named_or_written_out() {
    let code = code(
        "[preview \"Themes\" matrix:themes]\ncheckbox\n\n[preview \"Grid\" args(agree:false) matrix:(mode:[light dark] locale:[en \"ar\"] dir:[ltr rtl] viewport:[390x844 phone] control_size:[small large] size:[12 16] label:[\"Save\" Primary] agree:[true false])]\ncheckbox size:12 label:\"Save\" checked:$agree\n",
    );
    assert!(
        code.contains(".matrix(::telar::preview::Matrix::Named(\"themes\"))"),
        "{code}"
    );
    assert!(
        code.contains(".matrix(::telar::preview::Matrix::Axes(&[::telar::preview::Axis::Mode(&[\"light\", \"dark\"]), ::telar::preview::Axis::Locale(&[\"en\", \"ar\"]), ::telar::preview::Axis::Dir(&[::telar::Direction::Ltr, ::telar::Direction::Rtl]), ::telar::preview::Axis::Viewport(&[\"390x844\", \"phone\"]), ::telar::preview::Axis::ControlSize(&[::telar::ControlSize::Small, ::telar::ControlSize::Large]), ::telar::preview::Axis::Arg(\"size\", &[\"12\", \"16\"]), ::telar::preview::Axis::Arg(\"label\", &[\"\\\"Save\\\"\", \"Primary\"]), ::telar::preview::Axis::Arg(\"agree\", &[\"true\", \"false\"])]))"),
        "{code}"
    );
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn a_matrix_the_transpiler_cannot_read_is_refused_on_its_header() {
    for (matrix, error) in [
        (
            "matrix:(dir:[up])",
            "`matrix:` axis `dir`: `up` is not ltr or rtl",
        ),
        (
            "matrix:(viewport:[a/b])",
            "`a/b` is not a size like 390x844",
        ),
        (
            "matrix:(control_size:[huge])",
            "`huge` is not one of mini, small, regular, large",
        ),
        (
            "matrix:(mode:[light light])",
            "lists `light` more than once",
        ),
        ("matrix:(size:[12 a+b])", "`a+b` is not an arg value"),
        ("matrix:(mode:[a] mode:[b])", "varies `mode` twice"),
        ("matrix:()", "varies nothing"),
        ("matrix:(mode)", "needs its values"),
        ("matrix:{x}", "is neither a matrix's name"),
        (
            "matrix:(weight:[1 2])",
            "varies `weight`, which is neither one of mode, locale, dir, viewport, control_size nor an arg of this preview",
        ),
    ] {
        let previews = format!("[preview \"Grid\" {matrix}]\ncheckbox size:12\n");
        let out = transpiled(&previews);
        assert!(
            out.rust_code.contains(error),
            "{matrix}: {error}\n{}",
            out.rust_code
        );
        assert_eq!(
            rsx_line_of(&out, error),
            Some(line_in(&previews, "[preview \"Grid\"")),
            "{matrix}"
        );
    }
}

#[test]
fn an_unknown_option_is_refused_on_the_header_that_wrote_it() {
    let previews = "[previews \"Forms\" colour:red]\nProse.\n\n[preview \"Default\" widht:360 layout:sideways layout:centered]\ncheckbox\n";
    let out = transpiled(previews);
    let code = &out.rust_code;
    let meta = "[previews] has no option `colour`: a preview header takes layout, viewport, bg, mode, locale, dir, tags, matrix, decorator, fixture, surface, animate, args";
    assert!(code.contains(meta), "{code}");
    assert_eq!(
        rsx_line_of(&out, meta),
        Some(line_in(previews, "[previews"))
    );
    let header = line_in(previews, "[preview \"Default\"");
    for error in [
        "[preview \\\"Default\\\"] has no option `widht`",
        "`layout:sideways` is not a layout: write padded, centered or fullscreen",
        "[preview \\\"Default\\\"] sets `layout` twice",
    ] {
        assert!(code.contains(error), "{error}\n{code}");
        assert_eq!(rsx_line_of(&out, error), Some(header), "{error}");
    }
}

#[test]
fn an_option_value_the_transpiler_cannot_read_is_refused() {
    for (option, error) in [
        ("layout", "`layout` needs a value, like `layout:centered`"),
        ("viewport:wide", "`viewport:wide` is not a size"),
        ("bg:surface", "`bg:surface` is not a colour"),
        ("dir:up", "`dir:up` is not a direction"),
        ("args:all", "`args:all` is not an option"),
        ("animate:yes", "`animate` is a flag"),
        ("animate", "`animate` plays a surface's enter transition"),
    ] {
        let code = code(&format!("[preview \"Default\" {option}]\ncheckbox\n"));
        assert!(code.contains(error), "{option}: {error}\n{code}");
    }
}

/// The entry carries the preview as written, for the source panel, and where it starts in the file.
#[test]
fn an_entry_carries_its_source_and_where_it_starts() {
    let previews = "[preview \"One\"]\ncheckbox label:\"A\"\n\n[preview \"Two\"]\ncheckbox\n\n[play]\ncanvas.click(by_text(\"A\"))?;\n";
    let out = transpiled(previews);
    let one = "[preview \\\"One\\\"]\\ncheckbox label:\\\"A\\\"";
    let line = line_in(previews, "[preview \"One\"");
    assert!(
        out.rust_code.contains(&format!(
            ".source(\"{one}\", &[::telar::preview::SourceSpan::new(0, 34, {line})])"
        )),
        "{}",
        out.rust_code
    );
    let two = "[preview \\\"Two\\\"]\\ncheckbox\\n\\n[play]\\ncanvas.click(by_text(\\\"A\\\"))?;\"";
    assert!(out.rust_code.contains(two), "{}", out.rust_code);
    assert!(
        !out.rust_code.contains("canvas.click(by_text(\"A\"))?;\n"),
        "a play zone is not emitted yet:\n{}",
        out.rust_code
    );
}

#[test]
fn an_entry_describes_its_component_by_the_files_props() {
    let code = code("[preview \"Default\"]\ncol\n");
    assert!(
        code.contains(".props(<CheckboxProps as ::telar::preview::HasPropsSchema>::schema)"),
        "{code}"
    );
}

#[test]
fn every_entry_line_maps_to_its_header() {
    let previews = "[preview \"Default\" layout:centered]\ncheckbox\n";
    let out = transpiled(previews);
    let header = line_in(previews, "[preview \"Default\"");
    for needle in ["PreviewEntry::new(", ".layout(", ".props(", ".source("] {
        assert_eq!(rsx_line_of(&out, needle), Some(header), "{needle}");
    }
}

#[test]
fn a_meta_section_with_no_preview_under_it_is_still_checked() {
    let previews = "[previews \"Forms\" colour:red]\nProse.\n";
    let out = transpiled(previews);
    assert!(
        out.rust_code.contains("[previews] has no option `colour`"),
        "{}",
        out.rust_code
    );
    assert!(
        !out.rust_code.contains("PREVIEW_ENTRIES"),
        "{}",
        out.rust_code
    );
    assert!(out.preview_names.is_empty());
}

#[test]
fn a_file_without_prose_carries_no_docs() {
    let code = code("[previews \"Forms\"]\n\n[preview \"Default\"]\ncheckbox\n");
    assert!(!code.contains(".docs("), "{code}");
}
