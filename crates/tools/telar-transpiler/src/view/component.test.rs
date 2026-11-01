use crate::{RsxSpan, SourceMap, transpile_source};

fn transpiled(rsx: &str) -> crate::TranspiledSource {
    transpile_source(rsx, "home", None, None).unwrap()
}

#[test]
fn a_bare_word_on_a_component_call_is_the_value_of_that_name_in_kw() {
    let out = transpiled("[view]\nbutton label:\"Go\" size:small variant:(ghost) tone:danger\n");
    assert!(
        out.rust_code.contains(
            ".size(::telar::kw::small).variant(::telar::kw::ghost).tone(::telar::kw::danger)"
        ),
        "{}",
        out.rust_code
    );
}

#[test]
fn a_binding_of_the_file_named_like_a_word_wins() {
    let rsx = "[logic]\nlet small = ControlSize::Large;\nfn ghost() -> Variant { Variant::Soft }\nuse crate::tones::danger;\n\n[view]\ncol\n    let large = ControlSize::Small;\n    button size:small variant:ghost tone:danger\n    button size:large\n    for regular in sizes()\n        button size:regular\n";
    let out = transpiled(rsx);
    for word in ["small", "ghost", "danger", "large", "regular"] {
        assert!(
            !out.rust_code.contains(&format!("::telar::kw::{word}")),
            "`{word}`:\n{}",
            out.rust_code
        );
    }
}

#[test]
fn a_word_on_a_built_in_tag_or_in_a_path_is_not_one() {
    let out = transpiled(
        "[view]\nrow align:center\n    button size:(ControlSize::Small) variant:Variant::Ghost\n",
    );
    assert!(
        !out.rust_code.contains("::telar::kw::"),
        "{}",
        out.rust_code
    );
}

#[test]
fn a_word_no_axis_has_is_rustc_s_error_on_the_word_the_author_wrote() {
    let rsx = "[view]\ncol\n    button label:\"Go\" size:smal\n";
    let out = transpiled(rsx);
    let map = SourceMap::new(out.source_map.clone(), out.expr_spans.clone());
    let word = out.rust_code.find("::telar::kw::smal").unwrap() + "::telar::kw::".len();
    let in_rsx = rsx.find("smal").unwrap() as u32;
    assert_eq!(
        map.locate(&out.rust_code, word as u32, word as u32 + 4, rsx),
        Some(RsxSpan::Exact {
            start: in_rsx,
            end: in_rsx + 4
        })
    );
    let call = out.rust_code.find("::telar::kw::smal").unwrap() as u32;
    assert_eq!(
        map.locate(&out.rust_code, call, call + 17, rsx),
        Some(RsxSpan::Line(2)),
        "a word the prop does not take is rustc's error on the whole value, which lands on the call's line"
    );
}

#[test]
fn a_word_on_a_preview_root_is_an_arg_whose_text_is_the_word() {
    let rsx = "[view]\ntext \"x\"\n\n[preview \"Small\" matrix:(variant:[solid ghost])]\nbutton label:\"Go\" size:small variant:solid\n";
    let out = transpiled(rsx);
    assert!(
        out.rust_code
            .contains("::telar::__preview_arg!(__preview, \"size\", ::telar::kw::"),
        "{}",
        out.rust_code
    );
    assert!(
        out.rust_code
            .contains("::telar::preview::ArgSpec::new(\"size\").default(\"small\")"),
        "{}",
        out.rust_code
    );
    assert!(
        out.rust_code
            .contains("::telar::preview::Axis::Arg(\"variant\", &[\"solid\", \"ghost\"])"),
        "{}",
        out.rust_code
    );
    assert!(
        !out.rust_code.contains("compile_error!"),
        "{}",
        out.rust_code
    );
}
