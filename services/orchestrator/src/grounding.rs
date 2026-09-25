//! Phase 5 — execution grounding.
//!
//! A browser test case is compiled from prose, and prose names no selector: an AI asked to "click Settings" has to
//! invent one, and a plan built on an invented selector fails at run time for reasons that have nothing to do with the
//! application (`execution_plan::compile`'s own doc comment says so). A crawl of the real application
//! (`crawler.rs`, `helpers/crawler.ts`) has seen the real pages and elements, and records — next to its result — the
//! grounding this module reads:
//!
//! ```text
//! crawl-grounding-<task id>.json   (written by helpers/crawler.ts, one per crawl)
//!         v
//! read_crawl_grounding()           -- per role: the pages it reached, each with the elements it saw, every one with a
//!                                     locator the crawl checked matches exactly one element; and the role's login
//!                                     steps, with any credential replaced by an `env:NAME` reference
//!         v
//! Grounding::for_case()            -- the role the case runs as, its login steps, and — bounded, and chosen by how
//!                                     much each page and element has to do with this case — what the compiler is shown
//!         v
//! prompt_block()                   -- what the AI is given, and told to copy locators from
//!         v
//! ungrounded()                     -- what an AI plan may not do: a selector the crawl never saw is not compiled, it
//!                                     is `NOT_EXECUTABLE`, whatever the AI claimed
//! ```
//!
//! Credentials are never part of this: a role's login steps carry `env:NAME` references (`execution_plan::secret_name`),
//! read from the orchestrator's environment when the test runs. A crawl made with literal credentials records none, and
//! a case cannot be grounded on such a role — the reason says how to crawl it again.

use crate::execution_plan::{secret_name, Action};
use crate::store::TestCaseRow;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// -------------------------------------------------- what the crawl records

