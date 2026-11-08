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
    assert_eq!(
        error,
        IconError::MissingSet {
            name: "home".to_string()
        }
    );
    assert!(error.to_string().contains("set:name"), "{error}");
}

#[test]
fn a_bare_name_is_read_in_the_default_set() {
    let id = IconId::parse_with_default("home", Some("mdi")).unwrap();
    assert_eq!(id, IconId::parse("mdi:home").unwrap());
}

#[test]
fn a_written_set_wins_over_the_default() {
    let id = IconId::parse_with_default("lucide:home", Some("mdi")).unwrap();
    assert_eq!(id.prefix(), "lucide");
}

#[test]
fn a_bare_name_without_a_default_set_names_the_missing_set() {
    let error = IconId::parse_with_default("arrow-left", None).unwrap_err();
    assert!(matches!(error, IconError::MissingSet { ref name } if name == "arrow-left"));
    assert!(error.to_string().contains("mdi:arrow-left"), "{error}");
}

#[test]
fn a_bare_name_outside_iconify_naming_is_refused_as_an_id() {
    for default_set in [None, Some("mdi")] {
        let error = IconId::parse_with_default("Home", default_set).unwrap_err();
        assert!(
            matches!(error, IconError::InvalidId { .. }),
            "{default_set:?}: {error}"
        );
    }
}

#[test]
fn an_invalid_default_set_is_refused() {
    let error = IconId::parse_with_default("home", Some("MDI")).unwrap_err();
    assert!(matches!(error, IconError::InvalidId { .. }), "{error}");
}

#[test]
fn a_set_prefix_follows_iconify_naming() {
    for prefix in ["mdi", "material-symbols", "fa6-solid"] {
        assert!(IconId::is_valid_set(prefix), "`{prefix}` should be a set");
    }
    for prefix in ["", "MDI", "mdi:home", "mdi_x", "-mdi", "mdi--x", "mdi/x"] {
        assert!(
            !IconId::is_valid_set(prefix),
            "`{prefix}` should be refused"
        );
    }
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
