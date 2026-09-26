//! Phase 8D — JUnit XML for a report, so that a CI system (GitHub Actions, Jenkins, GitLab, Azure DevOps ...) can show each case as a test
//! and fail a build on one that failed.
//!
//! Two reports have it: a scheduled regression run (`regression.rs`) and an OpenAPI test run (`openapi_runs.rs`). Each is turned into one
//! `<testsuite>` with a `<testcase>` for each case: `<failure>` for a case that did not pass, `<error>` for one that could not be run to
//! the end, `<skipped>` for one that was not run (or not repeated). Nothing is stored: it is made from the report, as the report is.

use serde_json::Value;

/// How a case came out, as JUnit has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    Error,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub classname: String,
    pub name: String,
    pub outcome: Outcome,
    /// Why it failed, could not run or was skipped.
    pub message: Option<String>,
    /// What else to know about it, shown as the case's output.
    pub output: Option<String>,
}

/// Text made safe for an XML attribute or element: the five characters XML reserves are escaped, and the control characters XML 1.0
/// cannot hold at all (a log with a stray escape code) are dropped.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// The whole document: one `<testsuite>` (inside `<testsuites>`, which some readers want) with its counts and properties.
pub fn suite_xml(
    suite: &str,
    timestamp: &str,
    properties: &[(&str, String)],
    cases: &[Case],
) -> String {
    let count = |r: Outcome| cases.iter().filter(|c| c.outcome == r).count();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuites name=\"{name}\" tests=\"{tests}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{skipped}\">\n",
        name = escape(suite),
        tests = cases.len(),
        failures = count(Outcome::Failed),
        errors = count(Outcome::Error),
        skipped = count(Outcome::Skipped),
    ));
    out.push_str(&format!(
        "  <testsuite name=\"{name}\" tests=\"{tests}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{skipped}\" time=\"0\" timestamp=\"{timestamp}\">\n",
        name = escape(suite),
        tests = cases.len(),
        failures = count(Outcome::Failed),
        errors = count(Outcome::Error),
        skipped = count(Outcome::Skipped),
        timestamp = escape(timestamp),
    ));
    if !properties.is_empty() {
        out.push_str("    <properties>\n");
        for (name, value) in properties {
            out.push_str(&format!(
                "      <property name=\"{}\" value=\"{}\"/>\n",
                escape(name),
                escape(value)
            ));
        }
        out.push_str("    </properties>\n");
    }
    for case in cases {
        out.push_str(&format!(
            "    <testcase classname=\"{}\" name=\"{}\" time=\"0\"",
            escape(&case.classname),
            escape(&case.name)
        ));
        let message = case.message.as_deref().unwrap_or("");
        let inner = match case.outcome {
            Outcome::Passed => None,
            Outcome::Failed => Some(format!(
                "<failure message=\"{m}\">{m}</failure>",
                m = escape(message)
            )),
            Outcome::Error => Some(format!(
                "<error message=\"{m}\">{m}</error>",
                m = escape(message)
            )),
            Outcome::Skipped => Some(format!("<skipped message=\"{}\"/>", escape(message))),
        };
        let output = case
            .output
            .as_deref()
            .map(|o| format!("<system-out>{}</system-out>", escape(o)));
        if inner.is_none() && output.is_none() {
            out.push_str("/>\n");
        } else {
            out.push_str(">\n");
            for part in inner.into_iter().chain(output) {
                out.push_str(&format!("      {part}\n"));
            }
            out.push_str("    </testcase>\n");
        }
    }
    out.push_str("  </testsuite>\n</testsuites>\n");
    out
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// How a case's outcome in a report is put: `PASSED`, `FAILED`, `ERROR`, and anything else (`PENDING`, `NOT_RUN`) as not run.
fn result_of(outcome: &str) -> Outcome {
    match outcome {
        "PASSED" => Outcome::Passed,
        "FAILED" => Outcome::Failed,
        "ERROR" => Outcome::Error,
        _ => Outcome::Skipped,
    }
}

