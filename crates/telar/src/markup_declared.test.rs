use super::*;

fn weight(style: Vec<u16>, value: Option<u16>) -> Vec<u16> {
    style.declaring_if(value, |mut style, weight| {
        style.push(weight);
        style
    })
}

#[test]
fn a_value_declares_itself_and_an_option_declares_what_it_holds() {
    assert_eq!(Handed(600u16).declared(), Some(600));
    assert_eq!(Handed(Some(600u16)).declared(), Some(600));
    assert_eq!(Handed(None::<u16>).declared(), None);
}

#[test]
fn nothing_declared_leaves_the_style_as_it_was() {
    assert_eq!(weight(vec![400], Handed(None::<u16>).declared()), [400]);
    assert_eq!(weight(vec![400], Handed(700u16).declared()), [400, 700]);
}
