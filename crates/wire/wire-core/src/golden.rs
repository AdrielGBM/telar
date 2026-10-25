use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::WIRE_VERSION;

const RECORD_VAR: &str = "TELAR_RECORD_WIRE_GOLDEN";
const VERSION_KEY: &str = "wire-version";
const BYTES_PER_LINE: usize = 32;

/// Checks `bytes` against the golden file at `path`, which pins them to [`WIRE_VERSION`]; with `TELAR_RECORD_WIRE_GOLDEN` set it rewrites the file instead. Prefer [`assert_golden!`](crate::assert_golden).
pub fn check_golden(path: &Path, bytes: &[u8]) {
    let recorded = render(bytes);
    if std::env::var_os(RECORD_VAR).is_some() {
        fs::write(path, &recorded).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
        return;
    }
    let expected = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "no golden at {}: {e}\nrecord it with {RECORD_VAR}=1\n{recorded}",
            path.display()
        )
    });
    let (version, expected_hex) = parse(&expected);
    assert_eq!(
        version,
        Some(WIRE_VERSION),
        "{} pins another WIRE_VERSION than the current {WIRE_VERSION}: re-record it with {RECORD_VAR}=1",
        path.display()
    );
    assert!(
        expected_hex == parse(&recorded).1,
        "the bytes of {} changed while WIRE_VERSION stayed at {WIRE_VERSION}: bump WIRE_VERSION in telar-wire-core, then re-record with {RECORD_VAR}=1\nnew bytes:\n{recorded}",
        path.display()
    );
}

fn render(bytes: &[u8]) -> String {
    let mut out = format!("{VERSION_KEY} {WIRE_VERSION}\n");
    for line in bytes.chunks(BYTES_PER_LINE) {
        for byte in line {
            write!(out, "{byte:02x}").expect("writing to a String");
        }
        out.push('\n');
    }
    out
}

fn parse(text: &str) -> (Option<u16>, String) {
    let mut lines = text.lines();
    let version = lines
        .next()
        .and_then(|line| line.strip_prefix(VERSION_KEY))
        .and_then(|rest| rest.trim().parse().ok());
    (version, lines.flat_map(str::split_whitespace).collect())
}

/// Asserts that `$bytes` equal the golden file at `$path`, relative to the calling crate's manifest directory.
#[macro_export]
macro_rules! assert_golden {
    ($path:literal, $bytes:expr) => {
        $crate::check_golden(
            &::std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join($path),
            $bytes,
        )
    };
}
