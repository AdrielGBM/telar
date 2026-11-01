use super::*;

#[test]
fn info_plist_carries_bundle_identity_and_version() {
    let plist = info_plist("demo", "1.2.3");
    assert!(
        plist.contains("<key>CFBundleExecutable</key><string>demo</string>"),
        "{plist}"
    );
    assert!(
        plist.contains("<key>CFBundleIdentifier</key><string>com.example.demo</string>"),
        "{plist}"
    );
    assert!(
        plist.contains("<key>CFBundleShortVersionString</key><string>1.2.3</string>"),
        "{plist}"
    );
}

#[test]
fn the_icon_notice_lands_in_the_bundles_resources() {
    let staging = super::super::tests::Staging::new("dmg", true);
    stage_app_bundle(
        &staging.out,
        &staging.bin_path,
        &staging.package_dir,
        "demo",
        "1.0.0",
    );
    assert!(staging.out.join("demo.app/Contents/MacOS/demo").is_file());
    assert_eq!(
        staging
            .notice_at("demo.app/Contents/Resources/ICONS-LICENSES.txt")
            .as_deref(),
        Some(super::super::tests::NOTICE)
    );
}
