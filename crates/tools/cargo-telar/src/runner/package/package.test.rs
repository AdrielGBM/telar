use super::*;

#[test]
fn desktop_entry_includes_icon_only_when_requested() {
    let plain = desktop_entry_file("myapp", false);
    assert!(plain.contains("[Desktop Entry]"), "{plain}");
    assert!(plain.contains("Type=Application"), "{plain}");
    assert!(plain.contains("Name=myapp"), "{plain}");
    assert!(plain.contains("Exec=myapp"), "{plain}");
    assert!(plain.contains("Categories=Utility;"), "{plain}");
    assert!(
        !plain.contains("Icon="),
        "an entry with no icon requested must not name one:\n{plain}"
    );

    let with_icon = desktop_entry_file("myapp", true);
    assert!(with_icon.contains("Icon=myapp"), "{with_icon}");
}

/// A package whose bake wrote an icon notice (or none), a built binary, and an empty directory to stage into, all under a fresh temporary root.
pub(super) struct Staging {
    root: PathBuf,
    pub(super) package_dir: PathBuf,
    pub(super) bin_path: PathBuf,
    pub(super) out: PathBuf,
}

pub(super) const NOTICE: &str = "Icons in demo\n";

impl Staging {
    pub(super) fn new(name: &str, with_notice: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("cargo_telar_staging_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package_dir = root.join("demo");
        std::fs::create_dir_all(package_dir.join(".telar")).unwrap();
        if with_notice {
            std::fs::write(
                package_dir
                    .join(".telar")
                    .join(telar_baker::ICONS_NOTICE_FILENAME),
                NOTICE,
            )
            .unwrap();
        }
        let bin_path = root.join("target/release/demo");
        std::fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
        std::fs::write(&bin_path, "binary").unwrap();
        Self {
            out: root.join("out"),
            root,
            package_dir,
            bin_path,
        }
    }

    /// The notice staged at `relative` under the output, `None` when nothing is there.
    pub(super) fn notice_at(&self, relative: &str) -> Option<String> {
        std::fs::read_to_string(self.out.join(relative)).ok()
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_raw_binary_ships_the_icon_notice_beside_it() {
    let staging = Staging::new("dir", true);
    stage_desktop_dir(
        &staging.out,
        &staging.bin_path,
        &staging.package_dir,
        "demo",
    );
    assert_eq!(staging.notice_at("demo").as_deref(), Some("binary"));
    assert_eq!(
        staging.notice_at("ICONS-LICENSES.txt").as_deref(),
        Some(NOTICE)
    );
}

#[test]
fn a_package_that_baked_no_icon_ships_no_notice() {
    let staging = Staging::new("dir_plain", false);
    stage_desktop_dir(
        &staging.out,
        &staging.bin_path,
        &staging.package_dir,
        "demo",
    );
    assert!(staging.out.join("demo").is_file());
    assert_eq!(staging.notice_at("ICONS-LICENSES.txt"), None);
    assert_eq!(
        copy_icon_notice(&staging.package_dir, &staging.out.join("x.txt")),
        None
    );
}

#[test]
fn an_apks_notice_is_named_for_its_package_beside_it() {
    let staging = Staging::new("apk", true);
    let path = apk_notice_path(&staging.out, "demo");
    assert_eq!(path, staging.out.join("demo-ICONS-LICENSES.txt"));
    assert_eq!(
        copy_icon_notice(&staging.package_dir, &path),
        Some(path.clone())
    );
    assert_eq!(
        staging.notice_at("demo-ICONS-LICENSES.txt").as_deref(),
        Some(NOTICE)
    );
}
