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
        .with_weight(FontWeight::range(100, 900))
        .with_style(FontStyle::Italic)
        .with_axis(FontAxis::new(*b"wght", 100.0, 900.0));
    assert_eq!(asset.family.as_deref(), Some("Telar Test"));
    assert_eq!(asset.weight, FontWeight::range(100, 900));
    assert_eq!(asset.style, FontStyle::Italic);
    assert_eq!(asset.axes, vec![FontAxis::new(*b"wght", 100.0, 900.0)]);
    assert_eq!(FontAsset::file("a.ttf").weight, FontWeight::fixed(400));
}
