//! `--report junit`: the run's `report.json` as the JUnit XML a CI shows test results from. One suite per preview, one case per preview and matrix cell, and one suite for what failed the run outside any preview.

use serde_json::Value;

/// The JUnit XML for `report`, the `report.json` a test run wrote, or why it does not read as one.
pub(super) fn junit_from_report(report: &Value) -> Result<String, String> {
    let test = report
        .get("test")
        .ok_or("the report has no `test` section; it was written by something other than `cargo telar test`")?;
    let mut suites = Vec::new();
    for preview in array(test, "previews") {
        let id = text(preview, "id");
        let cases: Vec<Case> = array(preview, "cells").iter().map(Case::of_cell).collect();
        suites.push(Suite { name: id, cases });
    }
    let errors = array(test, "errors");
    if !errors.is_empty() {
        let cases = errors
            .iter()
            .enumerate()
            .map(|(index, error)| Case {
                name: format!("error {}", index + 1),
                failure: Some(error.as_str().unwrap_or_default().to_string()),
            })
            .collect();
        suites.push(Suite {
            name: String::from("run"),
            cases,
        });
    }

    let tests: usize = suites.iter().map(|suite| suite.cases.len()).sum();
    let failures: usize = suites.iter().map(Suite::failures).sum();
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<testsuites name=\"telar previews\" tests=\"{tests}\" failures=\"{failures}\">\n"
    ));
    for suite in &suites {
        suite.write(&mut xml);
    }
    xml.push_str("</testsuites>\n");
    Ok(xml)
}

struct Suite {
    name: String,
    cases: Vec<Case>,
}

impl Suite {
    fn failures(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.failure.is_some())
            .count()
    }

    fn write(&self, xml: &mut String) {
        let name = escape(&self.name);
        xml.push_str(&format!(
            "  <testsuite name=\"{name}\" tests=\"{}\" failures=\"{}\">\n",
            self.cases.len(),
            self.failures()
        ));
        for case in &self.cases {
            let case_name = escape(&case.name);
            match &case.failure {
                None => xml.push_str(&format!(
                    "    <testcase classname=\"{name}\" name=\"{case_name}\"/>\n"
                )),
                Some(failure) => {
                    let message = escape(failure.lines().next().unwrap_or_default());
                    xml.push_str(&format!(
                        "    <testcase classname=\"{name}\" name=\"{case_name}\">\n      <failure message=\"{message}\">{}</failure>\n    </testcase>\n",
                        escape(failure)
                    ));
                }
            }
        }
        xml.push_str("  </testsuite>\n");
    }
}

struct Case {
    name: String,
    failure: Option<String>,
}

impl Case {
    fn of_cell(cell: &Value) -> Self {
        let failure =
            (cell.get("passed") != Some(&Value::Bool(true))).then(|| details(cell).join("\n"));
        Self {
            name: text(cell, "id"),
            failure,
        }
    }
}

/// What failed in `cell`, one line per check, as the run printed it.
fn details(cell: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    let failed = |check: &str| cell[check]["status"] == "failed";
    for check in ["build", "play", "a11y"] {
        if let Some(error) = cell[check]["error"].as_str() {
            lines.push(format!("{check}: {error}"));
        }
    }
    if failed("a11y") {
        lines.extend(
            array(&cell["a11y"], "violations")
                .iter()
                .filter(|finding| finding["severity"] == "error")
                .map(|finding| {
                    format!(
                        "a11y: {}  {}",
                        finding["rule"].as_str().unwrap_or_default(),
                        finding["message"].as_str().unwrap_or_default()
                    )
                }),
        );
    }
    if failed("snapshot")
        && let Some(detail) = cell["snapshot"]["detail"].as_str()
    {
        lines.push(format!("snapshot: {detail}"));
    }
    if lines.is_empty() {
        lines.push(String::from("failed"));
    }
    lines
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\n' | '\t' => escaped.push(c),
            c if (c as u32) < 0x20 => {}
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
#[path = "junit_test.rs"]
mod tests;
