//! Phase 6 — the gap check: what the documents say, against what the application shows.
//!
//! The documents give numbered requirements (`requirements.rs`); a crawl gives the pages of the running application and
//! what is on them (`crawler.rs`, `grounding.rs`). Nothing said whether the two agree: a requirement whose feature the
//! application does not show, a page no document mentions, a requirement with no test case. This module lines them up:
//!
//! ```text
//! requirements  ──┐                          per requirement:                     per page:
//! crawl (pages, ──┼─> analyze() ──>  FOUND / WEAK / MISSING / NOT_CHECKABLE   documented, or "not in the documents"
//!  elements)      │                  + the pages it points at, and the words   + which requirements point at it
//! saved cases  ───┘                  + the cases that test it, by technique
//! ```
//!
//! What "found" means is deliberately modest. A requirement is *found* when a page of the crawl carries the words the
//! requirement is about (in its address, title, headings, a link, a button or a field); that is evidence the feature has
//! somewhere to live, not a proof that it works. The matching is on words only — it reads "sign in" and "login" as the
//! same, "basket" and "cart", and stems plurals and endings (`reports` → `report`) — and it says *why*: the words that
//! matched and the elements they were on. A requirement the application words differently comes out `WEAK` or `MISSING`
//! here, which is what the optional steps built on this one (`gap_verify.rs`: the router's AI and the embedder) are for.
//!
//! What can be checked at all depends on what the requirement is about (`RequirementKind`): a crawl can show whether a
//! feature or a field exists, so functional, usability and data requirements can be `MISSING`; it cannot show a response
//! time, an API, or an account lock-out, so those are `NOT_CHECKABLE` unless a page happens to carry their words, and
//! background text and reference material are never checked.
//!
//! Everything in this file is pure (no calls, no clock, no files): the same input gives the same report, and the report
//! is worked out in one pass over the requirements and the pages.

use crate::coverage::{applicable_techniques, resolve_technique, Technique};
use crate::crawler::CrawlOutput;
use crate::domain::TestCaseStatus;
use crate::grounding::CrawlGrounding;
use crate::requirement_generation::RequirementItem;
use crate::requirements::RequirementKind;
use crate::store::TestCaseRow;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// ------------------------------------------------------------------- what a crawl gives

/// One thing on a page a person can see or use.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementEvidence {
    /// `link`, `button`, `input`, `select` or `textarea`.
    pub kind: String,
    /// What a person would call it: its label, placeholder or visible text.
    pub text: String,
    /// A locator or selector hint: only the words in it are used (`[data-test="add-to-cart"]` says "add to cart").
    pub hint: String,
    /// Where a link leads.
    pub href: Option<String>,
}

/// One page of the crawl, however many roles reached it.
#[derive(Debug, Clone, PartialEq)]
pub struct PageEvidence {
    pub url: String,
    pub title: String,
    pub headings: Vec<String>,
    pub elements: Vec<ElementEvidence>,
    /// The roles whose crawl reached the page.
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoleEvidence {
    pub role: String,
    pub login_succeeded: bool,
    pub pages: usize,
}

/// A crawl as far as the gap check is concerned: its pages (merged across roles by address) and its roles.
#[derive(Debug, Clone, PartialEq)]
pub struct CrawlEvidence {
    /// `grounding` (the crawl's grounding file: links, buttons and fields) or `result` (the older result: links and
    /// buttons only).
    pub source: &'static str,
    pub roles: Vec<RoleEvidence>,
    pub pages: Vec<PageEvidence>,
}

/// The most pages one report reads: a crawl visits twenty-odd pages per role, so this is a guard, not a limit anyone
/// meets.
const MAX_PAGES: usize = 300;

fn normalize_url(url: &str) -> String {
    let no_fragment = url.split('#').next().unwrap_or(url).trim();
    match no_fragment.split_once("://") {
        Some((scheme, rest)) => {
            let (host, path) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
            let path = path.trim_end_matches('/');
            if path.is_empty() {
                format!("{}://{}/", scheme.to_lowercase(), host.to_lowercase())
            } else {
                format!("{}://{}/{path}", scheme.to_lowercase(), host.to_lowercase())
            }
        }
        None => no_fragment.to_string(),
    }
}

/// Adds `page` (as one role saw it) to `pages`, merging with the page already there at the same address.
fn merge_page(pages: &mut Vec<PageEvidence>, role: &str, mut page: PageEvidence) {
    page.url = normalize_url(&page.url);
    page.roles = vec![role.to_string()];
    if let Some(existing) = pages.iter_mut().find(|p| p.url == page.url) {
        if !existing.roles.iter().any(|r| r == role) {
            existing.roles.push(role.to_string());
        }
        if existing.title.trim().is_empty() {
            existing.title = page.title;
        }
        for heading in page.headings {
            if !existing.headings.contains(&heading) {
                existing.headings.push(heading);
            }
        }
        for element in page.elements {
            if !existing.elements.contains(&element) {
                existing.elements.push(element);
            }
        }
    } else if pages.len() < MAX_PAGES {
        pages.push(page);
    }
}

/// Picks the roles the report is about: all of them, or the one named (case does not matter). The `Err` lists the
/// roles there are.
fn wanted<'a>(roles: &'a [String], role: Option<&str>) -> Result<Vec<&'a str>, String> {
    match role.map(str::trim).filter(|r| !r.is_empty()) {
        None => Ok(roles.iter().map(String::as_str).collect()),
        Some(name) => roles
            .iter()
            .find(|r| r.eq_ignore_ascii_case(name))
            .map(|r| vec![r.as_str()])
            .ok_or_else(|| {
                format!(
                    "the crawl has no role {name:?} (its roles: {})",
                    roles.join(", ")
                )
            }),
    }
}

/// The evidence in a crawl's grounding file — every link, button and field, with its label and locator.
pub fn evidence_from_grounding(
    crawl: &CrawlGrounding,
    role: Option<&str>,
) -> Result<CrawlEvidence, String> {
    let names: Vec<String> = crawl.roles.iter().map(|r| r.role.clone()).collect();
    let chosen = wanted(&names, role)?;
    let mut pages = Vec::new();
    let mut roles = Vec::new();
    for r in crawl
        .roles
        .iter()
        .filter(|r| chosen.contains(&r.role.as_str()))
    {
        roles.push(RoleEvidence {
            role: r.role.clone(),
            login_succeeded: r.login_succeeded,
            pages: r.pages.len(),
        });
        for p in &r.pages {
            merge_page(
                &mut pages,
                &r.role,
                PageEvidence {
                    url: p.url.clone(),
                    title: p.title.clone(),
                    headings: p.headings.clone(),
                    elements: p
                        .elements
                        .iter()
                        .map(|e| ElementEvidence {
                            kind: e.kind.clone(),
                            text: e.text.clone(),
                            hint: e.locator.clone(),
                            href: e.href.clone(),
                        })
                        .collect(),
                    roles: Vec::new(),
                },
            );
        }
    }
    Ok(CrawlEvidence {
        source: "grounding",
        roles,
        pages,
    })
}

/// The evidence in a crawl's result — links and buttons only; what an older crawl (made before grounding was recorded)
/// has.
pub fn evidence_from_result(
    output: &CrawlOutput,
    role: Option<&str>,
) -> Result<CrawlEvidence, String> {
    let names: Vec<String> = output.roles.iter().map(|r| r.role.clone()).collect();
    let chosen = wanted(&names, role)?;
    let mut pages = Vec::new();
    let mut roles = Vec::new();
    for r in output
        .roles
        .iter()
        .filter(|r| chosen.contains(&r.role.as_str()))
    {
        roles.push(RoleEvidence {
            role: r.role.clone(),
            login_succeeded: r.login_succeeded,
            pages: r.pages.len(),
        });
        for p in &r.pages {
            merge_page(
                &mut pages,
                &r.role,
                PageEvidence {
                    url: p.url.clone(),
                    title: p.title.clone(),
                    headings: Vec::new(),
                    elements: p
                        .elements
                        .iter()
                        .map(|e| ElementEvidence {
                            kind: e.kind.clone(),
                            text: e.text.clone(),
                            hint: e.selector_hint.clone(),
                            href: e.href.clone(),
                        })
                        .collect(),
                    roles: Vec::new(),
                },
            );
        }
    }
    Ok(CrawlEvidence {
        source: "result",
        roles,
        pages,
    })
}

// ----------------------------------------------------------------------------- words

/// Words too common in requirement prose, or on any web page, to say which feature a text is about.
#[rustfmt::skip]
const STOP_WORDS: &[&str] = &[
    "about", "above", "across", "after", "again", "all", "also", "although", "always", "and", "any", "are",
    "because", "been", "before", "being", "below", "between", "both", "but", "can", "cannot", "could", "did",
    "does", "doing", "done", "down", "during", "each", "either", "else", "etc", "even", "ever", "every", "few",
    "for", "from", "further", "had", "has", "have", "having", "her", "here", "his", "how", "however", "into", "its",
    "itself", "just", "least", "less", "like", "may", "might", "more", "most", "much", "must", "nor", "not", "now",
    "off", "once", "one", "only", "onto", "other", "our", "out", "over", "own", "per", "same", "shall", "she",
    "should", "since", "some", "such", "than", "that", "the", "their", "them", "then", "there", "these", "they",
    "this", "those", "though", "through", "thus", "too", "under", "until", "upon", "use", "used", "using", "very",
    "via", "was", "were", "what", "when", "whenever", "where", "whether", "which", "while", "who", "whom", "whose",
    "why", "will", "with", "within", "without", "would", "yet", "you", "your",
    // What a requirement says about itself, and about pages, rather than about a feature.
    "system", "application", "app", "software", "platform", "site", "website", "web", "user", "users", "able",
    "ability", "allow", "allows", "allowed", "allowing", "enable", "enables", "enabled", "provide", "provides",
    "provided", "providing", "support", "supports", "supported", "ensure", "ensures", "ensuring", "include",
    "includes", "included", "including", "need", "needs", "needed", "require", "requires", "required",
    "requirement", "requirements", "following", "listed", "given", "specified", "appropriate", "applicable",
    "relevant", "valid", "invalid", "correct", "correctly", "properly", "successfully", "successful", "new",
    "existing", "available", "various", "several", "different", "specific", "certain", "feature", "features",
    "function", "functions", "functionality", "capability", "capabilities", "page", "pages", "screen", "screens",
    "view", "views", "display", "displays", "displayed", "show", "shows", "shown", "see", "sees", "seen", "click",
    "clicks", "clicked", "select", "selects", "selected", "choose", "chooses", "enter", "enters", "entered", "type",
    "types", "typed", "press", "button", "buttons", "link", "links", "field", "fields", "minute", "minutes",
    "second", "seconds", "hour", "hours", "day", "days", "data", "information", "detail", "details", "option",
    "options", "list", "case", "test", "verify",
    // Verbs a requirement uses for "do something with" a feature, which say nothing about which feature.
    "open", "opens", "opened", "access", "accesses", "accessed", "navigate", "navigates", "perform", "performs",
    "performed", "make", "makes", "made", "get", "gets", "take", "takes", "give", "gives", "giving", "let", "lets",
    "want", "wants", "complete", "completes", "completed", "proceed", "proceeds",
];

