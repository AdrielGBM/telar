use super::*;

#[test]
fn parses_set_and_name() {
    let id = IconId::parse("mdi:home").unwrap();
    assert_eq!(id.prefix(), "mdi");
    assert_eq!(id.name(), "home");
    assert_eq!(id.to_string(), "mdi:home");
    assert_eq!(id.path(), "mdi/home");
}

#[test]
fn hyphens_and_digits_are_part_of_either_half() {
    let id = IconId::parse("material-symbols:arrow-left-2").unwrap();
    assert_eq!(id.prefix(), "material-symbols");
    assert_eq!(id.name(), "arrow-left-2");
}

#[test]
fn a_path_reads_back_into_the_same_id() {
    let id = IconId::parse("tabler:settings").unwrap();
    assert_eq!(IconId::from_path(&id.path()).unwrap(), id);
}

#[test]
fn an_id_without_a_set_is_refused() {
    let error = IconId::parse("home").unwrap_err();
    assert!(matches!(error, IconError::InvalidId { ref id, .. } if id == "home"));
    assert!(error.to_string().contains("set:name"), "{error}");
}

#[test]
fn anything_outside_iconify_naming_is_refused() {
    for id in [
        ":home",
        "mdi:",
        "MDI:home",
        "mdi:Home",
        "mdi:home_outline",
        "mdi:-home",
        "mdi:home-",
        "mdi:home--outline",
        "mdi:../etc",
        "mdi:a/b",
        "mdi:home.svg",
        "mdi:home:extra",
    ] {
        assert!(IconId::parse(id).is_err(), "`{id}` should be refused");
    }
}

#[test]
fn from_str_matches_parse() {
    let id: IconId = "lucide:circle".parse().unwrap();
    assert_eq!(id, IconId::parse("lucide:circle").unwrap());
}
