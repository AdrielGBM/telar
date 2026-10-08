use super::*;

#[test]
fn the_shared_indentation_and_trailing_whitespace_go() {
    assert_eq!(dedent(&["    a  ", "      b", "    c"]), ["a", "  b", "c"]);
}

#[test]
fn a_blank_line_does_not_count_towards_the_indentation() {
    assert_eq!(dedent(&["    a", "", "  ", "    b"]), ["a", "", "", "b"]);
}

#[test]
fn tabs_count_as_indentation_too() {
    assert_eq!(dedent(&["\t\ta", "\t\t\tb"]), ["a", "\tb"]);
}

#[test]
fn other_whitespace_is_content() {
    assert_eq!(dedent(&["\u{3000}a", " b"]), ["\u{3000}a", " b"]);
}
