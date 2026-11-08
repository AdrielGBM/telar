use super::*;

fn fake_rustfmt(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("rustfmt{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&exe, "").unwrap();
    exe
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rustfmt_resolve_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn an_explicit_rustfmt_variable_wins_over_everything() {
    let dir = scratch("explicit");
    fake_rustfmt(&dir);
    let env = RustfmtEnv {
        rustfmt: Some("/opt/custom/rustfmt".into()),
        path: Some(dir.join("bin").into()),
        cargo_home: Some(dir.clone().into()),
        home: None,
    };
    assert_eq!(
        resolve_rustfmt(&env),
        Some(PathBuf::from("/opt/custom/rustfmt"))
    );
}

#[test]
fn the_first_path_entry_holding_rustfmt_is_used() {
    let first = scratch("path_first");
    let second = scratch("path_second");
    let empty = scratch("path_empty");
    let expected = fake_rustfmt(&first);
    fake_rustfmt(&second);
    let path =
        std::env::join_paths([empty.join("bin"), first.join("bin"), second.join("bin")]).unwrap();
    let env = RustfmtEnv {
        path: Some(path),
        ..RustfmtEnv::default()
    };
    assert_eq!(resolve_rustfmt(&env), Some(expected));
}

#[test]
fn cargo_home_is_searched_when_path_has_no_rustfmt() {
    let cargo_home = scratch("cargo_home");
    let home = scratch("home_unused");
    let expected = fake_rustfmt(&cargo_home);
    fake_rustfmt(&home.join(".cargo"));
    let env = RustfmtEnv {
        path: Some(scratch("path_none").into()),
        cargo_home: Some(cargo_home.into()),
        home: Some(home.into()),
        ..RustfmtEnv::default()
    };
    assert_eq!(resolve_rustfmt(&env), Some(expected));
}

#[test]
fn the_home_cargo_directory_is_the_fallback_without_cargo_home() {
    let home = scratch("home_fallback");
    let expected = fake_rustfmt(&home.join(".cargo"));
    let env = RustfmtEnv {
        home: Some(home.into()),
        ..RustfmtEnv::default()
    };
    assert_eq!(resolve_rustfmt(&env), Some(expected));
}

#[test]
fn nothing_resolves_when_no_location_holds_rustfmt() {
    let env = RustfmtEnv {
        path: Some(scratch("nowhere").into()),
        cargo_home: Some(scratch("nowhere_home").into()),
        ..RustfmtEnv::default()
    };
    assert_eq!(resolve_rustfmt(&env), None);
}
