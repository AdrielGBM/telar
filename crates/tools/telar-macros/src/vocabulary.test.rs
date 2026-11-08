use super::*;

fn expand_str(source: &str) -> syn::Result<String> {
    Ok(expand(syn::parse_str(source)?)?.to_string())
}

const AXES: &str =
    "size => crate::ControlSize, variant => crate::Variant, tone => crate::Tone { Color }";

#[test]
fn each_word_names_its_axis_s_value() {
    let out = expand_str(AXES).unwrap();
    assert!(
        out.contains("pub use crate :: ControlSize :: Small as small ;"),
        "{out}"
    );
    assert!(
        out.contains("pub use crate :: Variant :: Ghost as ghost ;"),
        "{out}"
    );
    assert!(
        out.contains("pub use crate :: Tone :: Danger as danger ;"),
        "{out}"
    );
}

#[test]
fn a_value_no_word_names_has_none() {
    let out = expand_str(AXES).unwrap();
    assert!(
        out.contains("Self :: Color { .. } => :: core :: option :: Option :: None"),
        "{out}"
    );
    assert!(
        out.contains("\"small\" => :: core :: option :: Option :: Some (Self :: Small)"),
        "{out}"
    );
}

#[test]
fn the_axes_are_the_list_s() {
    let err = expand_str("size => crate::ControlSize").unwrap_err();
    assert!(err.to_string().contains("size, variant, tone"), "{err}");
}