/// The same word said another way: what a requirement and a page are likely to differ on, and nothing cleverer.
const SYNONYMS: &[(&str, &str)] = &[
    ("signin", "login"),
    ("signon", "login"),
    ("logon", "login"),
    ("logins", "login"),
    ("signout", "logout"),
    ("logoff", "logout"),
    ("logouts", "logout"),
    ("signup", "register"),
    ("registration", "register"),
    ("registrations", "register"),
    ("registering", "register"),
    ("registered", "register"),
    ("registers", "register"),
    ("basket", "cart"),
    ("baskets", "cart"),
    ("trolley", "cart"),
    ("carts", "cart"),
    ("passcode", "password"),
    ("passwd", "password"),
    ("pwd", "password"),
    ("passwords", "password"),
    ("userid", "username"),
    ("usernames", "username"),
    ("emails", "email"),
    ("remove", "delete"),
    ("removes", "delete"),
    ("removing", "delete"),
    ("removed", "delete"),
    ("removal", "delete"),
    ("erase", "delete"),
    ("deleting", "delete"),
    ("deleted", "delete"),
    ("deletion", "delete"),
    ("modify", "edit"),
    ("modifies", "edit"),
    ("modification", "edit"),
    ("update", "edit"),
    ("updates", "edit"),
    ("updating", "edit"),
    ("editing", "edit"),
    ("purchase", "checkout"),
    ("purchases", "checkout"),
    ("purchasing", "checkout"),
];

/// The words the synonyms and the phrase merges lead to.
#[rustfmt::skip]
const CANONICAL: &[&str] = &[
    "login", "logout", "register", "cart", "checkout", "email", "username", "password", "delete", "edit",
];

/// Words in a locator (`[data-test="..."]`, `#id`, `.class`) that describe the markup, not the feature.
#[rustfmt::skip]
const LOCATOR_NOISE: &[&str] = &[
    "data", "test", "testid", "nth", "child", "input", "button", "link", "div", "span", "container", "wrapper",
    "btn", "form", "name", "type", "class", "href", "text", "role", "aria", "label", "item", "items", "row", "col",
    "cell", "table", "list", "icon", "img", "image", "main", "root", "app", "content", "section", "header",
    "footer", "body", "html", "first", "last", "not", "has", "contains",
];

/// A file extension in an address, which is not a word about the feature.
#[rustfmt::skip]
const ADDRESS_NOISE: &[&str] = &[
    "html", "htm", "php", "aspx", "asp", "jsp", "index", "www", "com", "org", "net",
];

/// Splits `text` into words: at anything that is not a letter or digit, and inside `camelCase`.
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for c in text.chars() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous_lower = false;
            continue;
        }
        if c.is_uppercase() && previous_lower && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        previous_lower = c.is_lowercase();
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Pairs of words that are one word in a page: `sign in` is `login`.
fn merge_phrases(words: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some(next) = words.get(i + 1) {
            let merged = match (words[i].as_str(), next.as_str()) {
                ("log", "in") | ("sign", "in") | ("log", "on") => Some("login"),
                ("log", "out") | ("sign", "out") | ("log", "off") => Some("logout"),
                ("sign", "up") => Some("register"),
                ("check", "out") => Some("checkout"),
                ("e", "mail") => Some("email"),
                ("user", "name") => Some("username"),
                ("shopping", "cart") | ("shopping", "basket") => Some("cart"),
                _ => None,
            };
            if let Some(word) = merged {
                out.push(word.to_string());
                i += 2;
                continue;
            }
        }
        out.push(words[i].clone());
        i += 1;
    }
    out
}

fn undouble(word: &mut String) {
    let bytes = word.as_bytes();
    let n = bytes.len();
    if n >= 4 && bytes[n - 1] == bytes[n - 2] && b"bdgmnprt".contains(&bytes[n - 1]) {
        word.pop();
    }
}

/// The word without its ending, so `reports`, `reported` and `reporting` are one word. Crude on purpose: it has to make
/// the requirement's word and the page's word the *same*, not be right about English.
fn stem(word: &str) -> String {
    if !word.is_ascii() || word.len() <= 3 {
        return word.to_string();
    }
    let mut w = word.to_string();
    // Plurals.
    if w.ends_with("ies") && w.len() > 4 {
        w.truncate(w.len() - 3);
        w.push('y');
    } else if w.ends_with("sses") {
        w.truncate(w.len() - 2);
    } else if w.len() >= 5
        && w.ends_with("es")
        && ["ches", "shes", "xes", "zes"]
            .iter()
            .any(|s| w.ends_with(s))
    {
        w.truncate(w.len() - 2);
    } else if w.len() >= 4 && w.ends_with('s') && !["ss", "us", "is"].iter().any(|s| w.ends_with(s))
    {
        w.pop();
    }
    // Verb endings.
    if w.len() >= 6 && w.ends_with("ing") {
        w.truncate(w.len() - 3);
        undouble(&mut w);
    } else if w.len() >= 5 && w.ends_with("ed") {
        w.truncate(w.len() - 2);
        undouble(&mut w);
    } else if w.len() >= 8 && w.ends_with("ation") {
        w.truncate(w.len() - 5);
        w.push_str("at");
    }
    if w.len() >= 5 && w.ends_with('e') {
        w.pop();
    }
    w
}

/// A word a requirement or a page is about: the stem it is compared by, and the word to show a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub stem: String,
    pub shown: String,
}

/// The words of `text` worth comparing, each once, in the order they first appear.
pub fn terms_of(text: &str) -> Vec<Term> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for word in merge_phrases(split_words(text)) {
        if word.chars().count() < 3 || word.chars().all(|c| c.is_numeric()) {
            continue;
        }
        if STOP_WORDS.contains(&word.as_str()) {
            continue;
        }
        let term = match SYNONYMS.iter().find(|(from, _)| *from == word) {
            Some((_, to)) => Term {
                stem: (*to).to_string(),
                shown: (*to).to_string(),
            },
            // A word the synonyms lead to is already what it is compared by: stemming it would split `username` from
            // `usernames`.
            None if CANONICAL.contains(&word.as_str()) => Term {
                stem: word.clone(),
                shown: word,
            },
            None => Term {
                stem: stem(&word),
                shown: word,
            },
        };
        if seen.insert(term.stem.clone()) {
            out.push(term);
        }
    }
    out
}

/// The words of a locator or selector hint that name a feature.
fn locator_terms(locator: &str) -> Vec<Term> {
    terms_of(locator)
        .into_iter()
        .filter(|t| {
            !LOCATOR_NOISE.contains(&t.shown.as_str()) && !LOCATOR_NOISE.contains(&t.stem.as_str())
        })
        .collect()
}

/// The words of an address's path: `/reports/monthly.html` is `reports` and `monthly`.
fn address_terms(url: &str) -> Vec<Term> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.split_once('/').map_or("", |(_, p)| p);
    let path = path.split(['?', '#']).next().unwrap_or("");
    terms_of(path)
        .into_iter()
        .filter(|t| !ADDRESS_NOISE.contains(&t.shown.as_str()))
        .collect()
}

// ---------------------------------------------------------------------------- pages

const WEIGHT_ADDRESS: f32 = 1.5;
const WEIGHT_TITLE: f32 = 1.2;
const WEIGHT_HEADING: f32 = 1.2;
const WEIGHT_LABEL: f32 = 1.0;
const WEIGHT_LOCATOR: f32 = 0.8;
const WEIGHT_LINK_TARGET: f32 = 0.7;

/// A page prepared for matching.
struct PageDoc {
    url: String,
    title: String,
    headings: Vec<String>,
    roles: Vec<String>,
    /// Word (stem) -> how strongly the page carries it: the best of where it appears.
    terms: HashMap<String, f32>,
    /// Elements, for showing which ones carried a word.
    elements: Vec<ElementDoc>,
}

struct ElementDoc {
    text: String,
    stems: BTreeSet<String>,
}

fn add_terms(terms: &mut HashMap<String, f32>, found: Vec<Term>, weight: f32) {
    for term in found {
        let entry = terms.entry(term.stem).or_insert(0.0);
        if weight > *entry {
            *entry = weight;
        }
    }
}

fn prepare(page: &PageEvidence) -> PageDoc {
    let mut terms = HashMap::new();
    add_terms(&mut terms, address_terms(&page.url), WEIGHT_ADDRESS);
    add_terms(&mut terms, terms_of(&page.title), WEIGHT_TITLE);
    for heading in &page.headings {
        add_terms(&mut terms, terms_of(heading), WEIGHT_HEADING);
    }
    let mut elements = Vec::new();
    for element in &page.elements {
        let label = terms_of(&element.text);
        let stems: BTreeSet<String> = label.iter().map(|t| t.stem.clone()).collect();
        add_terms(&mut terms, label, WEIGHT_LABEL);
        add_terms(&mut terms, locator_terms(&element.hint), WEIGHT_LOCATOR);
        if let Some(href) = &element.href {
            add_terms(&mut terms, address_terms(href), WEIGHT_LINK_TARGET);
        }
        if !element.text.trim().is_empty() {
            elements.push(ElementDoc {
                text: element.text.trim().to_string(),
                stems,
            });
        }
    }
    PageDoc {
        url: page.url.clone(),
        title: page.title.clone(),
        headings: page.headings.clone(),
        roles: page.roles.clone(),
        terms,
        elements,
    }
}

// -------------------------------------------------------------------------- findings

/// How well the application shows a requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    /// A page carries the words the requirement is about.
    Found,
    /// Something related is on a page, but not enough to say the feature is there.
    Weak,
    /// Nothing on any page of the crawl carries them.
    Missing,
    /// A crawl cannot show this kind of requirement (a response time, an API, background text).
    NotCheckable,
}

/// What a page match rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Words the requirement and the page share.
    Text,
    /// The page's text is close in meaning to the requirement's (the embedder).
    Embedding,
    /// The AI read the requirement and the page and agreed.
    Ai,
}