/// The message of a case that is not a pass: its reason, else what it is.
fn message_of(outcome: &str, reason: &Value) -> Option<String> {
    let reason = reason.as_str().filter(|r| !r.is_empty());
    match outcome {
        "PASSED" => None,
        "PENDING" => Some(reason.unwrap_or("waiting to run").to_string()),
        "NOT_RUN" => Some(reason.unwrap_or("not run").to_string()),
        _ => Some(reason.unwrap_or("no reason was given").to_string()),
    }
}

/// A regression report (`regression::build_report`) as JUnit. A case's `output` says what became of it since the last time, so a
/// build page can show a regression as one; a case that was in scope and not repeated is a skipped test with the reason.
pub fn regression_junit(report: &Value) -> String {
    let schedule = text(&report["schedule_name"]);
    let classname = format!("regression.{schedule}");
    let mut cases: Vec<Case> = report["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            let outcome = text(&c["outcome"]);
            let before = c["before"]
                .as_str()
                .map(|b| {
                    format!(
                        "{} ({})",
                        b.to_lowercase(),
                        text(&c["before_from"]).to_lowercase().replace('_', " ")
                    )
                })
                .unwrap_or_else(|| "no earlier result".to_string());
            Case {
                classname: classname.clone(),
                name: text(&c["title"]),
                outcome: result_of(&outcome),
                message: message_of(&outcome, &c["reason"]),
                output: Some(format!("{}; before: {before}", text(&c["change"]))),
            }
        })
        .collect();
    cases.extend(
        report["skipped"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| Case {
                classname: classname.clone(),
                name: text(&c["title"]),
                outcome: Outcome::Skipped,
                message: Some(text(&c["reason"])),
                output: Some("not repeated".to_string()),
            }),
    );
    suite_xml(
        &format!("AIQA regression: {schedule}"),
        &text(&report["created_at"]),
        &[
            ("verdict", text(&report["verdict"])),
            ("run_id", text(&report["run_id"])),
            ("trigger", text(&report["trigger"])),
            ("regressed", report["summary"]["regressed"].to_string()),
            ("fixed", report["summary"]["fixed"].to_string()),
        ],
        &cases,
    )
}

