use serde_json::json;

use super::*;

fn report() -> Value {
    json!({
        "version": 1,
        "test": {
            "errors": ["preview id `demo--a` names 2 previews (a.rs:1, b.rs:2); rename all but one"],
            "previews": [{
                "id": "demo--card--a",
                "cells": [
                    {
                        "id": "demo--card--a",
                        "passed": true,
                        "a11y": { "status": "warned", "violations": [{ "rule": "missing-lang", "severity": "warning", "message": "no language" }] }
                    },
                    {
                        "id": "demo--card--a--mode=dark",
                        "passed": false,
                        "build": { "status": "passed", "error": null },
                        "play": { "status": "failed", "error": "found no button \"Save\" <here>" },
                        "a11y": { "status": "failed", "error": null, "violations": [
                            { "rule": "contrast", "severity": "error", "message": "text at 2.1:1" },
                            { "rule": "target-size", "severity": "warning", "message": "small" }
                        ] },
                        "snapshot": { "status": "failed", "detail": "the draw text changed" }
                    }
                ]
            }]
        }
    })
}

#[test]
fn each_preview_is_a_suite_and_each_cell_a_case() {
    let xml = junit_from_report(&report()).unwrap();
    assert!(
        xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
        "{xml}"
    );
    assert!(
        xml.contains("<testsuites name=\"telar previews\" tests=\"3\" failures=\"2\">"),
        "{xml}"
    );
    assert!(
        xml.contains("<testsuite name=\"demo--card--a\" tests=\"2\" failures=\"1\">"),
        "{xml}"
    );
    assert!(
        xml.contains("<testcase classname=\"demo--card--a\" name=\"demo--card--a\"/>"),
        "{xml}"
    );
}

#[test]
fn a_failed_case_says_each_check_that_failed_escaped() {
    let xml = junit_from_report(&report()).unwrap();
    assert!(
        xml.contains("<failure message=\"play: found no button &quot;Save&quot; &lt;here&gt;\">"),
        "{xml}"
    );
    assert!(xml.contains("a11y: contrast  text at 2.1:1"), "{xml}");
    assert!(
        !xml.contains("target-size"),
        "a warning fails nothing: {xml}"
    );
    assert!(xml.contains("snapshot: the draw text changed"), "{xml}");
}

#[test]
fn what_failed_the_run_outside_a_preview_is_a_suite_of_its_own() {
    let xml = junit_from_report(&report()).unwrap();
    assert!(
        xml.contains("<testsuite name=\"run\" tests=\"1\" failures=\"1\">"),
        "{xml}"
    );
    assert!(xml.contains("name=\"error 1\""), "{xml}");
}

#[test]
fn a_report_without_a_test_section_is_refused() {
    let error = junit_from_report(&json!({ "version": 1, "snapshots": [] })).unwrap_err();
    assert!(error.contains("`test`"), "{error}");
}