/// What the AI said about a requirement it was asked to check (`gap_verify.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    /// A page it was shown clearly shows the feature.
    Confirmed,
    /// None of the pages it was shown does.
    NotFound,
    /// A page shows something related, not enough to tell.
    Unclear,
    /// It could not be asked, or did not answer for this requirement.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verification {
    pub verdict: Verdict,
    /// The addresses of the pages it named — only ones it was shown.
    pub pages: Vec<String>,
    pub reason: String,
    /// `provider/model` that answered.
    pub by: String,
}

/// One page a requirement points at.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageMatch {
    pub url: String,
    pub title: String,
    /// How much of what the requirement is about the page carries, 0 to 1: the requirement's words on the page, each
    /// counted by how much it says about that requirement.
    pub score: f32,
    pub basis: Basis,
    /// The requirement's words found on the page, most telling first.
    pub terms: Vec<String>,
    /// Up to three things on the page that carry those words.
    pub elements: Vec<String>,
    pub roles: Vec<String>,
    /// For an `embedding` match: how close, 0 to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity: Option<f32>,
}

/// What the requirement's cases cover. Rejected cases count for nothing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseCoverage {
    pub total: usize,
    pub approved: usize,
    pub executed: usize,
    /// Cases by technique (a case with no technique label counts as positive, as it does when a run is generated).
    pub techniques: BTreeMap<&'static str, usize>,
    /// Techniques that apply to this kind of requirement and have no case.
    pub missing_techniques: Vec<&'static str>,
}

/// A saved test case as the gap check needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseInfo {
    pub requirement_key: Option<String>,
    pub status: TestCaseStatus,
    pub technique: Technique,
}

impl From<&TestCaseRow> for CaseInfo {
    fn from(row: &TestCaseRow) -> Self {
        let label = row
            .tags
            .iter()
            .find_map(|t| t.strip_prefix("technique:"))
            .map(str::trim);
        CaseInfo {
            requirement_key: row
                .requirement_reference
                .as_deref()
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_string),
            status: row.status,
            technique: resolve_technique(label, Technique::Positive),
        }
    }
}

/// A requirement's standing against the crawl.
#[derive(Debug, Clone)]
pub struct Finding {
    pub key: String,
    pub section: Option<String>,
    pub kind: RequirementKind,
    pub text: String,
    pub status: Status,
    pub basis: Option<Basis>,
    pub matches: Vec<PageMatch>,
    pub verification: Option<Verification>,
    pub cases: CaseCoverage,
    /// The requirement's words, and how each page scored on them (best first): what later steps work from.
    pub(crate) terms: Vec<Term>,
    pub(crate) scored: Vec<Scored>,
}

impl Finding {
    /// How much of the requirement the best page carries, by the words.
    pub(crate) fn best_coverage(&self) -> f32 {
        self.scored.first().map_or(0.0, |s| s.coverage)
    }

    /// The pages the words came nearest to (best first, at most [`MAX_MATCHES`]), whether or not they were enough.
    pub(crate) fn candidate_pages(&self) -> Vec<usize> {
        self.scored
            .iter()
            .take(MAX_MATCHES)
            .map(|s| s.page)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Scored {
    pub page: usize,
    pub coverage: f32,
    pub strength: f32,
    /// Matched words (stems) by how much each counted, most first.
    pub stems: Vec<String>,
}

/// A page no requirement points at.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UndocumentedPage {
    pub url: String,
    pub title: String,
    pub roles: Vec<String>,
    /// A few things on it, so a person can tell what the page is.
    pub elements: Vec<String>,
    pub element_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GapSummary {
    pub requirements: usize,
    /// Requirements a crawl can show: functional, usability and data ones.
    pub checkable: usize,
    pub found: usize,
    pub weak: usize,
    pub missing: usize,
    pub not_checkable: usize,
    pub pages: usize,
    pub undocumented_pages: usize,
    /// Cases (not rejected) tied to a requirement of this report.
    pub cases: usize,
    pub with_cases: usize,
    /// Requirements the application shows (found or weak) that no case tests.
    pub shown_without_cases: usize,
    /// Requirements with cases that the crawl found no sign of: their cases may not be executable.
    pub tested_without_evidence: usize,
}

/// What the check says, before the parts a caller adds (which crawl, which optional steps ran).
#[derive(Debug, Clone, Serialize)]
pub struct GapReport {
    pub summary: GapSummary,
    pub requirements: Vec<RequirementFinding>,
    pub undocumented_pages: Vec<UndocumentedPage>,
}

/// A requirement as the report shows it.
#[derive(Debug, Clone, Serialize)]
pub struct RequirementFinding {
    pub key: String,
    pub section: Option<String>,
    pub kind: &'static str,
    pub text: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis: Option<Basis>,
    pub matches: Vec<PageMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<Verification>,
    pub cases: CaseCoverage,
}

/// The working state of one check: what the deterministic pass found, open to the optional steps, which change a
/// finding's status and pages, and then to [`Analysis::report`].
pub struct Analysis {
    pub findings: Vec<Finding>,
    pages: Vec<PageDoc>,
}

pub(crate) const MAX_MATCHES: usize = 3;
const MAX_MATCH_ELEMENTS: usize = 3;
const MAX_TEXT_CHARS: usize = 400;
const MAX_UNDOCUMENTED_ELEMENTS: usize = 6;
/// A page carries this share of what a requirement is about (see [`PageMatch::score`]) or more: the feature is there.
const FOUND_COVERAGE: f32 = 0.6;
/// … or this much: something related is there, no more. Under it a page says nothing about the requirement.
const WEAK_COVERAGE: f32 = 0.25;
/// A page is kept as a match when it carries at least this share of what the best page does.
const KEEP_SHARE: f32 = 0.6;

fn round2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}

/// An address without its origin: what tells two pages of one application apart.
fn path_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    match rest.split_once('/') {
        Some((_, path)) => format!("/{path}"),
        None => "/".to_string(),
    }
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let cut: String = text.chars().take(limit).collect();
    format!("{}…", cut.trim_end())
}

