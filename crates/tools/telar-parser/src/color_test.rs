use super::*;

#[test]
fn every_accepted_length() {
    assert_eq!(parse_hex("#f80"), Some([255, 136, 0, 255]));
    assert_eq!(parse_hex("#f808"), Some([255, 136, 0, 136]));
    assert_eq!(parse_hex("ff8800"), Some([255, 136, 0, 255]));
    assert_eq!(parse_hex("#ff880080"), Some([255, 136, 0, 128]));
}

#[test]
fn rejects_what_is_not_a_colour() {
    assert_eq!(parse_hex("#zzz"), None);
    assert_eq!(parse_hex("#12"), None);
    assert_eq!(parse_hex("#1234567"), None);
    assert_eq!(parse_hex(""), None);
    assert_eq!(parse_hex("#áéí"), None);
}
