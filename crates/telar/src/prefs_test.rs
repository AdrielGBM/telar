use super::*;

struct At(PathBuf);

impl AppPathsProvider for At {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }
    fn data_dir(&self) -> Option<PathBuf> {
        None
    }
    fn cache_dir(&self) -> Option<PathBuf> {
        None
    }
}

fn config(name: &str) -> At {
    let dir = std::env::temp_dir().join(format!("telar-userprefs-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("app")).unwrap();
    At(dir)
}

#[test]
fn an_unparsable_file_is_not_overwritten_by_a_save() {
    let paths = config("unparsable");
    let file = paths.0.join("app").join("prefs.toml");
    std::fs::write(&file, "history = [unterminated").unwrap();
    let mut prefs = UserPrefs::load("app", &paths);
    prefs.history = vec!["/start".into()];
    assert!(prefs.save("app", &paths).is_err());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "history = [unterminated"
    );
}

#[test]
fn a_file_that_is_not_text_is_not_overwritten_by_a_save() {
    let paths = config("binary");
    let file = paths.0.join("app").join("prefs.toml");
    std::fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
    let prefs = UserPrefs::load("app", &paths);
    assert!(prefs.save("app", &paths).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), [0xff, 0xfe, 0x00]);
}

#[test]
fn a_missing_file_is_saved_and_read_back() {
    let paths = config("missing");
    let mut prefs = UserPrefs::load("app", &paths);
    prefs.history = vec!["/start".into()];
    prefs.save("app", &paths).unwrap();
    assert_eq!(UserPrefs::load("app", &paths).history, ["/start"]);
}