/// How much each word says about *which* requirement it belongs to: a word in many requirements says little (`product`
/// in a shop's FRD), a word in one says a lot (`wishlist`). Never zero: a common word is still part of what the
/// requirement is about.
fn requirement_weights(term_lists: &[&[Term]]) -> HashMap<String, f32> {
    let n = term_lists.len().max(1) as f32;
    let idf = |df: f32| (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
    let best = idf(1.0);
    let mut df: HashMap<&str, f32> = HashMap::new();
    for terms in term_lists {
        for term in *terms {
            *df.entry(term.stem.as_str()).or_insert(0.0) += 1.0;
        }
    }
    df.into_iter()
        .map(|(stem, df)| {
            let share = if best > 0.0 {
                (idf(df) / best).clamp(0.0, 1.0)
            } else {
                1.0
            };
            (stem.to_string(), 0.3 + 0.7 * share)
        })
        .collect()
}

/// How much of a requirement one page carries. `coverage` is the share of the requirement's words, weighted by how much
/// each says, that appear on the page in a label, title, heading or address (a word found only in a locator or a link's
/// target counts for less). `strength` also rewards where they appear (an address says more than a label), and only
/// breaks ties between pages that carry the same share.
fn score_page(
    page_index: usize,
    page: &PageDoc,
    wanted: &[Term],
    weights: &HashMap<String, f32>,
    total_weight: f32,
) -> Scored {
    let mut hits: Vec<(String, f32, f32)> = wanted
        .iter()
        .filter_map(|term| {
            let field = *page.terms.get(&term.stem)?;
            let weight = weights.get(&term.stem).copied().unwrap_or(0.3);
            Some((term.stem.clone(), weight * field, weight * field.min(1.0)))
        })
        .collect();
    hits.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let covered: f32 = hits.iter().map(|h| h.2).sum();
    Scored {
        page: page_index,
        strength: hits.iter().map(|h| h.1).sum(),
        coverage: if total_weight > 0.0 {
            (covered / total_weight).clamp(0.0, 1.0)
        } else {
            0.0
        },
        stems: hits.into_iter().map(|h| h.0).collect(),
    }
}

/// The status a requirement's best page amounts to.
fn classify(best: &Scored) -> Status {
    if best.stems.is_empty() || best.coverage < WEAK_COVERAGE {
        Status::Missing
    } else if best.coverage >= FOUND_COVERAGE {
        Status::Found
    } else {
        Status::Weak
    }
}

/// What kinds of requirement a crawl can speak to.
enum Reach {
    /// It can say a feature is not there.
    Full,
    /// It can say a feature is there, and nothing else.
    FoundOnly,
    /// It cannot say anything.
    Nothing,
}

fn reach(kind: RequirementKind) -> Reach {
    match kind {
        RequirementKind::Functional | RequirementKind::Usability | RequirementKind::Data => {
            Reach::Full
        }
        RequirementKind::Security => Reach::FoundOnly,
        RequirementKind::Interface
        | RequirementKind::Performance
        | RequirementKind::Context
        | RequirementKind::Reference => Reach::Nothing,
    }
}

/// Whether a crawl can show that a requirement of this kind is missing.
pub fn is_checkable(kind: RequirementKind) -> bool {
    matches!(reach(kind), Reach::Full)
}

fn case_coverage(kind: RequirementKind, cases: &[&CaseInfo]) -> CaseCoverage {
    let live: Vec<&&CaseInfo> = cases
        .iter()
        .filter(|c| c.status != TestCaseStatus::Rejected)
        .collect();
    let mut techniques: BTreeMap<&'static str, usize> = BTreeMap::new();
    for case in &live {
        *techniques.entry(case.technique.as_str()).or_insert(0) += 1;
    }
    let missing_techniques = applicable_techniques(kind)
        .iter()
        .filter(|t| !techniques.contains_key(t.as_str()))
        .map(|t| t.as_str())
        .collect();
    CaseCoverage {
        total: live.len(),
        approved: live
            .iter()
            .filter(|c| c.status == TestCaseStatus::Approved)
            .count(),
        executed: live
            .iter()
            .filter(|c| c.status == TestCaseStatus::Executed)
            .count(),
        techniques,
        missing_techniques,
    }
}

/// The words a match was made on, as the requirement wrote them.
fn shown_terms(finding_terms: &[Term], stems: &[String]) -> Vec<String> {
    stems
        .iter()
        .filter_map(|s| finding_terms.iter().find(|t| &t.stem == s))
        .map(|t| t.shown.clone())
        .collect()
}

/// Up to [`MAX_MATCH_ELEMENTS`] elements of the page that carry the matched words, the ones carrying most first.
fn carrying_elements(page: &PageDoc, stems: &[String]) -> Vec<String> {
    let wanted: HashSet<&String> = stems.iter().collect();
    let mut carrying: Vec<(usize, &ElementDoc)> = page
        .elements
        .iter()
        .map(|e| (e.stems.iter().filter(|s| wanted.contains(s)).count(), e))
        .filter(|(n, _)| *n > 0)
        .collect();
    carrying.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = HashSet::new();
    carrying
        .into_iter()
        .filter(|(_, e)| seen.insert(e.text.to_lowercase()))
        .take(MAX_MATCH_ELEMENTS)
        .map(|(_, e)| truncate_chars(&e.text, 60))
        .collect()
}

impl Analysis {
    fn page_match(&self, finding: &Finding, scored: &Scored) -> PageMatch {
        let page = &self.pages[scored.page];
        PageMatch {
            url: page.url.clone(),
            title: page.title.clone(),
            score: round2(scored.coverage),
            basis: Basis::Text,
            terms: shown_terms(&finding.terms, &scored.stems),
            elements: carrying_elements(page, &scored.stems),
            roles: page.roles.clone(),
            similarity: None,
        }
    }

    pub(crate) fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn page_url(&self, page: usize) -> &str {
        &self.pages[page].url
    }

    pub(crate) fn element_count(&self, page: usize) -> usize {
        self.pages[page].elements.len()
    }

    /// A page as an AI is shown it, one line: its path, its title and the labels of what is on it.
    pub(crate) fn page_brief(&self, page: usize, max_chars: usize) -> String {
        let p = &self.pages[page];
        let mut line = path_of(&p.url);
        if !p.title.trim().is_empty() {
            line.push_str(&format!(" — {:?}", truncate_chars(p.title.trim(), 60)));
        }
        let mut seen = HashSet::new();
        let labels: Vec<String> = p
            .elements
            .iter()
            .filter(|e| seen.insert(e.text.to_lowercase()))
            .map(|e| format!("{:?}", truncate_chars(&e.text, 40)))
            .collect();
        if !labels.is_empty() {
            line.push_str(" — on it: ");
            line.push_str(&labels.join(", "));
        }
        truncate_chars(&line, max_chars)
    }

    /// The text of a page an embedder compares: its address, title, headings and the labels of what is on it.
    pub(crate) fn page_text(&self, page: usize, max_chars: usize) -> String {
        let p = &self.pages[page];
        let mut parts = vec![path_of(&p.url), p.title.trim().to_string()];
        parts.extend(p.headings.iter().map(|h| h.trim().to_string()));
        let mut seen = HashSet::new();
        parts.extend(
            p.elements
                .iter()
                .filter(|e| seen.insert(e.text.to_lowercase()))
                .map(|e| e.text.clone()),
        );
        parts.retain(|s| !s.is_empty());
        truncate_chars(&parts.join(". "), max_chars)
    }

    /// The match a page makes for a requirement when a step other than the words chose it: it still says which of the
    /// requirement's words the page carries.
    pub(crate) fn match_for_page(
        &self,
        finding: usize,
        page: usize,
        basis: Basis,
        similarity: Option<f32>,
    ) -> PageMatch {
        let f = &self.findings[finding];
        let scored = f
            .scored
            .iter()
            .find(|s| s.page == page)
            .cloned()
            .unwrap_or(Scored {
                page,
                coverage: 0.0,
                strength: 0.0,
                stems: Vec::new(),
            });
        let mut m = self.page_match(f, &scored);
        m.basis = basis;
        m.similarity = similarity.map(round2);
        m
    }

    /// Sets what a step that decided a finding says of it: its status, what that rests on, and the pages.
    pub(crate) fn settle(
        &mut self,
        finding: usize,
        status: Status,
        basis: Basis,
        matches: Vec<PageMatch>,
    ) {
        let f = &mut self.findings[finding];
        f.status = status;
        f.basis = Some(basis);
        f.matches = matches;
    }

    /// The report: the counts, each requirement's standing, and the pages no requirement points at.
    pub fn report(&self) -> GapReport {
        let mut documented: HashSet<&str> = HashSet::new();
        for finding in &self.findings {
            if matches!(finding.status, Status::Found | Status::Weak) {
                for m in &finding.matches {
                    documented.insert(m.url.as_str());
                }
            }
        }
        let undocumented_pages: Vec<UndocumentedPage> = self
            .pages
            .iter()
            .filter(|p| !documented.contains(p.url.as_str()))
            .map(|p| {
                let mut seen = HashSet::new();
                UndocumentedPage {
                    url: p.url.clone(),
                    title: p.title.clone(),
                    roles: p.roles.clone(),
                    elements: p
                        .elements
                        .iter()
                        .filter(|e| seen.insert(e.text.to_lowercase()))
                        .take(MAX_UNDOCUMENTED_ELEMENTS)
                        .map(|e| truncate_chars(&e.text, 40))
                        .collect(),
                    element_count: p.elements.len(),
                }
            })
            .collect();

        let mut summary = GapSummary {
            requirements: self.findings.len(),
            pages: self.pages.len(),
            undocumented_pages: undocumented_pages.len(),
            ..GapSummary::default()
        };
        for f in &self.findings {
            if is_checkable(f.kind) {
                summary.checkable += 1;
            }
            match f.status {
                Status::Found => summary.found += 1,
                Status::Weak => summary.weak += 1,
                Status::Missing => summary.missing += 1,
                Status::NotCheckable => summary.not_checkable += 1,
            }
            summary.cases += f.cases.total;
            if f.cases.total > 0 {
                summary.with_cases += 1;
            }
            if matches!(f.status, Status::Found | Status::Weak) && f.cases.total == 0 {
                summary.shown_without_cases += 1;
            }
            if f.status == Status::Missing && f.cases.total > 0 {
                summary.tested_without_evidence += 1;
            }
        }
        GapReport {
            summary,
            requirements: self
                .findings
                .iter()
                .map(|f| RequirementFinding {
                    key: f.key.clone(),
                    section: f.section.clone(),
                    kind: f.kind.as_str(),
                    text: truncate_chars(&f.text, MAX_TEXT_CHARS),
                    status: f.status,
                    basis: f.basis,
                    matches: f.matches.clone(),
                    verification: f.verification.clone(),
                    cases: f.cases.clone(),
                })
                .collect(),
            undocumented_pages,
        }
    }
}

/// Lines each requirement up against the pages of the crawl, and against the cases that test it.
pub fn analyze(
    requirements: &[RequirementItem],
    evidence: &CrawlEvidence,
    cases: &[CaseInfo],
) -> Analysis {
    let pages: Vec<PageDoc> = evidence.pages.iter().map(prepare).collect();
    // The words of every requirement a crawl can speak to, so what a word says about a requirement can be told from how
    // many requirements share it.
    let term_lists: Vec<Vec<Term>> = requirements
        .iter()
        .map(|r| match reach(r.kind) {
            Reach::Nothing => Vec::new(),
            _ => terms_of(&r.text),
        })
        .collect();
    let weights = requirement_weights(
        &term_lists
            .iter()
            .map(Vec::as_slice)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>(),
    );

    let mut by_key: HashMap<&str, Vec<&CaseInfo>> = HashMap::new();
    for case in cases {
        if let Some(key) = case.requirement_key.as_deref() {
            by_key.entry(key).or_default().push(case);
        }
    }

    let mut analysis = Analysis {
        findings: Vec::with_capacity(requirements.len()),
        pages,
    };
    for (requirement, terms) in requirements.iter().zip(term_lists) {
        let how = reach(requirement.kind);
        let total_weight: f32 = terms
            .iter()
            .map(|t| weights.get(&t.stem).copied().unwrap_or(0.3))
            .sum();
        let mut scored: Vec<Scored> = analysis
            .pages
            .iter()
            .enumerate()
            .map(|(i, page)| score_page(i, page, &terms, &weights, total_weight))
            .filter(|s| !s.stems.is_empty())
            .collect();
        scored.sort_by(|a, b| {
            b.coverage
                .total_cmp(&a.coverage)
                .then_with(|| b.strength.total_cmp(&a.strength))
                .then_with(|| a.page.cmp(&b.page))
        });

        let mut finding = Finding {
            key: requirement.key.clone(),
            section: requirement.section.clone(),
            kind: requirement.kind,
            text: requirement.text.clone(),
            status: Status::NotCheckable,
            basis: None,
            matches: Vec::new(),
            verification: None,
            cases: case_coverage(
                requirement.kind,
                by_key
                    .get(requirement.key.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ),
            terms,
            scored,
        };
        // A requirement with no word worth comparing (all filler) cannot be lined up with anything.
        if !matches!(how, Reach::Nothing) && !finding.terms.is_empty() {
            let status = match finding.scored.first() {
                Some(best) => classify(best),
                None => Status::Missing,
            };
            finding.status = match (status, &how) {
                (Status::Missing, Reach::FoundOnly) => Status::NotCheckable,
                (other, _) => other,
            };
            if matches!(finding.status, Status::Found | Status::Weak) {
                let keep = WEAK_COVERAGE.max(KEEP_SHARE * finding.scored[0].coverage);
                let kept: Vec<Scored> = finding
                    .scored
                    .iter()
                    .filter(|s| s.coverage >= keep)
                    .take(MAX_MATCHES)
                    .cloned()
                    .collect();
                finding.matches = kept
                    .iter()
                    .map(|s| analysis.page_match(&finding, s))
                    .collect();
                finding.basis = Some(Basis::Text);
            }
        }
        analysis.findings.push(finding);
    }
    analysis
}

/// The most words [`focus_terms`] gives, and the shortest stem worth looking for (a link's text or address is searched for
/// it as a substring, and three letters match too much).
const MAX_FOCUS_TERMS: usize = 200;
const MIN_FOCUS_TERM_CHARS: usize = 4;
/// With this many requirements or more, a word most of them share (more than [`FOCUS_TOO_COMMON`]) names no page.
const FOCUS_COMMON_FROM: usize = 5;
const FOCUS_TOO_COMMON: f32 = 0.4;

/// The words a crawl should look for first, from what the requirements a crawl can speak to are about: the stems of their
/// words (a stem is the start of most of the words it stands for, so it finds `reports` and `reporting`), those that recur
/// across requirements first, without the words nearly every requirement shares. Each is 4 or more lower-case letters or
/// digits; at most 200; the same requirements always give the same words in the same order.
pub fn focus_terms(requirements: &[RequirementItem]) -> Vec<String> {
    let lists: Vec<Vec<Term>> = requirements
        .iter()
        .filter(|r| !matches!(reach(r.kind), Reach::Nothing))
        .map(|r| terms_of(&r.text))
        .collect();
    let mut df: BTreeMap<String, usize> = BTreeMap::new();
    for terms in &lists {
        for t in terms {
            *df.entry(t.stem.clone()).or_insert(0) += 1;
        }
    }
    let n = lists.len();
    let mut kept: Vec<(String, usize)> = df
        .into_iter()
        .filter(|(stem, count)| {
            stem.chars().count() >= MIN_FOCUS_TERM_CHARS
                && stem
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                && (n < FOCUS_COMMON_FROM || (*count as f32) / (n as f32) <= FOCUS_TOO_COMMON)
        })
        .collect();
    kept.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    kept.into_iter()
        .take(MAX_FOCUS_TERMS)
        .map(|(stem, _)| stem)
        .collect()
}

/// What a reader of a report should know about the crawl behind it.
pub fn notes_for(evidence: &CrawlEvidence) -> Vec<String> {
    let mut notes = Vec::new();
    for role in evidence.roles.iter().filter(|r| !r.login_succeeded) {
        notes.push(format!(
            "Role {:?} could not log in, so its pages are only what was reachable before that.",
            role.role
        ));
    }
    if evidence.source == "result" {
        notes.push(
            "This crawl was made before form fields were recorded, so only its links and buttons were compared: crawl again \
             to include the fields."
                .to_string(),
        );
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounding::{GroundedElement, GroundedPage, GroundedRole};

    // ------------------------------------------------------------------ fixtures

    fn el(kind: &str, text: &str, hint: &str) -> ElementEvidence {
        ElementEvidence {
            kind: kind.to_string(),
            text: text.to_string(),
            hint: hint.to_string(),
            href: None,
        }
    }

    fn page(url: &str, title: &str, elements: Vec<ElementEvidence>) -> PageEvidence {
        PageEvidence {
            url: url.to_string(),
            title: title.to_string(),
            headings: Vec::new(),
            elements,
            roles: vec!["standard".to_string()],
        }
    }

    /// The pages of a small shop's crawl, shaped like a real one (a public demo shop): a login page, the product list, the
    /// cart, the first checkout step, and a page nobody wrote a requirement for.
    fn shop() -> CrawlEvidence {
        let products = [
            ("Sauce Labs Backpack", "sauce-labs-backpack"),
            ("Sauce Labs Bike Light", "sauce-labs-bike-light"),
            ("Sauce Labs Bolt T-Shirt", "sauce-labs-bolt-t-shirt"),
            ("Sauce Labs Fleece Jacket", "sauce-labs-fleece-jacket"),
        ];
        let mut inventory = vec![
            el("button", "Open Menu", "#react-burger-menu-btn"),
            el(
                "link",
                "All Items",
                "[data-test=\"inventory-sidebar-link\"]",
            ),
            el("link", "About", "[data-test=\"about-sidebar-link\"]"),
            el("link", "Logout", "[data-test=\"logout-sidebar-link\"]"),
            el(
                "link",
                "Reset App State",
                "[data-test=\"reset-sidebar-link\"]",
            ),
            el("select", "", "[data-test=\"product-sort-container\"]"),
            el("link", "", "[data-test=\"shopping-cart-link\"]"),
        ];
        for (name, slug) in products {
            inventory.push(el(
                "link",
                name,
                &format!("[data-test=\"item-{slug}-title-link\"]"),
            ));
            inventory.push(el(
                "button",
                "Add to cart",
                &format!("[data-test=\"add-to-cart-{slug}\"]"),
            ));
        }
        CrawlEvidence {
            source: "grounding",
            roles: vec![RoleEvidence {
                role: "standard".to_string(),
                login_succeeded: true,
                pages: 5,
            }],
            pages: vec![
                page(
                    "https://www.saucedemo.com/",
                    "Swag Labs",
                    vec![
                        el("input", "Username", "[data-test=\"username\"]"),
                        el("input", "Password", "[data-test=\"password\"]"),
                        el("button", "Login", "[data-test=\"login-button\"]"),
                    ],
                ),
                page(
                    "https://www.saucedemo.com/inventory.html",
                    "Swag Labs",
                    inventory,
                ),
                page(
                    "https://www.saucedemo.com/cart.html",
                    "Swag Labs",
                    vec![
                        el(
                            "button",
                            "Continue Shopping",
                            "[data-test=\"continue-shopping\"]",
                        ),
                        el("button", "Checkout", "[data-test=\"checkout\"]"),
                        el(
                            "button",
                            "Remove",
                            "[data-test=\"remove-sauce-labs-backpack\"]",
                        ),
                        el(
                            "link",
                            "Sauce Labs Backpack",
                            "[data-test=\"item-4-title-link\"]",
                        ),
                    ],
                ),
                page(
                    "https://www.saucedemo.com/checkout-step-one.html",
                    "Swag Labs",
                    vec![
                        el("input", "First Name", "[data-test=\"firstName\"]"),
                        el("input", "Last Name", "[data-test=\"lastName\"]"),
                        el("input", "Zip/Postal Code", "[data-test=\"postalCode\"]"),
                        el("button", "Cancel", "[data-test=\"cancel\"]"),
                        el("button", "Continue", "[data-test=\"continue\"]"),
                    ],
                ),
                page(
                    "https://www.saucedemo.com/legal/terms.html",
                    "Terms of Service",
                    vec![el("link", "Privacy Policy", "a")],
                ),
            ],
        }
    }

    fn req(key: &str, kind: RequirementKind, text: &str) -> RequirementItem {
        RequirementItem {
            key: key.to_string(),
            section: None,
            kind,
            text: text.to_string(),
        }
    }

    fn shop_requirements() -> Vec<RequirementItem> {
        use RequirementKind::*;
        vec![
            req(
                "FR-01",
                Security,
                "The user shall be able to log in with a username and password.",
            ),
            req(
                "FR-02",
                Functional,
                "The system shall show the list of products, each with its name.",
            ),
            req("FR-03", Functional, "A user can add a product to the shopping cart."),
            req("FR-04", Functional, "The user can remove an item from the cart."),
            req(
                "FR-05",
                Functional,
                "The user can complete the checkout by giving first name, last name and postal code.",
            ),
            req("FR-06", Functional, "The user shall be able to log out."),
            req(
                "FR-07",
                Functional,
                "Users can sort the product list by price or by name.",
            ),
            req("FR-08", Functional, "Users can add products to a wishlist."),
            req(
                "FR-09",
                Functional,
                "The system shall export any report as a CSV file.",
            ),
            req("FR-10", Performance, "Pages shall load within 2 seconds."),
            req("FR-11", Context, "The shop sells promotional items."),
            req(
                "FR-12",
                Security,
                "Accounts shall be locked after five failed sign-in attempts.",
            ),
        ]
    }

    fn finding<'a>(analysis: &'a Analysis, key: &str) -> &'a Finding {
        analysis
            .findings
            .iter()
            .find(|f| f.key == key)
            .unwrap_or_else(|| panic!("no finding {key}"))
    }

    fn stems(text: &str) -> Vec<String> {
        terms_of(text).into_iter().map(|t| t.stem).collect()
    }

    // --------------------------------------------------------------------- words

    #[test]
    fn sign_in_log_in_and_login_are_one_word_and_so_are_the_ways_of_saying_the_rest() {
        assert_eq!(stems("sign in"), stems("Log-in"));
        assert_eq!(stems("login"), stems("Sign In"));
        assert_eq!(stems("logon"), vec!["login"]);
        assert_eq!(stems("sign out"), stems("logout"));
        assert_eq!(stems("Sign up"), stems("Registration"));
        assert_eq!(stems("basket"), stems("shopping cart"));
        assert_eq!(stems("user name"), stems("usernames"));
        assert_eq!(stems("e-mail"), vec!["email"]);
        assert_eq!(stems("remove"), stems("Deleting"));
        assert_eq!(stems("Modify"), stems("updating"));
        assert_eq!(stems("check out"), stems("Purchase"));
    }

    #[test]
    fn endings_are_stemmed_so_a_requirements_word_and_a_pages_word_are_the_same() {
        assert_eq!(stem("reports"), "report");
        assert_eq!(stem("reporting"), "report");
        assert_eq!(stem("reported"), "report");
        assert_eq!(stem("categories"), "category");
        assert_eq!(stem("addresses"), "address");
        assert_eq!(stem("boxes"), "box");
        assert_eq!(stem("logging"), "log");
        assert_eq!(stem("adding"), "add");
        assert_eq!(stem("created"), stem("create"));
        assert_eq!(stem("creation"), stem("create"));
        assert_eq!(stem("validation"), stem("validate"));
        assert_eq!(stem("status"), "status");
        assert_eq!(stem("analysis"), "analysis");
        assert_eq!(stem("process"), "process");
        // A word already short is left alone, and so is a word that is not ASCII.
        assert_eq!(stem("cart"), "cart");
        assert_eq!(stem("पंजीकरण"), "पंजीकरण");
    }

    #[test]
    fn filler_words_digits_and_short_words_are_not_worth_comparing() {
        assert_eq!(
            stems("The system shall be able to allow users to view the 5 items"),
            vec!["item"]
        );
        assert!(stems("It is so, or 42, or a.").is_empty());
    }

    #[test]
    fn words_are_split_at_punctuation_and_inside_camel_case_and_hyphens() {
        assert_eq!(stems("addToCart"), vec!["add", "cart"]);
        // (What is compared is the stem: `sauce` and `labs` lose their endings.)
        assert_eq!(
            stems("add-to-cart-sauce-labs-backpack"),
            vec!["add", "cart", "sauc", "lab", "backpack"]
        );
        assert_eq!(stems("Zip/Postal Code"), vec!["zip", "postal", "code"]);
    }

    #[test]
    fn a_locators_markup_words_are_dropped_and_its_feature_words_kept() {
        let words: Vec<String> = locator_terms("[data-test=\"product-sort-container\"]")
            .into_iter()
            .map(|t| t.shown)
            .collect();
        assert_eq!(words, vec!["product", "sort"]);
        assert!(locator_terms("div.container > span:nth-child(2)").is_empty());
    }

    #[test]
    fn an_addresss_path_gives_its_words_and_its_file_extension_none() {
        let words: Vec<String> =
            address_terms("https://shop.example.com/reports/monthly-summary.html?page=2#top")
                .into_iter()
                .map(|t| t.shown)
                .collect();
        assert_eq!(words, vec!["reports", "monthly", "summary"]);
        assert!(address_terms("https://shop.example.com/").is_empty());
        assert!(address_terms("https://shop.example.com/index.php").is_empty());
    }

    // ------------------------------------------------------------------ evidence

    fn grounded(role: &str, ok: bool, pages: Vec<GroundedPage>) -> GroundedRole {
        GroundedRole {
            role: role.to_string(),
            login_succeeded: ok,
            login_error: None,
            login_uses_literals: false,
            login_actions: Vec::new(),
            pages,
        }
    }

    fn grounded_page(
        url: &str,
        title: &str,
        headings: &[&str],
        elements: &[(&str, &str, &str)],
    ) -> GroundedPage {
        GroundedPage {
            url: url.to_string(),
            title: title.to_string(),
            headings: headings.iter().map(|h| h.to_string()).collect(),
            elements: elements
                .iter()
                .map(|(kind, text, locator)| GroundedElement {
                    kind: kind.to_string(),
                    text: text.to_string(),
                    locator: locator.to_string(),
                    href: None,
                    input_type: None,
                })
                .collect(),
        }
    }

    #[test]
    fn a_page_two_roles_reached_is_one_page_carrying_what_either_saw() {
        let crawl = CrawlGrounding {
            roles: vec![
                grounded(
                    "admin",
                    true,
                    vec![grounded_page(
                        "https://app.example.com/settings/",
                        "Settings",
                        &["Team"],
                        &[
                            ("button", "Invite", "#invite"),
                            ("link", "Billing", "#billing"),
                        ],
                    )],
                ),
                grounded(
                    "user",
                    true,
                    vec![grounded_page(
                        "https://APP.example.com/settings#top",
                        "",
                        &["Team", "Profile"],
                        &[("link", "Billing", "#billing"), ("button", "Save", "#save")],
                    )],
                ),
            ],
        };
        let evidence = evidence_from_grounding(&crawl, None).unwrap();
        assert_eq!(evidence.source, "grounding");
        assert_eq!(
            evidence.pages.len(),
            1,
            "one address, one page: {evidence:?}"
        );
        let page = &evidence.pages[0];
        assert_eq!(page.url, "https://app.example.com/settings");
        assert_eq!(page.title, "Settings");
        assert_eq!(page.roles, vec!["admin", "user"]);
        assert_eq!(page.headings, vec!["Team", "Profile"]);
        assert_eq!(
            page.elements
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Invite", "Billing", "Save"]
        );
        assert_eq!(evidence.roles.len(), 2);
        assert_eq!(evidence.roles[1].pages, 1);
    }

    #[test]
    fn a_page_first_seen_with_no_title_takes_the_title_another_role_saw_and_keeps_one_it_has() {
        let crawl = CrawlGrounding {
            roles: vec![
                grounded(
                    "admin",
                    true,
                    vec![grounded_page(
                        "https://app.example.com/reports",
                        "",
                        &[],
                        &[],
                    )],
                ),
                grounded(
                    "user",
                    true,
                    vec![
                        grounded_page("https://app.example.com/reports", "Reports", &[], &[]),
                        grounded_page("https://app.example.com/help", "Help", &[], &[]),
                    ],
                ),
                grounded(
                    "guest",
                    true,
                    vec![grounded_page(
                        "https://app.example.com/help",
                        "Help centre",
                        &[],
                        &[],
                    )],
                ),
            ],
        };
        let evidence = evidence_from_grounding(&crawl, None).unwrap();
        let title_of = |path: &str| {
            evidence
                .pages
                .iter()
                .find(|p| p.url.ends_with(path))
                .unwrap()
                .title
                .clone()
        };
        assert_eq!(title_of("/reports"), "Reports");
        assert_eq!(
            title_of("/help"),
            "Help",
            "a title already there is not replaced"
        );
    }

    #[test]
    fn a_role_can_be_chosen_by_name_and_an_unknown_one_lists_the_roles() {
        let crawl = CrawlGrounding {
            roles: vec![
                grounded(
                    "admin",
                    true,
                    vec![grounded_page("http://a.test/x", "X", &[], &[])],
                ),
                grounded(
                    "user",
                    false,
                    vec![grounded_page("http://a.test/y", "Y", &[], &[])],
                ),
            ],
        };
        let only_user = evidence_from_grounding(&crawl, Some(" USER ")).unwrap();
        assert_eq!(only_user.pages.len(), 1);
        assert_eq!(only_user.pages[0].url, "http://a.test/y");
        assert_eq!(only_user.roles.len(), 1);
        assert!(!only_user.roles[0].login_succeeded);
        let err = evidence_from_grounding(&crawl, Some("root")).unwrap_err();
        assert!(
            err.contains("\"root\"") && err.contains("admin, user"),
            "{err}"
        );
        // An empty name means every role.
        assert_eq!(
            evidence_from_grounding(&crawl, Some("  "))
                .unwrap()
                .pages
                .len(),
            2
        );
    }

    #[test]
    fn an_older_crawls_result_gives_its_links_and_buttons() {
        let output: CrawlOutput = serde_json::from_str(
            r##"{"roles":[{"role":"anonymous","loginSucceeded":true,"pages":[
                {"url":"https://x.test/","title":"Home","elements":[
                  {"kind":"link","text":"Pricing","selectorHint":"#pricing","href":"https://x.test/pricing"},
                  {"kind":"button","text":"Sign up","selectorHint":".cta"}]}]}],
              "visibilityMatrix":[]}"##,
        )
        .unwrap();
        let evidence = evidence_from_result(&output, None).unwrap();
        assert_eq!(evidence.source, "result");
        assert_eq!(evidence.pages.len(), 1);
        assert_eq!(evidence.pages[0].elements.len(), 2);
        assert_eq!(
            evidence.pages[0].elements[0].href.as_deref(),
            Some("https://x.test/pricing")
        );
        assert!(evidence_from_result(&output, Some("admin")).is_err());
    }

    #[test]
    fn headings_are_evidence_too() {
        let crawl = CrawlGrounding {
            roles: vec![grounded(
                "user",
                true,
                vec![grounded_page(
                    "https://x.test/p/42",
                    "App",
                    &["Invoices"],
                    &[],
                )],
            )],
        };
        let evidence = evidence_from_grounding(&crawl, None).unwrap();
        let analysis = analyze(
            &[req(
                "R-1",
                RequirementKind::Functional,
                "Users can see their invoices.",
            )],
            &evidence,
            &[],
        );
        assert_eq!(finding(&analysis, "R-1").status, Status::Found);
    }

    // ------------------------------------------------------------------- matching

    #[test]
    fn each_requirement_of_the_shop_is_lined_up_with_the_page_that_carries_it() {
        let analysis = analyze(&shop_requirements(), &shop(), &[]);

        // The words are all on the login page.
        let f = finding(&analysis, "FR-01");
        assert_eq!(f.status, Status::Found);
        assert_eq!(f.basis, Some(Basis::Text));
        assert_eq!(f.matches[0].url, "https://www.saucedemo.com/");
        assert_eq!(f.matches[0].score, 1.0);
        assert_eq!(f.matches[0].terms.len(), 3);
        assert_eq!(f.matches[0].elements, vec!["Username", "Password", "Login"]);

        // Adding to the cart happens on the product list; removing from it, on the cart.
        assert_eq!(finding(&analysis, "FR-03").status, Status::Found);
        assert_eq!(
            finding(&analysis, "FR-03").matches[0].url,
            "https://www.saucedemo.com/inventory.html"
        );
        assert_eq!(
            finding(&analysis, "FR-03").matches[0].elements,
            vec!["Add to cart"]
        );
        assert_eq!(finding(&analysis, "FR-04").status, Status::Found);
        assert_eq!(
            finding(&analysis, "FR-04").matches[0].url,
            "https://www.saucedemo.com/cart.html"
        );
        assert_eq!(
            finding(&analysis, "FR-04").matches[0].elements,
            vec!["Remove"]
        );

        // "log out" is the menu's Logout link; the checkout step names the fields and the address says "checkout".
        assert_eq!(finding(&analysis, "FR-06").status, Status::Found);
        assert_eq!(finding(&analysis, "FR-05").status, Status::Found);
        assert_eq!(
            finding(&analysis, "FR-05").matches[0].url,
            "https://www.saucedemo.com/checkout-step-one.html"
        );
    }

    #[test]
    fn a_requirement_only_partly_on_a_page_is_weak_not_found() {
        let analysis = analyze(&shop_requirements(), &shop(), &[]);
        // "add products to a wishlist": the shop has "add" and products, and no wishlist. Its rare word decides.
        let wishlist = finding(&analysis, "FR-08");
        assert_eq!(
            wishlist.status,
            Status::Weak,
            "matches: {:?}",
            wishlist.matches
        );
        assert!(wishlist.matches[0].terms.contains(&"add".to_string()));
        assert!(!wishlist.matches[0].terms.contains(&"wishlist".to_string()));
        // Sorting is what the product list's sort control is for, but nothing on the page says "price".
        assert_eq!(finding(&analysis, "FR-07").status, Status::Weak);
        assert_eq!(finding(&analysis, "FR-02").status, Status::Weak);
    }

    #[test]
    fn a_requirement_no_page_carries_is_missing() {
        let analysis = analyze(&shop_requirements(), &shop(), &[]);
        let export = finding(&analysis, "FR-09");
        assert_eq!(export.status, Status::Missing);
        assert!(export.matches.is_empty());
        assert_eq!(export.basis, None);
    }

    #[test]
    fn what_a_crawl_cannot_show_is_not_checkable_rather_than_missing() {
        let analysis = analyze(&shop_requirements(), &shop(), &[]);
        // A response time, background text, and a lock-out rule (a security requirement whose words the login page only
        // half carries) are never reported as missing.
        assert_eq!(finding(&analysis, "FR-10").status, Status::NotCheckable);
        assert_eq!(finding(&analysis, "FR-11").status, Status::NotCheckable);
        assert_eq!(finding(&analysis, "FR-12").status, Status::NotCheckable);
        assert!(finding(&analysis, "FR-10").matches.is_empty());
        // A security requirement the pages do show is found like any other.
        assert_eq!(finding(&analysis, "FR-01").status, Status::Found);
    }

    #[test]
    fn a_requirement_with_only_filler_words_cannot_be_lined_up_with_anything() {
        let analysis = analyze(
            &[req("R-1", RequirementKind::Functional, "It shall be so.")],
            &shop(),
            &[],
        );
        assert_eq!(finding(&analysis, "R-1").status, Status::NotCheckable);
    }

    #[test]
    fn a_word_many_requirements_share_says_less_than_a_word_only_one_has() {
        // The same page carries "add" and "cart"; the requirement that also wants a rare word is not found there.
        let evidence = CrawlEvidence {
            source: "grounding",
            roles: Vec::new(),
            pages: vec![page(
                "https://x.test/shop",
                "Shop",
                vec![el("button", "Add to cart", "#add")],
            )],
        };
        let requirements = vec![
            req(
                "R-1",
                RequirementKind::Functional,
                "Add an item to the cart.",
            ),
            req(
                "R-2",
                RequirementKind::Functional,
                "Remove an item from the cart.",
            ),
            req(
                "R-3",
                RequirementKind::Functional,
                "Add an item to the cart and pay by voucher.",
            ),
        ];
        let analysis = analyze(&requirements, &evidence, &[]);
        assert_eq!(finding(&analysis, "R-1").status, Status::Found);
        assert_eq!(finding(&analysis, "R-3").status, Status::Weak);
    }

    #[test]
    fn the_page_a_word_is_in_the_address_of_outranks_a_page_that_only_links_to_it() {
        // Every page has a "Reports" link in its menu, but only one is the reports page.
        let menu = || vec![el("link", "Reports", "#nav-reports")];
        let evidence = CrawlEvidence {
            source: "grounding",
            roles: Vec::new(),
            pages: vec![
                page("https://x.test/home", "App", menu()),
                page("https://x.test/reports", "App", menu()),
                page("https://x.test/help", "App", menu()),
            ],
        };
        let analysis = analyze(
            &[req(
                "R-1",
                RequirementKind::Functional,
                "Users can open the reports.",
            )],
            &evidence,
            &[],
        );
        let f = finding(&analysis, "R-1");
        assert_eq!(f.status, Status::Found);
        assert_eq!(f.matches[0].url, "https://x.test/reports");
    }

    #[test]
    fn at_most_three_pages_are_kept_and_only_those_close_to_the_best() {
        let same = || vec![el("button", "Export", "#export")];
        let evidence = CrawlEvidence {
            source: "grounding",
            roles: Vec::new(),
            pages: (1..=6)
                .map(|i| page(&format!("https://x.test/p{i}"), "Page", same()))
                .collect(),
        };
        let analysis = analyze(
            &[req("R-1", RequirementKind::Functional, "Users can export.")],
            &evidence,
            &[],
        );
        assert_eq!(finding(&analysis, "R-1").matches.len(), MAX_MATCHES);
    }

    #[test]
    fn the_same_input_gives_the_same_report() {
        let a = analyze(&shop_requirements(), &shop(), &[]).report();
        let b = analyze(&shop_requirements(), &shop(), &[]).report();
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn a_crawl_with_no_pages_leaves_every_checkable_requirement_missing() {
        let empty = CrawlEvidence {
            source: "result",
            roles: Vec::new(),
            pages: Vec::new(),
        };
        let report = analyze(&shop_requirements(), &empty, &[]).report();
        assert_eq!(report.summary.missing, 8);
        assert_eq!(report.summary.found + report.summary.weak, 0);
        assert!(report.undocumented_pages.is_empty());
    }

    #[test]
    fn no_requirements_give_an_empty_report_and_every_page_is_undocumented() {
        let report = analyze(&[], &shop(), &[]).report();
        assert_eq!(report.summary.requirements, 0);
        assert_eq!(report.undocumented_pages.len(), 5);
    }

    // ------------------------------------------------------------------- pages

    #[test]
    fn a_page_no_requirement_points_at_is_listed_as_not_in_the_documents() {
        let report = analyze(&shop_requirements(), &shop(), &[]).report();
        assert_eq!(report.undocumented_pages.len(), 1);
        let page = &report.undocumented_pages[0];
        assert_eq!(page.url, "https://www.saucedemo.com/legal/terms.html");
        assert_eq!(page.title, "Terms of Service");
        assert_eq!(page.elements, vec!["Privacy Policy"]);
        assert_eq!(page.element_count, 1);
        assert_eq!(page.roles, vec!["standard"]);
        assert_eq!(report.summary.pages, 5);
        assert_eq!(report.summary.undocumented_pages, 1);
    }

    #[test]
    fn a_page_only_a_weak_match_points_at_is_still_documented() {
        let analysis = analyze(&shop_requirements(), &shop(), &[]);
        let report = analysis.report();
        assert!(
            !report
                .undocumented_pages
                .iter()
                .any(|p| p.url.ends_with("/inventory.html")),
            "the product list is what FR-02, FR-03, FR-07 and FR-08 are about"
        );
    }

    // ------------------------------------------------------------------- cases

    fn case(key: Option<&str>, status: TestCaseStatus, technique: Technique) -> CaseInfo {
        CaseInfo {
            requirement_key: key.map(str::to_string),
            status,
            technique,
        }
    }

    #[test]
    fn a_requirements_cases_are_counted_by_status_and_technique_and_rejected_ones_not_at_all() {
        let cases = vec![
            case(Some("FR-03"), TestCaseStatus::Approved, Technique::Positive),
            case(Some("FR-03"), TestCaseStatus::Approved, Technique::Negative),
            case(Some("FR-03"), TestCaseStatus::Executed, Technique::Negative),
            case(
                Some("FR-03"),
                TestCaseStatus::Generated,
                Technique::Positive,
            ),
            case(Some("FR-03"), TestCaseStatus::Rejected, Technique::Edge),
            case(Some("FR-99"), TestCaseStatus::Approved, Technique::Positive),
            case(None, TestCaseStatus::Approved, Technique::Positive),
        ];
        let analysis = analyze(&shop_requirements(), &shop(), &cases);
        let c = &finding(&analysis, "FR-03").cases;
        // Two approved, one executed, one still only generated: not a count that is the same for every status.
        assert_eq!((c.total, c.approved, c.executed), (4, 2, 1));
        assert_eq!(c.techniques.get("positive"), Some(&2));
        assert_eq!(c.techniques.get("negative"), Some(&2));
        assert_eq!(
            c.techniques.get("edge"),
            None,
            "a rejected case is no coverage"
        );
        // Positive, negative and edge apply to a functional requirement; edge is the one still missing.
        assert_eq!(c.missing_techniques, vec!["edge"]);
        // A requirement no case names has none, and every applicable technique is missing.
        let none = &finding(&analysis, "FR-04").cases;
        assert_eq!(none.total, 0);
        assert_eq!(
            none.missing_techniques,
            vec!["positive", "negative", "edge"]
        );
        // Background text has no technique that applies.
        assert!(finding(&analysis, "FR-11")
            .cases
            .missing_techniques
            .is_empty());
    }

    #[test]
    fn the_summary_says_what_is_shown_untested_and_what_is_tested_but_not_shown() {
        let cases = vec![
            case(Some("FR-03"), TestCaseStatus::Approved, Technique::Positive), // shown, tested
            case(Some("FR-09"), TestCaseStatus::Approved, Technique::Positive), // missing, tested
            case(
                Some("FR-10"),
                TestCaseStatus::Approved,
                Technique::Performance,
            ), // not checkable, tested
        ];
        let report = analyze(&shop_requirements(), &shop(), &cases).report();
        let s = &report.summary;
        assert_eq!(s.requirements, 12);
        assert_eq!(s.checkable, 8);
        assert_eq!((s.found, s.weak, s.missing, s.not_checkable), (5, 3, 1, 3));
        assert_eq!(s.cases, 3);
        assert_eq!(s.with_cases, 3);
        // Eight requirements are found or weak; one of them (FR-03) has a case.
        assert_eq!(s.shown_without_cases, 7);
        assert_eq!(s.tested_without_evidence, 1);
    }

    fn row(key: Option<&str>, tags: &[&str], status: TestCaseStatus) -> TestCaseRow {
        let now = chrono::Utc::now();
        TestCaseRow {
            id: uuid::Uuid::new_v4(),
            project_id: "p".to_string(),
            title: "t".to_string(),
            description: String::new(),
            category: "Functional".to_string(),
            priority: "HIGH".to_string(),
            severity: "MAJOR".to_string(),
            preconditions: Vec::new(),
            steps: Vec::new(),
            expected_result: String::new(),
            assertions: Vec::new(),
            test_data: None,
            source_document: None,
            requirement_reference: key.map(str::to_string),
            source_requirement: None,
            traceability: None,
            risk_factor: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            source: "ai".to_string(),
            status,
            validation_notes: None,
            executed_run_id: None,
            executed_task_id: None,
            created_by: None,
            created_at: now,
            updated_at: now,
            suite_id: None,
        }
    }

    #[test]
    fn a_saved_case_names_its_requirement_and_its_technique_through_its_reference_and_tags() {
        let with_tag = CaseInfo::from(&row(
            Some(" FR-03 "),
            &["functional", "technique:negative"],
            TestCaseStatus::Approved,
        ));
        assert_eq!(with_tag.requirement_key.as_deref(), Some("FR-03"));
        assert_eq!(with_tag.technique, Technique::Negative);
        // No technique label: it counts as positive, the same convention a generation run uses.
        let plain = CaseInfo::from(&row(Some("FR-03"), &[], TestCaseStatus::Generated));
        assert_eq!(plain.technique, Technique::Positive);
        // A case tied to no requirement, or to a blank one, names none.
        assert_eq!(
            CaseInfo::from(&row(None, &[], TestCaseStatus::Approved)).requirement_key,
            None
        );
        assert_eq!(
            CaseInfo::from(&row(Some("  "), &[], TestCaseStatus::Approved)).requirement_key,
            None
        );
    }

    // ------------------------------------------------------------------- report

    #[test]
    fn a_long_requirement_is_cut_in_the_report_and_the_report_serializes_as_the_api_sends_it() {
        let long = format!("Users can export the report. {}", "Detail. ".repeat(100));
        let report = analyze(
            &[req("R-1", RequirementKind::Functional, &long)],
            &shop(),
            &[],
        )
        .report();
        assert!(report.requirements[0].text.chars().count() <= MAX_TEXT_CHARS + 1);
        assert!(report.requirements[0].text.ends_with('…'));

        let json =
            serde_json::to_value(analyze(&shop_requirements(), &shop(), &[]).report()).unwrap();
        assert_eq!(json["requirements"][0]["key"], "FR-01");
        assert_eq!(json["requirements"][0]["status"], "FOUND");
        assert_eq!(json["requirements"][0]["basis"], "text");
        assert_eq!(json["requirements"][0]["kind"], "security");
        assert_eq!(json["requirements"][8]["status"], "MISSING");
        assert!(json["requirements"][8].get("basis").is_none());
        assert_eq!(json["requirements"][9]["status"], "NOT_CHECKABLE");
        assert_eq!(
            json["requirements"][0]["matches"][0]["terms"][0],
            "password"
        );
        assert!(json["requirements"][0]["matches"][0]
            .get("similarity")
            .is_none());
        assert_eq!(json["summary"]["found"], 5);
        assert_eq!(json["undocumented_pages"][0]["element_count"], 1);
    }

    #[test]
    fn the_notes_say_which_role_could_not_log_in_and_when_the_crawl_has_no_fields() {
        let mut evidence = shop();
        assert!(notes_for(&evidence).is_empty());
        evidence.roles.push(RoleEvidence {
            role: "admin".to_string(),
            login_succeeded: false,
            pages: 1,
        });
        evidence.source = "result";
        let notes = notes_for(&evidence);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(
            notes[0].contains("\"admin\"") && notes[0].contains("could not log in"),
            "{notes:?}"
        );
        assert!(notes[1].contains("form fields"), "{notes:?}");
    }

    // ------------------------------------------------------- more, each pinning one rule

    fn one_page_crawl(title: &str, elements: Vec<ElementEvidence>) -> CrawlEvidence {
        CrawlEvidence {
            source: "grounding",
            roles: Vec::new(),
            pages: vec![page("https://x.test/p/42", title, elements)],
        }
    }

    fn check_one(text: &str, evidence: &CrawlEvidence) -> Analysis {
        analyze(
            &[req("R-1", RequirementKind::Functional, text)],
            evidence,
            &[],
        )
    }

    #[test]
    fn a_crawl_reads_at_most_a_bounded_number_of_pages() {
        let pages = (0..350)
            .map(|i| grounded_page(&format!("https://x.test/p{i}"), "P", &[], &[]))
            .collect();
        let crawl = CrawlGrounding {
            roles: vec![grounded("user", true, pages)],
        };
        assert_eq!(
            evidence_from_grounding(&crawl, None).unwrap().pages.len(),
            MAX_PAGES
        );
    }

    #[test]
    fn a_word_is_listed_once_and_a_number_is_not_a_word() {
        assert_eq!(stems("cart cart Cart carts"), vec!["cart"]);
        assert_eq!(stems("Version 2026 items"), vec!["version", "item"]);
    }

    #[test]
    fn a_word_only_the_title_carries_counts() {
        let analysis = check_one(
            "Users can see their invoices.",
            &one_page_crawl("Invoices", vec![]),
        );
        assert_eq!(finding(&analysis, "R-1").status, Status::Found);
    }

    #[test]
    fn a_word_only_a_locator_or_a_links_target_carries_counts_a_little_less() {
        // No visible label: the locator says what it is for.
        let by_locator = one_page_crawl(
            "App",
            vec![el("button", "", "[data-test=\"export-report\"]")],
        );
        let analysis = check_one("Users can export the report.", &by_locator);
        assert_eq!(finding(&analysis, "R-1").status, Status::Found);
        assert!(finding(&analysis, "R-1").matches[0].score < 1.0);
        // A link called "Basket" that leads to the monthly reports.
        let mut link = el("link", "Basket", "#basket");
        link.href = Some("https://x.test/reports/monthly".to_string());
        let by_target = one_page_crawl("App", vec![link]);
        let analysis = check_one("Users can view the monthly reports.", &by_target);
        assert_eq!(finding(&analysis, "R-1").status, Status::Found);
    }

    #[test]
    fn an_address_word_counts_no_more_than_a_label_word_towards_found() {
        // "reports" is in the address; "export" is nowhere. Half of what the requirement says is there.
        let evidence = CrawlEvidence {
            source: "grounding",
            roles: Vec::new(),
            pages: vec![page("https://x.test/reports", "App", vec![])],
        };
        let analysis = check_one("Users can export reports.", &evidence);
        assert_eq!(finding(&analysis, "R-1").status, Status::Weak);
        assert_eq!(finding(&analysis, "R-1").matches[0].score, 0.5);
    }

    #[test]
    fn only_pages_close_to_the_best_one_are_kept_as_matches() {
        let evidence = CrawlEvidence {
            source: "grounding",
            roles: Vec::new(),
            pages: vec![
                page(
                    "https://x.test/a",
                    "App",
                    vec![el("button", "Export report", "#a")],
                ),
                page(
                    "https://x.test/b",
                    "App",
                    vec![el("button", "Export", "#b")],
                ),
            ],
        };
        let analysis = check_one("Users can export a report.", &evidence);
        let f = finding(&analysis, "R-1");
        assert_eq!(f.matches.len(), 1, "{:?}", f.matches);
        assert_eq!(f.matches[0].url, "https://x.test/a");
    }

    #[test]
    fn matched_elements_are_listed_most_telling_first_without_repeats_and_at_most_three() {
        let evidence = one_page_crawl(
            "App",
            vec![
                el("button", "Report", "#1"),
                el("button", "Export report", "#2"),
                el("button", "export REPORT", "#3"),
                el("button", "Export", "#4"),
                el("button", "Reports", "#5"),
                el("button", "", "#6"),
            ],
        );
        let analysis = check_one("Users can export a report.", &evidence);
        assert_eq!(
            finding(&analysis, "R-1").matches[0].elements,
            vec!["Export report", "Report", "Export"]
        );
    }

    #[test]
    fn a_page_no_requirement_points_at_lists_a_few_distinct_things_on_it() {
        let mut elements = vec![
            el("link", "Privacy", "a"),
            el("link", "privacy", "a"),
            el("link", "PRIVACY", "a"),
            el("link", "", "a"),
        ];
        for i in 0..8 {
            elements.push(el("link", &format!("Topic {i}"), "a"));
        }
        let report = analyze(&[], &one_page_crawl("Legal", elements), &[]).report();
        let page = &report.undocumented_pages[0];
        assert_eq!(page.elements.len(), MAX_UNDOCUMENTED_ELEMENTS);
        assert_eq!(page.elements[0], "Privacy");
        assert_eq!(
            page.elements[1], "Topic 0",
            "repeats are listed once: {:?}",
            page.elements
        );
        assert_eq!(
            page.element_count, 11,
            "the blank one is not counted: only what has a name"
        );
    }

    #[test]
    fn a_text_of_exactly_the_limit_is_not_cut_and_a_longer_one_is() {
        assert_eq!(truncate_chars("abcde", 5), "abcde");
        assert_eq!(truncate_chars("abcdef", 5), "abcde…");
        assert_eq!(
            truncate_chars("abc def", 4),
            "abc…",
            "trailing space is not kept before the mark"
        );
    }

    #[test]
    fn scores_are_rounded_to_two_decimals() {
        let report = analyze(&shop_requirements(), &shop(), &[]).report();
        let mut seen = 0;
        for f in &report.requirements {
            for m in &f.matches {
                seen += 1;
                let scaled = m.score * 100.0;
                assert!(
                    (scaled.round() - scaled).abs() < 1e-3,
                    "{} scored {}",
                    f.key,
                    m.score
                );
            }
        }
        assert!(seen > 5);
    }

    // ------------------------------------------------- words to look for first (Phase 6C)

    #[test]
    fn a_crawl_is_told_to_look_first_for_the_words_the_requirements_recur_on() {
        use RequirementKind::*;
        let requirements = vec![
            req("R-1", Functional, "Users can add a product to the cart."),
            req(
                "R-2",
                Functional,
                "Users can remove a product from the cart.",
            ),
            req("R-3", Functional, "Users can export the monthly report."),
            req("R-4", Functional, "Users can email the monthly report."),
            req("R-5", Functional, "Users can open the checkout."),
        ];
        // 5 requirements: cart, monthly, product and report are each in 2 (40%: not too common), the rest in 1 ("remove" is
        // "delete" to the words, and "add" is too short to look for).
        assert_eq!(
            focus_terms(&requirements),
            vec!["cart", "monthly", "product", "report", "checkout", "delete", "email", "export"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn words_nearly_every_requirement_shares_name_no_page_and_short_ones_match_too_much() {
        use RequirementKind::*;
        // Six requirements that all say "invoice": it names no page in particular. Three-letter words are left out.
        let mut requirements: Vec<RequirementItem> = (1..=6)
            .map(|i| {
                req(
                    &format!("R-{i}"),
                    Functional,
                    &format!(
                        "Users can send the invoice by {}.",
                        ["post", "fax", "courier", "hand", "van", "drone"][i - 1]
                    ),
                )
            })
            .collect();
        let terms = focus_terms(&requirements);
        assert!(!terms.contains(&"invoic".to_string()), "{terms:?}");
        assert!(
            terms.contains(&"post".to_string()) && terms.contains(&"courier".to_string()),
            "{terms:?}"
        );
        assert!(
            !terms.contains(&"fax".to_string()) && !terms.contains(&"van".to_string()),
            "{terms:?}"
        );
        // With fewer than five requirements nothing is "too common".
        requirements.truncate(4);
        assert!(focus_terms(&requirements).contains(&"invoic".to_string()));
    }

    #[test]
    fn only_requirements_a_crawl_can_speak_to_give_words_and_only_ascii_ones() {
        use RequirementKind::*;
        let requirements = vec![
            req("R-1", Performance, "Pages shall respond quickly under heavy traffic."),
            req("R-2", Context, "The shop sells umbrellas."),
            req("R-3", Interface, "The gateway exposes webhooks."),
            req("R-4", Functional, "Users can review the wishlist."),
            req("R-5", Security, "Users can change the password."),
            req("R-6", Functional, "\u{092a}\u{0902}\u{091c}\u{0940}\u{0915}\u{0930}\u{0923} \u{0915}\u{0930}\u{0947}\u{0902}"),
        ];
        let terms = focus_terms(&requirements);
        assert_eq!(
            terms,
            vec!["chang", "password", "review", "wishlist"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert!(focus_terms(&[req("R-1", Context, "Background only.")]).is_empty());
        assert!(focus_terms(&[]).is_empty());
    }

    #[test]
    fn a_word_in_most_of_five_requirements_names_no_page_but_one_in_two_does() {
        use RequirementKind::*;
        let with = |n: usize| -> Vec<RequirementItem> {
            (0..5)
                .map(|i| {
                    let word = if i < n { "wishlist" } else { "catalogue" };
                    req(
                        &format!("R-{i}"),
                        Functional,
                        &format!("Users can browse the {word} number{i}."),
                    )
                })
                .collect()
        };
        // In 2 of 5 (40%) "wishlist" is kept; in 3 of 5 (60%) it is not; the word in all five never is.
        assert!(focus_terms(&with(2)).contains(&"wishlist".to_string()));
        assert!(!focus_terms(&with(3)).contains(&"wishlist".to_string()));
        assert!(!focus_terms(&with(3)).contains(&"brows".to_string()));
    }

    #[test]
    fn at_most_two_hundred_words_and_always_the_same_ones() {
        use RequirementKind::*;
        let requirements: Vec<RequirementItem> = (0..250)
            .map(|i| {
                req(
                    &format!("R-{i}"),
                    Functional,
                    &format!("Users can frobnicate gadgetnumber{i} today."),
                )
            })
            .collect();
        let a = focus_terms(&requirements);
        assert_eq!(a.len(), 200);
        assert_eq!(a, focus_terms(&requirements));
        assert!(a.iter().all(|t| t.chars().count() >= 4
            && t.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())));
        // Every requirement shares "frobnicate" and "today": they are too common to name a page.
        assert!(!a.contains(&"frobnicat".to_string()) && !a.contains(&"today".to_string()));
    }
}