/// One element the crawl saw, with a locator (a Playwright selector string) that it checked matches exactly one element
/// on that page.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedElement {
    /// `link`, `button`, `input`, `select` or `textarea`.
    pub kind: String,
    /// What a person would call it: a link's or button's text, a field's label, placeholder or name.
    pub text: String,
    pub locator: String,
    #[serde(default)]
    pub href: Option<String>,
    /// For an `input`: its `type` (`text`, `password`, `checkbox`, ...).
    #[serde(default)]
    pub input_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedPage {
    pub url: String,
    pub title: String,
    pub elements: Vec<GroundedElement>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedRole {
    pub role: String,
    pub login_succeeded: bool,
    #[serde(default)]
    pub login_error: Option<String>,
    /// True when the login was given with literal credentials, which the crawl does not keep: `login_actions` then has
    /// no usable values.
    #[serde(default)]
    pub login_uses_literals: bool,
    /// The role's login steps as they were given, with every `env:NAME` reference kept and every literal value removed.
    #[serde(default)]
    pub login_actions: Vec<Action>,
    pub pages: Vec<GroundedPage>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlGrounding {
    pub roles: Vec<GroundedRole>,
}

/// Reads back the grounding a finished crawl wrote (`helpers/crawler.ts::runCrawl`), keyed like its result by the crawl
/// task's id. An `Err` — no such file: the crawl is still running, or was made before grounding was recorded — is
/// reported as it is, never replaced by an empty grounding.
pub fn read_crawl_grounding(task_id: &str) -> Result<CrawlGrounding, String> {
    let safe_id = crate::crawler::safe_id_from(task_id)?;
    let path = crate::crawler::generated_dir().join(crate::crawler::grounding_file_name(&safe_id));
    let content = std::fs::read_to_string(&path).map_err(|_| {
        "this crawl recorded no grounding — it is still running, or it was made before grounding was recorded: \
         run the crawl again"
            .to_string()
    })?;
    serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse the crawl's grounding: {e}"))
}

// ---------------------------------------------- what a compile is given

/// The most pages, and the most elements of a page, the compiler is shown, and the most characters they may take: a
/// local model's window can be 4,096 tokens, and the case and the rules need most of it.
pub const MAX_PAGES_OFFERED: usize = 4;
pub const MAX_ELEMENTS_PER_PAGE: usize = 25;
pub const MAX_OFFERED_CHARS: usize = 3_500;
/// The most reasons one refusal lists.
const MAX_REASONS: usize = 8;

/// What a case is compiled against: the role it runs as, the steps that sign that role in, and what the crawl saw.
#[derive(Debug, Clone)]
pub struct Grounding {
    /// The role the case runs as; `None` for the crawl's role that never logged in.
    pub role: Option<String>,
    /// Signs `role` in, run before the case's own actions: NAVIGATE / FILL / CLICK, credentials as `env:NAME`.
    pub login: Vec<Action>,
    /// What the compiler is shown: the pages of the role's crawl most to do with the case, and on each the elements.
    pub pages: Vec<GroundedPage>,
    /// Every element of the role's crawl by locator — what a compiled plan may use.
    pub known: BTreeMap<String, GroundedElement>,
    pub pages_in_crawl: usize,
}

/// What the compile response says the compiler was given (names only where secrets are concerned).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroundingReport {
    pub role: Option<String>,
    pub pages_in_crawl: usize,
    pub pages_offered: usize,
    pub elements_offered: usize,
    pub login_steps: usize,
    /// The environment variables the login reads when the test runs.
    pub secrets: Vec<String>,
}

const STOP_WORDS: [&str; 24] = [
    "the", "and", "for", "with", "that", "this", "from", "into", "then", "when", "should", "shall",
    "will", "can", "not", "are", "has", "have", "page", "click", "user", "test", "case", "verify",
];

/// The lower-case words of `text` worth matching on: letters and digits, three or more, and not a filler word.
fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 3 && !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

fn case_words(case: &TestCaseRow) -> BTreeSet<String> {
    let mut text = format!(
        "{} {} {}",
        case.title, case.description, case.expected_result
    );
    for step in &case.steps {
        text.push(' ');
        text.push_str(&step.action);
        text.push(' ');
        text.push_str(&step.expected_result);
    }
    for assertion in &case.assertions {
        text.push(' ');
        text.push_str(assertion);
    }
    words(&text)
}

/// How much an element has to do with the case: words of the case in its label count twice, in its locator once.
fn element_score(element: &GroundedElement, wanted: &BTreeSet<String>) -> usize {
    let label = words(&element.text);
    let locator = words(&element.locator);
    wanted.iter().filter(|w| label.contains(*w)).count() * 2
        + wanted.iter().filter(|w| locator.contains(*w)).count()
}

/// How much a page has to do with the case: its best elements, and its address and title, weighted.
fn page_score(page: &GroundedPage, wanted: &BTreeSet<String>) -> usize {
    let mut element_scores: Vec<usize> = page
        .elements
        .iter()
        .map(|e| element_score(e, wanted))
        .collect();
    element_scores.sort_unstable_by(|a, b| b.cmp(a));
    let best: usize = element_scores.iter().take(3).sum();
    let about = words(&format!("{} {}", page.title, page.url));
    best + wanted.iter().filter(|w| about.contains(*w)).count() * 3
}

fn element_line(element: &GroundedElement) -> String {
    let what = match element.input_type.as_deref() {
        Some(kind) if element.kind == "input" => format!("input ({kind})"),
        _ => element.kind.clone(),
    };
    format!("- {what} {:?} -> {}\n", element.text, element.locator)
}

impl Grounding {
    /// The grounding for `case` from `crawl`, run as `role` (`None` = the crawl's role that never logged in).
    /// The `Err` says why the case cannot be grounded on that role and what to do about it.
    pub fn for_case(
        crawl: &CrawlGrounding,
        role: Option<&str>,
        case: &TestCaseRow,
    ) -> Result<Grounding, String> {
        let names = || {
            crawl
                .roles
                .iter()
                .map(|r| r.role.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let chosen = match role.map(str::trim).filter(|r| !r.is_empty()) {
            Some(name) => crawl
                .roles
                .iter()
                .find(|r| r.role.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("the crawl has no role {name:?} (its roles: {})", names()))?,
            None => crawl
                .roles
                .iter()
                .find(|r| r.login_actions.is_empty() && !r.login_uses_literals)
                .ok_or_else(|| {
                    format!(
                        "the crawl has no role that did not log in; choose the role the case runs as (its roles: {})",
                        names()
                    )
                })?,
        };

        if !chosen.login_succeeded {
            return Err(format!(
                "the crawl could not log in as role {:?}{} — run it again once the login works",
                chosen.role,
                chosen
                    .login_error
                    .as_deref()
                    .map(|e| format!(" ({e})"))
                    .unwrap_or_default()
            ));
        }
        if chosen.login_uses_literals {
            return Err(format!(
                "role {:?} was crawled with a literal password, which is never kept: run the crawl again with the \
                 login values written as env:NAME references (for example env:AIQA_LOGIN_ADMIN_PASSWORD), so a test \
                 can sign in without a password being stored",
                chosen.role
            ));
        }
        // What the file holds is what a person could have edited: a login step that types a literal, or anything but
        // NAVIGATE / FILL / CLICK, is refused rather than run.
        for action in &chosen.login_actions {
            let sound = match action {
                Action::Navigate { .. } | Action::Click { .. } => true,
                Action::Fill { value, .. } => secret_name(value).is_some(),
                _ => false,
            };
            if !sound {
                return Err(format!(
                    "the login recorded for role {:?} is not usable: every FILL must be an env:NAME reference and the \
                     steps NAVIGATE, FILL or CLICK — run the crawl again",
                    chosen.role
                ));
            }
        }
        if chosen.pages.iter().all(|p| p.elements.is_empty()) {
            return Err(format!(
                "the crawl found no page element as role {:?}, so there is nothing to ground a test on",
                chosen.role
            ));
        }

        let wanted = case_words(case);
        let mut known: BTreeMap<String, GroundedElement> = BTreeMap::new();
        for page in &chosen.pages {
            for element in &page.elements {
                known
                    .entry(element.locator.clone())
                    .or_insert_with(|| element.clone());
            }
        }

        // The pages most to do with the case first (a page nothing on it matches keeps its crawl order after them).
        let mut ranked: Vec<(usize, usize, &GroundedPage)> = chosen
            .pages
            .iter()
            .filter(|p| !p.elements.is_empty())
            .enumerate()
            .map(|(order, page)| (page_score(page, &wanted), order, page))
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

        let mut budget = MAX_OFFERED_CHARS;
        let mut pages: Vec<GroundedPage> = Vec::new();
        for (_, _, page) in ranked.into_iter().take(MAX_PAGES_OFFERED) {
            let header = 24 + page.title.len() + page.url.len();
            if budget <= header {
                break;
            }
            budget -= header;
            let mut elements: Vec<(usize, usize, &GroundedElement)> = page
                .elements
                .iter()
                .enumerate()
                .map(|(order, e)| (element_score(e, &wanted), order, e))
                .collect();
            elements.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let mut shown = Vec::new();
            for (_, _, element) in elements.into_iter().take(MAX_ELEMENTS_PER_PAGE) {
                let cost = element_line(element).len();
                if budget < cost {
                    break;
                }
                budget -= cost;
                shown.push(element.clone());
            }
            if !shown.is_empty() {
                pages.push(GroundedPage {
                    url: page.url.clone(),
                    title: page.title.clone(),
                    elements: shown,
                });
            }
        }

        Ok(Grounding {
            role: (!chosen.login_actions.is_empty()).then(|| chosen.role.clone()),
            login: chosen.login_actions.clone(),
            pages,
            known,
            pages_in_crawl: chosen.pages.len(),
        })
    }

    pub fn report(&self) -> GroundingReport {
        GroundingReport {
            role: self.role.clone(),
            pages_in_crawl: self.pages_in_crawl,
            pages_offered: self.pages.len(),
            elements_offered: self.pages.iter().map(|p| p.elements.len()).sum(),
            login_steps: self.login.len(),
            secrets: crate::execution_plan::secret_names_of(&self.login),
        }
    }
}

// ----------------------------------------------------------------- prompt

/// What the compiler is shown and told: the pages and locators of the crawl most to do with the case, whether the
/// system signs in first, and the rule that a selector must be copied from the list.
pub fn prompt_block(grounding: &Grounding) -> String {
    let mut block = String::from(
        "GROUNDED APPLICATION (a live crawl of the real application; each locator below was checked to match \
         exactly one element on its page):\n",
    );
    for page in &grounding.pages {
        block.push_str(&format!("Page {:?} ({})\n", page.title, page.url));
        for element in &page.elements {
            block.push_str(&element_line(element));
        }
    }
    match &grounding.role {
        Some(role) => block.push_str(&format!(
            "LOGIN: the system signs in as the role {role:?} before your first action. Do not write login steps: \
             start from the signed-in state.\n"
        )),
        None => block.push_str("LOGIN: the case runs without signing in.\n"),
    }
    block.push_str(
        "GROUNDING RULES (critical): use ONLY a locator copied exactly from the list above as the \"selector\" of \
         CLICK, FILL, ASSERT_VISIBLE and ASSERT_TEXT. If the case needs an element that is not in the list, do not \
         guess one: return {\"executable\": false, \"reasons\": [...]} naming what is missing. Never write a \
         password or another credential as a FILL value.\n\n",
    );
    block
}

// ------------------------------------------------------------ the check

/// Why a plan cannot be trusted to run on the real application: every CLICK / FILL / ASSERT_VISIBLE / ASSERT_TEXT whose
/// selector the crawl never saw. An empty list means every selector is one the crawl found.
pub fn ungrounded(actions: &[Action], grounding: &Grounding) -> Vec<String> {
    let mut reasons: Vec<String> = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            let (kind, selector) = match action {
                Action::Click { selector } => ("CLICK", selector),
                Action::Fill { selector, .. } => ("FILL", selector),
                Action::AssertVisible { selector } => ("ASSERT_VISIBLE", selector),
                Action::AssertText { selector, .. } => ("ASSERT_TEXT", selector),
                _ => return None,
            };
            (!grounding.known.contains_key(selector.as_str())).then(|| {
                format!(
                    "step {} ({kind}) uses the selector {selector:?}, which the crawl never found on any page",
                    index + 1
                )
            })
        })
        .collect();
    if reasons.len() > MAX_REASONS {
        let more = reasons.len() - MAX_REASONS;
        reasons.truncate(MAX_REASONS);
        reasons.push(format!("and {more} more"));
    }
    reasons
}

/// A note for each step that types a literal value into a field the crawl saw is a password: fine for a test of a wrong
/// password, but a real credential belongs in an `env:NAME` login, not in a case.
pub fn literal_password_notes(actions: &[Action], grounding: &Grounding) -> Vec<String> {
    actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            let Action::Fill { selector, value } = action else {
                return None;
            };
            let is_password = grounding
                .known
                .get(selector.as_str())
                .is_some_and(|e| e.input_type.as_deref() == Some("password"));
            (is_password && !value.is_empty() && secret_name(value).is_none()).then(|| {
                format!(
                    "step {} types a literal value into a password field — right for a test of a wrong password; a \
                     real credential is not written into a case",
                    index + 1
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_plan::test_support::case;

    fn element(kind: &str, text: &str, locator: &str) -> GroundedElement {
        GroundedElement {
            kind: kind.to_string(),
            text: text.to_string(),
            locator: locator.to_string(),
            href: None,
            input_type: None,
        }
    }

    fn page(url: &str, title: &str, elements: Vec<GroundedElement>) -> GroundedPage {
        GroundedPage {
            url: url.to_string(),
            title: title.to_string(),
            elements,
        }
    }

    fn login() -> Vec<Action> {
        vec![
            Action::Navigate {
                url: "http://127.0.0.1:8080/login".to_string(),
            },
            Action::Fill {
                selector: "#username".to_string(),
                value: "env:AIQA_LOGIN_ADMIN_USERNAME".to_string(),
            },
            Action::Fill {
                selector: "#password".to_string(),
                value: "env:AIQA_LOGIN_ADMIN_PASSWORD".to_string(),
            },
            Action::Click {
                selector: "#submit-login".to_string(),
            },
        ]
    }

    fn role(name: &str, login_actions: Vec<Action>, pages: Vec<GroundedPage>) -> GroundedRole {
        GroundedRole {
            role: name.to_string(),
            login_succeeded: true,
            login_error: None,
            login_uses_literals: false,
            login_actions,
            pages,
        }
    }

    fn dashboard() -> GroundedPage {
        page(
            "http://127.0.0.1:8080/dashboard",
            "Dashboard",
            vec![
                element("link", "Dashboard", "a[href=\"/dashboard\"]"),
                element("link", "Settings", "a[href=\"/settings\"]"),
                element("link", "Manage Users", "a[href=\"/users\"]"),
            ],
        )
    }

    fn settings() -> GroundedPage {
        page(
            "http://127.0.0.1:8080/settings",
            "Settings",
            vec![element("button", "Delete Account", "#danger-zone")],
        )
    }

    fn login_page() -> GroundedPage {
        page(
            "http://127.0.0.1:8080/login",
            "Log in",
            vec![
                GroundedElement {
                    input_type: Some("text".to_string()),
                    ..element("input", "Username", "#username")
                },
                GroundedElement {
                    input_type: Some("password".to_string()),
                    ..element("input", "Password", "#password")
                },
                element("button", "Log in", "#submit-login"),
            ],
        )
    }

    fn crawl() -> CrawlGrounding {
        CrawlGrounding {
            roles: vec![
                role("admin", login(), vec![dashboard(), settings()]),
                role("anonymous", vec![], vec![login_page()]),
            ],
        }
    }

    fn open_settings_case() -> TestCaseRow {
        case(
            "Admin opens Settings",
            "Functional",
            vec![("Click the Settings link", "The Settings page is shown")],
            "Settings page is shown",
            vec![],
        )
    }

    // ------------------------------------------------------------- the file

    #[test]
    fn the_grounding_file_the_crawler_writes_parses_as_it_is_written() {
        let json = r##"{
          "roles": [{
            "role": "admin", "loginSucceeded": true, "loginUsesLiterals": false,
            "loginActions": [
              {"type": "NAVIGATE", "url": "http://x/login"},
              {"type": "FILL", "selector": "#password", "value": "env:AIQA_LOGIN_ADMIN_PASSWORD"},
              {"type": "CLICK", "selector": "#go"}
            ],
            "pages": [{"url": "http://x/d", "title": "D", "elements": [
              {"kind": "link", "text": "Settings", "locator": "a[href=\"/settings\"]", "href": "http://x/settings"},
              {"kind": "input", "text": "Password", "locator": "#password", "inputType": "password"}
            ]}]
          }]
        }"##;
        let parsed: CrawlGrounding = serde_json::from_str(json).expect("parse");
        let admin = &parsed.roles[0];
        assert_eq!(admin.login_actions.len(), 3);
        assert_eq!(
            admin.pages[0].elements[1].input_type.as_deref(),
            Some("password")
        );
        assert_eq!(
            admin.pages[0].elements[0].href.as_deref(),
            Some("http://x/settings")
        );
    }

    #[test]
    fn a_crawl_with_no_grounding_file_is_reported_as_such_never_as_an_empty_grounding() {
        let err = read_crawl_grounding("task-that-never-crawled").expect_err("no file");
        assert!(err.contains("recorded no grounding"), "err={err}");
        assert!(err.contains("run the crawl again"), "err={err}");
    }

    // -------------------------------------------------------- choosing a role

    #[test]
    fn a_case_is_grounded_on_the_role_it_runs_as_with_that_roles_login_and_pages() {
        let g =
            Grounding::for_case(&crawl(), Some("Admin"), &open_settings_case()).expect("grounded");
        assert_eq!(
            g.role.as_deref(),
            Some("admin"),
            "role names match without regard to case"
        );
        assert_eq!(g.login.len(), 4);
        assert_eq!(g.pages_in_crawl, 2);
        // What that role's crawl found — and nothing of another role's.
        assert!(g.known.contains_key("a[href=\"/settings\"]"));
        assert!(g.known.contains_key("#danger-zone"));
        assert!(
            !g.known.contains_key("#username"),
            "the login page belongs to the anonymous role"
        );
        assert_eq!(
            g.report().secrets,
            vec![
                "AIQA_LOGIN_ADMIN_PASSWORD".to_string(),
                "AIQA_LOGIN_ADMIN_USERNAME".to_string()
            ]
        );
    }

    #[test]
    fn with_no_role_the_case_runs_as_the_role_that_never_logged_in() {
        let g = Grounding::for_case(&crawl(), None, &open_settings_case()).expect("grounded");
        assert_eq!(g.role, None);
        assert!(g.login.is_empty());
        assert!(g.known.contains_key("#username"));
        assert!(!g.known.contains_key("#danger-zone"));
        assert_eq!(g.report().login_steps, 0);
    }

    #[test]
    fn an_unknown_role_is_refused_and_the_roles_the_crawl_has_are_named() {
        let err = Grounding::for_case(&crawl(), Some("auditor"), &open_settings_case())
            .expect_err("no such role");
        assert!(err.contains("no role \"auditor\""), "err={err}");
        assert!(err.contains("admin, anonymous"), "err={err}");
    }

    #[test]
    fn a_role_that_could_not_log_in_cannot_ground_a_case() {
        let mut c = crawl();
        c.roles[0].login_succeeded = false;
        c.roles[0].login_error = Some("selector never resolved".to_string());
        let err =
            Grounding::for_case(&c, Some("admin"), &open_settings_case()).expect_err("no login");
        assert!(
            err.contains("could not log in as role \"admin\""),
            "err={err}"
        );
        assert!(err.contains("selector never resolved"), "err={err}");
    }

    #[test]
    fn a_role_crawled_with_a_literal_password_says_how_to_crawl_it_again() {
        let mut c = crawl();
        c.roles[0].login_uses_literals = true;
        let err =
            Grounding::for_case(&c, Some("admin"), &open_settings_case()).expect_err("literal");
        assert!(err.contains("env:NAME"), "err={err}");
        assert!(err.contains("run the crawl again"), "err={err}");
    }

    #[test]
    fn a_login_that_types_a_literal_is_refused_even_if_the_file_says_it_is_fine() {
        let mut c = crawl();
        c.roles[0].login_actions[2] = Action::Fill {
            selector: "#password".to_string(),
            value: "hunter2".to_string(),
        };
        let err =
            Grounding::for_case(&c, Some("admin"), &open_settings_case()).expect_err("literal");
        assert!(err.contains("not usable"), "err={err}");
        assert!(
            !err.contains("hunter2"),
            "the reason never repeats a value: {err}"
        );
    }

    #[test]
    fn a_login_with_any_action_but_navigate_fill_or_click_is_refused() {
        let mut c = crawl();
        c.roles[0].login_actions.push(Action::AssertVisible {
            selector: "#x".to_string(),
        });
        let err =
            Grounding::for_case(&c, Some("admin"), &open_settings_case()).expect_err("refused");
        assert!(err.contains("not usable"), "err={err}");
    }

    #[test]
    fn a_role_that_reached_no_element_leaves_nothing_to_ground_on() {
        let mut c = crawl();
        c.roles[0].pages = vec![page("http://x/", "Empty", vec![])];
        let err = Grounding::for_case(&c, Some("admin"), &open_settings_case()).expect_err("empty");
        assert!(err.contains("nothing to ground"), "err={err}");
    }

    // ------------------------------------------------------- what is offered

    #[test]
    fn the_page_a_case_is_about_is_offered_first_even_when_it_was_crawled_last() {
        let filler = |i: usize| {
            page(
                &format!("http://127.0.0.1:8080/misc/{i}"),
                &format!("Misc {i}"),
                vec![element(
                    "link",
                    &format!("Unrelated {i}"),
                    &format!("a[href=\"/misc/{i}\"]"),
                )],
            )
        };
        let invoices = page(
            "http://127.0.0.1:8080/billing/invoices",
            "Invoices",
            vec![
                element("link", "Home", "a[href=\"/\"]"),
                element("button", "Refund", "#refund"),
                element("button", "Download invoice", "#download"),
            ],
        );
        let mut pages: Vec<GroundedPage> = (0..6).map(filler).collect();
        pages.push(invoices);
        let crawl = CrawlGrounding {
            roles: vec![role("anonymous", vec![], pages)],
        };
        let invoice_case = case(
            "Download an invoice",
            "Functional",
            vec![("Click Download invoice", "The invoice is downloaded")],
            "invoice downloaded",
            vec![],
        );
        let g = Grounding::for_case(&crawl, None, &invoice_case).expect("grounded");
        assert_eq!(
            g.pages[0].title,
            "Invoices",
            "pages: {:?}",
            g.pages.iter().map(|p| &p.title).collect::<Vec<_>>()
        );
        assert_eq!(
            g.pages[0].elements[0].text, "Download invoice",
            "and its best-matching element leads it"
        );
        assert!(g.pages.len() <= MAX_PAGES_OFFERED);
    }

    #[test]
    fn on_a_page_the_elements_that_match_the_case_lead() {
        let g =
            Grounding::for_case(&crawl(), Some("admin"), &open_settings_case()).expect("grounded");
        let dashboard = g
            .pages
            .iter()
            .find(|p| p.title == "Dashboard")
            .expect("dashboard offered");
        assert_eq!(
            dashboard.elements[0].text, "Settings",
            "the Settings link leads the dashboard"
        );
        assert_eq!(
            dashboard.elements.len(),
            3,
            "and nothing on it is dropped while there is room"
        );
    }

    #[test]
    fn what_is_offered_is_bounded_however_large_the_crawl_is() {
        let many: Vec<GroundedElement> = (0..500)
            .map(|i| {
                element(
                    "link",
                    &format!("Report number {i} with a long descriptive label"),
                    &format!("a[href=\"/r/{i}\"]"),
                )
            })
            .collect();
        let pages: Vec<GroundedPage> = (0..40)
            .map(|i| {
                page(
                    &format!("http://127.0.0.1:8080/section/{i}"),
                    &format!("Section {i}"),
                    many.clone(),
                )
            })
            .collect();
        let crawl = CrawlGrounding {
            roles: vec![role("anonymous", vec![], pages)],
        };
        let g = Grounding::for_case(&crawl, None, &open_settings_case()).expect("grounded");
        assert!(g.pages.len() <= MAX_PAGES_OFFERED);
        assert!(g
            .pages
            .iter()
            .all(|p| p.elements.len() <= MAX_ELEMENTS_PER_PAGE));
        let block = prompt_block(&g);
        assert!(
            block.len() <= MAX_OFFERED_CHARS + 1_200,
            "the whole block, rules included, stays small enough for a 4,096-token window: {} chars",
            block.len()
        );
        // Everything the crawl found is still what a plan may use, offered or not.
        assert_eq!(g.known.len(), 500);
    }

    #[test]
    fn a_case_that_matches_nothing_is_offered_the_first_pages_in_crawl_order() {
        let odd = case(
            "Zzz qqq",
            "Functional",
            vec![("Do zzz", "qqq")],
            "qqq",
            vec![],
        );
        let g = Grounding::for_case(&crawl(), Some("admin"), &odd).expect("grounded");
        assert_eq!(g.pages[0].title, "Dashboard");
    }

    // ---------------------------------------------------------------- prompt

    #[test]
    fn the_prompt_lists_real_locators_says_who_signs_in_and_forbids_guessing() {
        let g =
            Grounding::for_case(&crawl(), Some("admin"), &open_settings_case()).expect("grounded");
        let block = prompt_block(&g);
        assert!(
            block.contains("Page \"Settings\" (http://127.0.0.1:8080/settings)"),
            "{block}"
        );
        assert!(
            block.contains("- button \"Delete Account\" -> #danger-zone"),
            "{block}"
        );
        assert!(
            block.contains("- link \"Settings\" -> a[href=\"/settings\"]"),
            "{block}"
        );
        assert!(block.contains("signs in as the role \"admin\""), "{block}");
        assert!(block.contains("ONLY a locator copied exactly"), "{block}");
        assert!(
            !block.contains("AIQA_LOGIN_"),
            "no secret name — let alone value — is shown to a model: {block}"
        );
        let anonymous =
            prompt_block(&Grounding::for_case(&crawl(), None, &open_settings_case()).unwrap());
        assert!(anonymous.contains("runs without signing in"), "{anonymous}");
        assert!(
            anonymous.contains("- input (password) \"Password\" -> #password"),
            "{anonymous}"
        );
    }

    // ------------------------------------------------------------- the check

    #[test]
    fn a_plan_whose_selectors_are_all_from_the_crawl_has_nothing_ungrounded() {
        let g = Grounding::for_case(&crawl(), Some("admin"), &open_settings_case()).unwrap();
        let actions = vec![
            Action::Navigate {
                url: "http://127.0.0.1:8080/dashboard".to_string(),
            },
            Action::Click {
                selector: "a[href=\"/settings\"]".to_string(),
            },
            Action::AssertUrl {
                url_contains: "/settings".to_string(),
            },
            Action::AssertVisible {
                selector: "#danger-zone".to_string(),
            },
        ];
        assert!(ungrounded(&actions, &g).is_empty());
    }

    #[test]
    fn an_invented_selector_is_named_by_its_step_and_kind() {
        let g = Grounding::for_case(&crawl(), Some("admin"), &open_settings_case()).unwrap();
        let actions = vec![
            Action::Click {
                selector: "#settings-link".to_string(),
            },
            Action::Fill {
                selector: "#username".to_string(),
                value: "x".to_string(),
            },
            Action::AssertText {
                selector: "h1".to_string(),
                text: "Settings".to_string(),
            },
            Action::AssertVisible {
                selector: "#danger-zone".to_string(),
            },
        ];
        let reasons = ungrounded(&actions, &g);
        assert_eq!(reasons.len(), 3, "reasons={reasons:?}");
        assert!(reasons[0].contains("step 1 (CLICK)") && reasons[0].contains("#settings-link"));
        assert!(
            reasons[1].contains("step 2 (FILL)"),
            "a login-page selector is not the admin's: {reasons:?}"
        );
        assert!(reasons[2].contains("step 3 (ASSERT_TEXT)"));
    }

    #[test]
    fn the_reasons_are_capped_and_the_rest_counted() {
        let g = Grounding::for_case(&crawl(), Some("admin"), &open_settings_case()).unwrap();
        let actions: Vec<Action> = (0..20)
            .map(|i| Action::Click {
                selector: format!("#nope-{i}"),
            })
            .collect();
        let reasons = ungrounded(&actions, &g);
        assert_eq!(reasons.len(), MAX_REASONS + 1);
        assert_eq!(reasons.last().unwrap(), "and 12 more");
    }

    #[test]
    fn a_literal_typed_into_a_password_field_is_noted_and_a_reference_or_a_plain_field_is_not() {
        let g = Grounding::for_case(&crawl(), None, &open_settings_case()).unwrap();
        let actions = vec![
            Action::Fill {
                selector: "#username".to_string(),
                value: "bob".to_string(),
            },
            Action::Fill {
                selector: "#password".to_string(),
                value: "wrong-password".to_string(),
            },
            Action::Fill {
                selector: "#password".to_string(),
                value: "env:AIQA_LOGIN_ADMIN_PASSWORD".to_string(),
            },
            Action::Fill {
                selector: "#password".to_string(),
                value: String::new(),
            },
        ];
        let notes = literal_password_notes(&actions, &g);
        assert_eq!(notes.len(), 1, "notes={notes:?}");
        assert!(notes[0].contains("step 2"), "notes={notes:?}");
        assert!(
            !notes[0].contains("wrong-password"),
            "the note never repeats the value: {notes:?}"
        );
    }
}
