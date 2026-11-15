use super::*;

#[test]
fn a_relative_file_resolves_beside_the_executable_and_an_absolute_one_stays_put() {
    let exe = Path::new("/opt/app");
    assert_eq!(
        FontSource::File("fonts/a.ttf".into()).resolved_path(Some(exe)),
        Some(PathBuf::from("/opt/app/fonts/a.ttf"))
    );
    assert_eq!(
        FontSource::File("/usr/share/a.ttf".into()).resolved_path(Some(exe)),
        Some(PathBuf::from("/usr/share/a.ttf"))
    );
    assert_eq!(FontSource::Static(b"x").resolved_path(Some(exe)), None);
}

#[test]
fn sources_compare_by_content_across_kinds_of_the_same_kind_only() {
    static BYTES: [u8; 3] = [1, 2, 3];
    assert_eq!(FontSource::Static(&BYTES), FontSource::Static(&[1, 2, 3]));
    assert_eq!(
        FontSource::Shared(Arc::from(&BYTES[..])),
        FontSource::Shared(Arc::from(&BYTES[..]))
    );
    assert_ne!(
        FontSource::Static(&BYTES),
        FontSource::Shared(Arc::from(&BYTES[..]))
    );
}

#[test]
fn the_builder_records_what_the_declaration_said() {
    let asset = FontAsset::embedded(b"face")
        .named("Telar Test")
        .with_weight(FontWeight::range(100, 900).unwrap())
        .with_style(FontStyle::Italic)
        .with_axis(FontAxis::new(*b"wght", 100.0, 900.0));
    assert_eq!(asset.family.as_deref(), Some("Telar Test"));
    assert_eq!(asset.weight, FontWeight::range(100, 900).unwrap());
    assert_eq!(asset.style, FontStyle::Italic);
    assert_eq!(asset.axes, vec![FontAxis::new(*b"wght", 100.0, 900.0)]);
    assert_eq!(FontAsset::file("a.ttf").weight, FontWeight::default());
}

#[test]
fn a_reversed_or_out_of_range_weight_is_an_error() {
    let reversed = FontWeight::range(700, 300).unwrap_err();
    assert_eq!((reversed.min, reversed.max), (700, 300));
    assert!(FontWeight::range(0, 400).is_err());
    assert!(FontWeight::range(400, 1001).is_err());
    assert!(FontWeight::fixed(0).is_err());
    assert!(FontWeight::fixed(1001).is_err());
}

#[test]
fn a_valid_weight_reads_back_its_bounds() {
    let weight = FontWeight::range(1, 1000).unwrap();
    assert_eq!((weight.min(), weight.max()), (1, 1000));
    assert_eq!(FontWeight::fixed(700).unwrap().min(), 700);
}
