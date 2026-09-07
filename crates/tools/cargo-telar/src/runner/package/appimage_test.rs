use super::*;

#[test]
fn apprun_execs_the_bundled_binary() {
    let script = apprun_script("myapp");
    assert!(
        script.starts_with("#!/bin/sh\n"),
        "AppRun has to be a shell script:\n{script}"
    );
    assert!(
        script.contains("exec \"$HERE/usr/bin/myapp\" \"$@\""),
        "{script}"
    );
}
