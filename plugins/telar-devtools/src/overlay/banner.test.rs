use super::*;

const REPORT: &str = "error[E0425]: cannot find value `count` in this scope\n --> src/app.rs:12:9\n  |\n12 |     count + 1\n  |     ^^^^^ not found in this scope\nerror: could not compile `app`";

#[test]
fn the_report_marks_what_the_compiler_points_at() {
    let (spans, lines) = diagnostics(REPORT);
    assert_eq!(lines, vec![1, 6], "each error heading is highlighted");
    let marked: Vec<&str> = spans
        .iter()
        .map(|span| &REPORT[span.range.clone()])
        .collect();
    assert_eq!(marked, vec!["error[E0425]", "src/app.rs:12:9", "error"]);
}