/// An OpenAPI test report (`openapi_runs::build_report`) as JUnit: an operation of the document is the class, a case its test.
pub fn openapi_junit(report: &Value) -> String {
    let title = format!(
        "{} {}",
        text(&report["spec"]["title"]),
        text(&report["spec"]["version"])
    );
    let mut cases = Vec::new();
    for operation in report["operations"].as_array().into_iter().flatten() {
        for c in operation["cases"].as_array().into_iter().flatten() {
            let outcome = text(&c["outcome"]);
            let expected = c["expect"]["statuses"]
                .as_array()
                .map(|s| {
                    s.iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(" or ")
                })
                .unwrap_or_default();
            let got = c["actual"]["status"]
                .as_u64()
                .map(|s| format!("; got {s}"))
                .unwrap_or_default();
            cases.push(Case {
                classname: text(&operation["id"]),
                name: text(&c["title"]),
                outcome: result_of(&outcome),
                message: message_of(&outcome, &c["reason"]),
                output: Some(format!(
                    "{} {}; expected {expected}{got}",
                    text(&c["request"]["method"]),
                    text(&c["request"]["url"])
                )),
            });
        }
    }
    suite_xml(
        &format!("AIQA API tests: {}", title.trim()),
        &text(&report["created_at"]),
        &[
            ("verdict", text(&report["verdict"])),
            ("run_id", text(&report["run_id"])),
            ("base_url", text(&report["spec"]["base_url"])),
        ],
        &cases,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn case(outcome: Outcome, message: Option<&str>, output: Option<&str>) -> Case {
        Case {
            classname: "suite.class".to_string(),
            name: "a case".to_string(),
            outcome,
            message: message.map(str::to_string),
            output: output.map(str::to_string),
        }
    }

    #[test]
    fn the_five_reserved_characters_are_escaped_and_the_ones_xml_cannot_hold_are_dropped() {
        assert_eq!(
            escape("a & b < c > d \" e ' f"),
            "a &amp; b &lt; c &gt; d &quot; e &apos; f"
        );
        assert_eq!(
            escape("tab\tnew\nline\rreturn"),
            "tab\tnew\nline\rreturn",
            "white space XML allows stays"
        );
        assert_eq!(
            escape("red\u{1b}[31m text\u{0}\u{8}\u{b}\u{c}\u{1f}"),
            "red[31m text",
            "escape codes and other control characters go"
        );
        assert_eq!(escape("é 漢 🙂"), "é 漢 🙂", "text of any language stays");
        assert_eq!(escape("\u{fffe}x\u{ffff}"), "x");
    }

    #[test]
    fn a_suite_counts_its_cases_by_how_they_came_out_and_has_a_testcase_each() {
        let xml = suite_xml(
            "Nightly & co",
            "2026-09-26T02:00:00Z",
            &[("verdict", "REGRESSED".to_string())],
            &[
                case(Outcome::Passed, None, None),
                case(
                    Outcome::Failed,
                    Some("got 500 <bad>"),
                    Some("REGRESSED; before: passed"),
                ),
                case(Outcome::Error, Some("timed out"), None),
                case(Outcome::Skipped, Some("not run"), None),
                case(Outcome::Passed, None, Some("fine")),
            ],
        );
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites name=\"Nightly &amp; co\" tests=\"5\" failures=\"1\" errors=\"1\" skipped=\"1\">"), "{xml}");
        assert!(xml.contains("<testsuite name=\"Nightly &amp; co\" tests=\"5\" failures=\"1\" errors=\"1\" skipped=\"1\" time=\"0\" timestamp=\"2026-09-26T02:00:00Z\">"), "{xml}");
        assert!(
            xml.contains("<property name=\"verdict\" value=\"REGRESSED\"/>"),
            "{xml}"
        );
        assert_eq!(xml.matches("<testcase ").count(), 5);
        assert!(
            xml.contains("<testcase classname=\"suite.class\" name=\"a case\" time=\"0\"/>"),
            "a pass with nothing to say is a bare test case: {xml}"
        );
        assert!(
            xml.contains("<failure message=\"got 500 &lt;bad&gt;\">got 500 &lt;bad&gt;</failure>"),
            "{xml}"
        );
        assert!(
            xml.contains("<system-out>REGRESSED; before: passed</system-out>"),
            "{xml}"
        );
        assert!(
            xml.contains("<error message=\"timed out\">timed out</error>"),
            "{xml}"
        );
        assert!(xml.contains("<skipped message=\"not run\"/>"), "{xml}");
        assert!(xml.contains("<testcase classname=\"suite.class\" name=\"a case\" time=\"0\">\n      <system-out>fine</system-out>\n    </testcase>"), "{xml}");
        assert!(xml.ends_with("  </testsuite>\n</testsuites>\n"), "{xml}");
    }

    #[test]
    fn a_suite_with_no_case_and_no_property_is_still_a_document() {
        let xml = suite_xml("Empty", "t", &[], &[]);
        assert!(xml.contains("tests=\"0\" failures=\"0\" errors=\"0\" skipped=\"0\""));
        assert!(!xml.contains("<properties>") && !xml.contains("<testcase"));
    }

    fn regression_report() -> Value {
        json!({
            "schedule_name": "Nightly", "verdict": "REGRESSED", "run_id": "run-1", "trigger": "SCHEDULE", "created_at": "2026-09-26T02:00:00Z",
            "summary": {"regressed": 1, "fixed": 1},
            "cases": [
                {"title": "Login", "outcome": "FAILED", "reason": "got 500", "change": "REGRESSED", "before": "PASSED", "before_from": "PREVIOUS_RUN"},
                {"title": "Search", "outcome": "PASSED", "reason": null, "change": "FIXED", "before": "FAILED", "before_from": "SOURCE_RUN"},
                {"title": "Profile", "outcome": "ERROR", "reason": "", "change": "NEW", "before": null, "before_from": null},
                {"title": "Cart", "outcome": "PENDING", "reason": null, "change": "UNCOMPARED", "before": null, "before_from": null},
                {"title": "Pay", "outcome": "NOT_RUN", "reason": "the task ended cancelled", "change": "UNCOMPARED", "before": null, "before_from": null},
            ],
            "skipped": [{"case_id": "c9", "title": "Never run", "reason": "approved, but not run yet"}],
        })
    }

    #[test]
    fn a_regression_report_is_a_suite_with_a_test_for_each_case_and_each_case_not_repeated() {
        let xml = regression_junit(&regression_report());
        assert!(xml.contains("<testsuites name=\"AIQA regression: Nightly\" tests=\"6\" failures=\"1\" errors=\"1\" skipped=\"3\">"), "{xml}");
        assert!(
            xml.contains("<property name=\"verdict\" value=\"REGRESSED\"/>")
                && xml.contains("<property name=\"regressed\" value=\"1\"/>"),
            "{xml}"
        );
        assert!(
            xml.contains("<testcase classname=\"regression.Nightly\" name=\"Login\" time=\"0\">"),
            "{xml}"
        );
        assert!(
            xml.contains("<failure message=\"got 500\">got 500</failure>"),
            "{xml}"
        );
        assert!(
            xml.contains("<system-out>REGRESSED; before: passed (previous run)</system-out>"),
            "what became of it since last time: {xml}"
        );
        assert!(
            xml.contains("<system-out>FIXED; before: failed (source run)</system-out>"),
            "{xml}"
        );
        assert!(
            xml.contains("<error message=\"no reason was given\">"),
            "a case that could not run with no reason still says something: {xml}"
        );
        assert!(xml.contains("NEW; before: no earlier result"), "{xml}");
        assert!(
            xml.contains("<skipped message=\"waiting to run\"/>")
                && xml.contains("<skipped message=\"the task ended cancelled\"/>"),
            "{xml}"
        );
        assert!(
            xml.contains("<skipped message=\"approved, but not run yet\"/>")
                && xml.contains("<system-out>not repeated</system-out>"),
            "{xml}"
        );
    }

    #[test]
    fn an_openapi_report_is_a_suite_with_an_operation_as_the_class_and_a_case_as_the_test() {
        let report = json!({
            "spec": {"title": "Pets", "version": "1.0", "base_url": "https://api.test/v1"}, "verdict": "FAILING", "run_id": "run-2", "created_at": "2026-09-26T03:00:00Z",
            "operations": [
                {"id": "GET /pets", "cases": [
                    {"title": "GET /pets: a valid request", "outcome": "PASSED", "reason": null, "request": {"method": "GET", "url": "https://api.test/v1/pets"}, "expect": {"statuses": [200]}, "actual": null},
                    {"title": "GET /pets: no credential", "outcome": "FAILED", "reason": "Status is one of 401, 403: got 200 OK", "request": {"method": "GET", "url": "https://api.test/v1/pets"}, "expect": {"statuses": [401, 403]}, "actual": {"status": 200}},
                ]},
                {"id": "POST /pets", "cases": [
                    {"title": "POST /pets: a valid request", "outcome": "NOT_RUN", "reason": "POST changes data on the server", "request": {"method": "POST", "url": "https://api.test/v1/pets"}, "expect": {"statuses": [201]}, "actual": null},
                ]},
                {"id": "GET /old", "cases": []},
            ],
        });
        let xml = openapi_junit(&report);
        assert!(xml.contains("<testsuites name=\"AIQA API tests: Pets 1.0\" tests=\"3\" failures=\"1\" errors=\"0\" skipped=\"1\">"), "{xml}");
        assert!(
            xml.contains("<property name=\"base_url\" value=\"https://api.test/v1\"/>")
                && xml.contains("<property name=\"verdict\" value=\"FAILING\"/>"),
            "{xml}"
        );
        assert!(
            xml.contains(
                "<testcase classname=\"GET /pets\" name=\"GET /pets: no credential\" time=\"0\">"
            ),
            "{xml}"
        );
        assert!(
            xml.contains("<failure message=\"Status is one of 401, 403: got 200 OK\">"),
            "{xml}"
        );
        assert!(xml.contains("<system-out>GET https://api.test/v1/pets; expected 401 or 403; got 200</system-out>"), "{xml}");
        assert!(
            xml.contains("<skipped message=\"POST changes data on the server\"/>"),
            "{xml}"
        );
        assert!(
            xml.contains("<system-out>GET https://api.test/v1/pets; expected 200</system-out>"),
            "a case that passed has no answer to show: {xml}"
        );
    }
}
