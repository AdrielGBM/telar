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
