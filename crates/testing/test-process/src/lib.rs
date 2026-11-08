//! Runs one test alone in a process of its own: the test binary started again, filtered to that one test. A test that changes what the whole process shares (a global logger, an installed store, a signal handler) runs this way, so it neither sees another test's change nor leaves its own behind.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::process::Command;

const OWN_TEST_VAR: &str = "TELAR_TEST_OWN_PROCESS";

/// One test of the running binary, and the environment its own process starts with.
pub struct OwnProcess {
    name: String,
    env: BTreeMap<OsString, OsString>,
}

impl OwnProcess {
    /// The test `test` in `module`, the test module's `module_path!()`.
    pub fn new(module: &str, test: &str) -> Self {
        let name = match module.split_once("::") {
            Some((_, path)) => format!("{path}::{test}"),
            None => test.to_owned(),
        };
        Self {
            name,
            env: BTreeMap::new(),
        }
    }

    /// Sets `key` to `value` in the test's own process.
    pub fn env(&mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> &mut Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Whether this process is the test's own.
    pub fn is_current(&self) -> bool {
        std::env::var_os(OWN_TEST_VAR).is_some_and(|name| name == self.name.as_str())
    }

    /// The command that starts the test's own process: this binary, running that one test whether it is ignored or not, with its output left uncaptured.
    pub fn command(&self) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("the running test binary"));
        command
            .args([
                self.name.as_str(),
                "--exact",
                "--include-ignored",
                "--nocapture",
            ])
            .env(OWN_TEST_VAR, &self.name)
            .envs(&self.env);
        command
    }

    /// Runs `body` in the test's own process, and from anywhere else starts that process and asserts the test passed there.
    pub fn run(&self, body: impl FnOnce()) {
        if self.is_current() {
            body();
        } else {
            self.passes();
        }
    }

    /// Starts the test's own process and asserts it ran the test once and passed, with what the process printed when it did not.
    pub fn passes(&self) {
        let output = self
            .command()
            .output()
            .expect("start the test's own process");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("test result: ok. 1 passed;"),
            "`{}` did not pass once in its own process ({}):\n--- stdout\n{stdout}\n--- stderr\n{}",
            self.name,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;
