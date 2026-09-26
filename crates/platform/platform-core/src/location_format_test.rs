use super::*;

fn projects() -> Location {
    Location::from_segments(["es", "projects"])
}

#[test]
fn the_root_is_the_base_itself() {
    assert_eq!(LocationFormat::root().format(&Location::root()), "/");
    assert_eq!(
        LocationFormat::new("portfolio").format(&Location::root()),
        "/portfolio/"
    );
}

#[test]
fn every_spelling_of_a_base_means_the_same_directory() {
    for base in [
        "portfolio",
        "/portfolio",
        "portfolio/",
        "/portfolio/",
        " /portfolio// ",
    ] {
        assert_eq!(LocationFormat::new(base).base(), "/portfolio/", "{base:?}");
    }
    assert_eq!(LocationFormat::new("").base(), "/");
    assert_eq!(LocationFormat::new("/").base(), "/");
}

#[test]
fn segments_params_and_fragment_are_written_in_uri_order() {
    let location = projects()
        .with_param("tag", "rust")
        .with_param("page", "2")
        .with_fragment("telar");
    assert_eq!(
        LocationFormat::root().format(&location),
        "/es/projects?tag=rust&page=2#telar"
    );
}

#[test]
fn a_trailing_slash_is_written_only_below_the_root() {
    let format = LocationFormat::new("/site").with_trailing_slash(true);
    assert_eq!(format.format(&projects()), "/site/es/projects/");
    assert_eq!(format.format(&Location::root()), "/site/");
    assert_eq!(
        format.format(&Location::from_segments(["es"]).with_fragment("top")),
        "/site/es/#top"
    );
}

#[test]
fn reserved_characters_are_escaped_where_they_would_be_misread() {
    let location = Location::from_segments(["a/b", "c d", "ñ"])
        .with_param("q", "1+1=2&more")
        .with_fragment("x y");
    let written = LocationFormat::root().format(&location);
    assert_eq!(written, "/a%2Fb/c%20d/%C3%B1?q=1%2B1%3D2%26more#x%20y");
    assert_eq!(LocationFormat::root().parse(&written), Some(location));
}

#[test]
fn what_is_written_reads_back_under_any_base() {
    let location = projects()
        .with_param("tag", "a")
        .with_param("tag", "b")
        .with_fragment("section-2");
    for format in [
        LocationFormat::root(),
        LocationFormat::new("/portfolio"),
        LocationFormat::new("deep/base").with_trailing_slash(true),
    ] {
        let written = format.format(&location);
        assert_eq!(format.parse(&written), Some(location.clone()), "{written}");
        assert_eq!(
            format.parse(&format.format(&Location::root())),
            Some(Location::root())
        );
    }
}

#[test]
fn a_path_outside_the_base_names_no_location() {
    let format = LocationFormat::new("/portfolio");
    assert_eq!(format.parse("/other/es"), None);
    assert_eq!(format.parse("/portfolios/es"), None);
    assert_eq!(format.parse("/portfolio"), Some(Location::root()));
}

#[test]
fn an_absolute_uri_loses_its_scheme_and_authority() {
    let format = LocationFormat::root();
    for uri in [
        "https://example.com/es/projects",
        "myapp:/es/projects",
        "myapp://open/es/projects",
    ] {
        assert_eq!(format.parse(uri), Some(projects()), "{uri}");
    }
    assert_eq!(
        format.parse("https://example.com?x=1"),
        Some(Location::root().with_param("x", "1"))
    );
}

#[test]
fn a_relative_reference_is_read_against_the_base() {
    assert_eq!(
        LocationFormat::new("/portfolio").parse("es/projects"),
        Some(projects())
    );
}

#[test]
fn empty_segments_params_and_fragments_are_not_locations() {
    let format = LocationFormat::root();
    assert_eq!(format.parse("//es///projects/"), Some(projects()));
    assert_eq!(format.parse("/?&&#"), Some(Location::root()));
    assert_eq!(
        format.parse("/?flag"),
        Some(Location::root().with_param("flag", ""))
    );
}

#[test]
fn a_plus_in_a_query_is_a_space_and_in_a_path_is_itself() {
    assert_eq!(
        LocationFormat::root().parse("/c++?q=a+b"),
        Some(Location::from_segments(["c++"]).with_param("q", "a b"))
    );
}

#[test]
fn a_broken_escape_is_kept_as_written() {
    assert_eq!(
        LocationFormat::root().parse("/100%/%zz"),
        Some(Location::from_segments(["100%", "%zz"]))
    );
}
