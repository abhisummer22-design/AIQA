//! Splits one requirements document into individual, numbered requirements.
//!
//! A generation request used to paste each uploaded document into every AI
//! call as one block of text, cut at 20,000 characters. A long FRD lost
//! everything after that mark without saying so, every skill call re-read the
//! whole thing, and a generated case could only point back at a quoted phrase.
//! This module reads the whole document instead and cuts it into the units a
//! person would call "a requirement", each with an id, the section it sits in
//! and a kind (functional, security, performance, ...), so a later step can
//! give each AI call only the requirements that skill is about, in batches
//! that fit, and tie every case to the requirement it came from.
//!
//! It is deterministic on purpose: the same document always yields the same
//! requirements with the same ids, it needs no AI provider, and it costs
//! nothing to run. It reads the structure a document really has — Markdown or
//! Word headings, numbered headings (`3.2 Login`), explicit ids
//! (`FR-012: ...`), list items and table rows — and falls back to paragraphs.
//! Nothing that carries content is dropped silently: text that is neither a
//! heading nor a requirement-shaped line becomes a `context` item, and the
//! only things left out on purpose (a table of contents, a revision history,
//! running page headers) are counted and reported.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::OnceLock;

/// Bumped when the parsing rules change in a way that should make documents
/// that are already stored be parsed again.
pub const PARSER_VERSION: i64 = 1;

/// A single requirement longer than this is split at sentence boundaries, so
/// one giant paragraph can never make an AI call too big on its own.
const MAX_ITEM_CHARS: usize = 1_200;
const SPLIT_TARGET_CHARS: usize = 900;
/// A reference file (JSON/YAML) is cut into pieces of about this size.
const REFERENCE_CHUNK_CHARS: usize = 1_500;

macro_rules! regex {
    ($pat:expr) => {{
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new($pat).unwrap())
    }};
}

/// What a requirement is about. It decides which skills are shown it
/// (`ai_generation`'s skill scopes), never whether it is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    /// Behaviour the system must show.
    Functional,
    /// Authentication, permissions, protection of data and access.
    Security,
    /// Speed, load, capacity, availability.
    Performance,
    /// Accessibility, layout, devices, language.
    Usability,
    /// APIs, integrations, protocols, message formats.
    Interface,
    /// Input rules, formats, limits, storage.
    Data,
    /// Background and business narrative with no obligation in it.
    Context,
    /// Raw reference material (a JSON/YAML spec, a code sample).
    Reference,
}

impl RequirementKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RequirementKind::Functional => "functional",
            RequirementKind::Security => "security",
            RequirementKind::Performance => "performance",
            RequirementKind::Usability => "usability",
            RequirementKind::Interface => "interface",
            RequirementKind::Data => "data",
            RequirementKind::Context => "context",
            RequirementKind::Reference => "reference",
        }
    }

    /// The kind a stored value names; anything unknown reads as `Context`
    /// (it is still shown to the skills that read background).
    pub fn from_db(value: &str) -> RequirementKind {
        match value {
            "functional" => RequirementKind::Functional,
            "security" => RequirementKind::Security,
            "performance" => RequirementKind::Performance,
            "usability" => RequirementKind::Usability,
            "interface" => RequirementKind::Interface,
            "data" => RequirementKind::Data,
            "reference" => RequirementKind::Reference,
            _ => RequirementKind::Context,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParsedRequirement {
    /// The id people see and a generated case carries: the document's own
    /// (`FR-012`) when it has one, otherwise `<DOC>-<section>-<nn>`.
    pub key: String,
    /// Position in the document, from 0.
    pub ordinal: usize,
    /// The heading it sits under (`3.2 Login`), if any.
    pub section: Option<String>,
    pub kind: RequirementKind,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ParsedDocument {
    pub requirements: Vec<ParsedRequirement>,
    /// Headings found (sections left out on purpose included).
    pub sections_count: usize,
    /// Sections left out on purpose (a table of contents, a revision
    /// history, a glossary), by title.
    pub skipped_sections: Vec<String>,
    /// Lines dropped as page furniture (page numbers, running headers).
    pub furniture_lines: usize,
}

// ------------------------------------------------------------------ entry point

/// Parses `text` (the whole extracted document) into requirements. `doc_label`
/// prefixes the generated ids (`BRD`, `FRD`, ...); `file_name` only decides
/// whether the file is reference material (JSON/YAML) rather than prose.
pub fn parse_document(doc_label: &str, file_name: &str, text: &str) -> ParsedDocument {
    let label = clean_label(doc_label, file_name);
    let text = normalize(text);
    let ext = file_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "json" | "yaml" | "yml") {
        return parse_reference(&label, &text);
    }
    parse_prose(&label, &text)
}

/// Id prefix: the document type when it is one word of letters and digits,
/// otherwise the start of the file name.
fn clean_label(doc_label: &str, file_name: &str) -> String {
    let from = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(6)
            .collect::<String>()
            .to_ascii_uppercase()
    };
    let label = from(doc_label);
    if !label.is_empty() {
        return label;
    }
    let stem = file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file_name);
    let label = from(stem);
    if label.is_empty() {
        "DOC".to_string()
    } else {
        label
    }
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ")
        .replace('\u{a0}', " ")
        .replace(['\u{200b}', '\u{feff}', '\u{ad}'], "")
}

// ------------------------------------------------------------ reference material

/// JSON / YAML: not prose, so no attempt is made to find requirements in it.
/// The whole file is kept, cut at line boundaries into pieces of about
/// `REFERENCE_CHUNK_CHARS`, so nothing is lost and no piece is huge.
fn parse_reference(label: &str, text: &str) -> ParsedDocument {
    let mut requirements = Vec::new();
    let mut current = String::new();
    let push = |current: &mut String, requirements: &mut Vec<ParsedRequirement>| {
        let piece = current.trim_end().to_string();
        current.clear();
        if piece.trim().is_empty() {
            return;
        }
        let ordinal = requirements.len();
        requirements.push(ParsedRequirement {
            key: format!("{label}-R{:02}", ordinal + 1),
            ordinal,
            section: None,
            kind: RequirementKind::Reference,
            text: piece,
        });
    };
    for line in text.lines() {
        // A single enormous line (minified JSON) is cut at the limit itself.
        let mut rest = line;
        while rest.chars().count() > REFERENCE_CHUNK_CHARS {
            let cut = char_boundary_at(rest, REFERENCE_CHUNK_CHARS);
            if !current.is_empty() {
                push(&mut current, &mut requirements);
            }
            current.push_str(&rest[..cut]);
            push(&mut current, &mut requirements);
            rest = &rest[cut..];
        }
        if current.chars().count() + rest.chars().count() + 1 > REFERENCE_CHUNK_CHARS
            && !current.is_empty()
        {
            push(&mut current, &mut requirements);
        }
        current.push_str(rest);
        current.push('\n');
    }
    push(&mut current, &mut requirements);
    ParsedDocument {
        requirements,
        sections_count: 0,
        skipped_sections: Vec::new(),
        furniture_lines: 0,
    }
}

/// Byte index of the `n`-th character (or the end of the string).
fn char_boundary_at(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len())
}

/// Where to cut `s` so that the first piece is at most `limit` characters:
/// after the last `;` or space in the second half of that range, so a cut
/// does not land in the middle of a word; at the limit itself if there is none.
fn cut_at_boundary(s: &str, limit: usize) -> usize {
    let hard = char_boundary_at(s, limit);
    if hard >= s.len() {
        return s.len();
    }
    let head = &s[..hard];
    let half = char_boundary_at(head, limit / 2);
    match head[half..].rfind([';', ' ']) {
        Some(i) => half + i + 1,
        None => hard,
    }
}

// ------------------------------------------------------------------- prose

fn modal_regex() -> &'static Regex {
    regex!(
        r"(?i)\b(shall|must|should|will|can|may|needs? to|is required|are required|has to|have to|cannot|can't|must not|is not allowed|are not allowed|only if|not be able|is expected to|are expected to|is able to|are able to)\b"
    )
}

fn has_modal(text: &str) -> bool {
    modal_regex().is_match(text)
}

fn page_number_regex() -> &'static Regex {
    regex!(r"(?i)^(page\s+\d+(\s+of\s+\d+)?|\d+\s*/\s*\d+|-\s*\d+\s*-|\d{1,4})$")
}

/// A running footer or header that ends in the page number ("... v6.0 Page 2").
fn page_footer_regex() -> &'static Regex {
    regex!(r"(?i)\S.*\bpage\s+\d+(\s+of\s+\d+)?$")
}

fn skip_section_regex() -> &'static Regex {
    regex!(
        r"(?i)^(?:\d+(?:\.\d+)*\.?\s*)?(table of contents|contents|revision history|version history|document (control|history|information|approval)|change ?log|document revision|approvals?|sign[- ]?off|distribution list|glossary|abbreviations|acronyms|definitions|list of (figures|tables)|references|bibliography|acknowledg\w*)\s*:?$"
    )
}

fn md_heading_regex() -> &'static Regex {
    regex!(r"^(#{1,6})\s+(\S.*?)\s*#*\s*$")
}

fn numbered_heading_regex() -> &'static Regex {
    regex!(r"^(\d+(?:\.\d+)+)\.?\s+(\S.*)$")
}

fn single_number_heading_regex() -> &'static Regex {
    regex!(r"^(\d{1,2})[.)]?\s+([A-Z][^.!?;:]{2,70})$")
}

fn keyword_heading_regex() -> &'static Regex {
    regex!(r"(?i)^(section|chapter|part|appendix)\s+([A-Z0-9]+)\b\s*[:.\-–—]?\s*(.*)$")
}

fn caps_heading_regex() -> &'static Regex {
    regex!(r"^[A-Z][A-Z0-9 &/,()'\-]{3,70}$")
}

fn id_line_regex() -> &'static Regex {
    regex!(
        r"^\s*[\[(]?([A-Z][A-Z0-9]{0,7}(?:-[A-Z0-9]{1,8})*-\d+(?:\.\d+)*[a-z]?)[\])]?\s*(?:[:.)\-–—]|\|)\s*(\S.*)$"
    )
}

fn id_only_regex() -> &'static Regex {
    regex!(r"^[A-Z][A-Z0-9]{0,7}(?:-[A-Z0-9]{1,8})*-\d+(?:\.\d+)*[a-z]?$")
}

fn bullet_regex() -> &'static Regex {
    regex!(r"^(\s*)[-*•●▪◦‣■□►▶–]\s+(\S.*)$")
}

fn numbered_item_regex() -> &'static Regex {
    regex!(r"^(\s*)(?:\d{1,3}|[a-zA-Z]|[ivxIVX]{1,4})[.)]\s+(\S.*)$")
}

/// A table-of-contents entry: a title, a run of dots, a page number.
fn toc_line_regex() -> &'static Regex {
    regex!(r"^.{3,90}?(?:\.{3,}|(?:\s\.){2,})\s*\d{1,4}\s*$")
}

/// The cells of a table row, if the line is one: Markdown (`| a | b |`) or the
/// `a | b` a Word table is written as.
fn table_cells(trimmed: &str) -> Option<Vec<String>> {
    if trimmed.starts_with('|') {
        let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
        return Some(inner.split('|').map(collapse_spaces).collect());
    }
    if trimmed.contains(" | ") {
        return Some(trimmed.split(" | ").map(collapse_spaces).collect());
    }
    None
}

/// The `|---|---|` line under a Markdown table's header.
fn is_separator_row(cells: &[String]) -> bool {
    !cells.is_empty() && cells.iter().all(|c| regex!(r"^:?-{2,}:?$").is_match(c))
}

/// A Word table has no separator line, so its first row is taken as the
/// header when every filled cell is a plain label (words, starting with a
/// letter: no digits-first cells, paths, quotes or emphasis) with no
/// obligation in it, and it does not start with a requirement id.
fn looks_like_header(cells: &[String]) -> bool {
    let filled: Vec<&String> = cells.iter().filter(|c| !c.is_empty()).collect();
    filled.len() >= 2
        && !cells.first().is_some_and(|c| id_only_regex().is_match(c))
        && filled
            .iter()
            .all(|c| regex!(r"^[A-Za-z#%][A-Za-z0-9 /&()#%?._'-]{0,38}$").is_match(c))
        && !filled.iter().any(|c| has_modal(c))
}

/// Where a table is up to: its header once known, and a first row that has
/// not yet been told apart from a header.
#[derive(Default)]
struct Table {
    open: bool,
    header: Option<Vec<String>>,
    first_row: Option<Vec<String>>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A running header or footer repeats on every page with only the page
/// number changing. Lines whose digit-normalised form appears at least five
/// times and that read like a header (not a sentence, not an item) are page
/// furniture.
fn running_furniture(lines: &[&str]) -> HashSet<String> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let key_of = |l: &str| -> String {
        regex!(r"\d+")
            .replace_all(l.trim(), "#")
            .to_ascii_lowercase()
    };
    for l in lines {
        let t = l.trim();
        if t.chars().count() < 12 || t.chars().count() > 100 {
            continue;
        }
        *counts.entry(key_of(t)).or_insert(0) += 1;
    }
    let mut furniture = HashSet::new();
    for l in lines {
        let t = l.trim();
        let key = key_of(t);
        if counts.get(&key).copied().unwrap_or(0) >= 5
            && !has_modal(t)
            && !bullet_regex().is_match(l)
            && !numbered_item_regex().is_match(l)
            && !id_line_regex().is_match(l)
            && !t.contains(" | ")
            && !t.starts_with('#')
        {
            furniture.insert(key);
        }
    }
    furniture
}

/// Heading detection for one line: `(level, number, title)`.
fn heading_of(
    line: &str,
    previous_blank: bool,
    has_md_headings: bool,
) -> Option<(usize, Option<String>, String)> {
    let t = line.trim();
    if t.is_empty() || indent_of(line) > 8 {
        return None;
    }
    if let Some(c) = md_heading_regex().captures(t) {
        let level = c[1].len();
        let title = c[2].trim().trim_end_matches(':').to_string();
        // A Markdown heading may itself be numbered ("## 3.2 Login", "# 1. Purpose").
        let number = regex!(r"^(\d+(?:\.\d+)*)[.)]?\s+\S")
            .captures(&title)
            .map(|n| n[1].to_string());
        return Some((level, number, title));
    }
    if let Some(c) = numbered_heading_regex().captures(t) {
        let number = c[1].to_string();
        let title = c[2].trim();
        let words = title.split_whitespace().count();
        let ends_like_sentence = title.ends_with(['.', ';', ',', '!', '?']);
        if words <= 10 && !ends_like_sentence && !has_modal(title) && !toc_line_regex().is_match(t)
        {
            let level = number.matches('.').count() + 1;
            return Some((level, Some(number.clone()), format!("{number} {title}")));
        }
        return None;
    }
    if let Some(c) = keyword_heading_regex().captures(t) {
        let rest = c[3].trim();
        if !has_modal(rest) && rest.split_whitespace().count() <= 10 {
            let title = if rest.is_empty() {
                format!("{} {}", &c[1], &c[2])
            } else {
                format!("{} {}: {rest}", &c[1], &c[2])
            };
            return Some((1, None, title));
        }
    }
    // "3. Authentication" — a lone number is also how a list item starts, so
    // it only counts when it is set apart by a blank line and reads like a
    // title, in a document that does not use Markdown headings.
    if previous_blank && !has_md_headings {
        if let Some(c) = single_number_heading_regex().captures(t) {
            let title = c[2].trim();
            let words: Vec<&str> = title.split_whitespace().collect();
            let title_cased = words
                .iter()
                .filter(|w| w.len() > 3)
                .all(|w| w.chars().next().is_some_and(|ch| ch.is_uppercase()));
            if words.len() <= 6 && title_cased && !has_modal(title) {
                return Some((1, Some(c[1].to_string()), format!("{} {title}", &c[1])));
            }
        }
    }
    if previous_blank && !id_only_regex().is_match(t) && caps_heading_regex().is_match(t) {
        let letters = t.chars().filter(|c| c.is_alphabetic()).count();
        if letters >= 4 && t.split_whitespace().count() >= 2 {
            return Some((2, None, t.to_string()));
        }
    }
    None
}

/// Words and phrases that decide a requirement's kind, checked in this order.
fn classify(text: &str, section: Option<&str>) -> RequirementKind {
    let lower = text.to_lowercase();
    let section_lower = section.map(|s| s.to_lowercase()).unwrap_or_default();
    let both = format!("{lower} {section_lower}");

    if regex!(
        r"\b(password\w*|passcode\w*|otp|mfa|2fa|two[- ]factor|authenticat\w*|authoriz\w*|authoris\w*|permission\w*|privilege\w*|rbac|role[- ]based|roles?|access control|encrypt\w*|decrypt\w*|hash\w*|tokens?|sessions?|jwt|oauth|sso|saml|csrf|xss|sql injection|injection|vulnerab\w*|penetration|audit (?:log|trail)|log ?in|log ?out|logged (?:in|out)|sign ?in|sign ?out|sign(?:ed) in|lock ?out|locked|captcha|brute[- ]force|pii|gdpr|privacy|confidential\w*|sensitive|credentials?|secure|security|certificates?|tls|ssl|https)\b"
    )
    .is_match(&both)
    {
        return RequirementKind::Security;
    }
    if regex!(
        r"\b(within (?:\d+(?:\.\d+)?|a|one|two|three|four|five|six|seven|eight|nine|ten|twelve|fifteen|twenty|thirty|forty|sixty|ninety|a few|several)\s*(?:ms|milliseconds?|seconds?|secs?|minutes?|mins?|hours?)|no more than (?:\d+(?:\.\d+)?|a|one|two|three|four|five|six|seven|eight|nine|ten|twelve|fifteen|twenty|thirty|forty|sixty|ninety|a few|several)\s*(?:ms|milliseconds?|seconds?|secs?)|response times?|latency|throughput|concurrent\w*|simultaneous\w*|per second|tps|rps|load tests?|stress tests?|scalab\w*|performance|capacity|uptime|availability|sla|peak load|benchmark\w*|time[- ]?outs?)\b"
    )
    .is_match(&both)
    {
        return RequirementKind::Performance;
    }
    if regex!(
        r"\b(accessib\w*|wcag|aria|screen readers?|keyboard navigation|contrast|responsive|mobile|tablets?|browsers?|layout|usab\w*|user[- ]friendly|localiz\w*|internationali\w*|language|translat\w*|fonts?|colou?rs?|themes?|tooltips?|placeholders?)\b"
    )
    .is_match(&both)
    {
        return RequirementKind::Usability;
    }
    if regex!(
        r"\b(api|apis|endpoints?|rest|restful|graphql|https?|payloads?|json|xml|webhooks?|soap|grpc|sftp|integrations?|third[- ]party|swagger|openapi|request bod(?:y|ies)|response (?:codes?|bod(?:y|ies))|status codes?|query param\w*)\b"
    )
    .is_match(&both)
    {
        return RequirementKind::Interface;
    }
    if regex!(
        r"\b(database|db|schema|tables?|columns?|records?|fields?|validat\w*|format\w*|length|characters?|digits?|numeric|alphanumeric|mandatory|required|optional|unique|duplicates?|duplicated|maximum|minimum|at least \d+|at most \d+|up to \d+|between \d+ and \d+|retention|backup\w*|import\w*|export\w*|upload\w*|download\w*|file size|csv|excel|date|timestamps?|currency|decimal|precision|default values?|null|blank)\b"
    )
    .is_match(&lower)
    {
        return RequirementKind::Data;
    }
    if has_modal(text) {
        RequirementKind::Functional
    } else {
        RequirementKind::Context
    }
}

/// Splits prose into sentences without breaking on `e.g.`, `i.e.`, `etc.` or
/// the dot inside a section number or a decimal (`3.2`, which is not followed
/// by a space).
fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let abbreviations = [
        "e.g", "i.e", "etc", "vs", "no", "fig", "sec", "mr", "mrs", "dr", "approx", "inc", "ltd",
    ];
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if matches!(c, '.' | '!' | '?') {
            let next_is_space = chars.get(i + 1).is_some_and(|n| n.is_whitespace());
            if next_is_space {
                // The first thing after the spaces decides whether a sentence starts.
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                let starts_sentence = chars.get(j).is_some_and(|n| {
                    n.is_uppercase() || n.is_ascii_digit() || matches!(n, '"' | '(' | '[')
                });
                // The word this full stop ends.
                let mut k = i;
                while k > start && !chars[k - 1].is_whitespace() {
                    k -= 1;
                }
                let word: String = chars[k..i].iter().collect::<String>().to_lowercase();
                let is_abbreviation = abbreviations.contains(&word.as_str());
                if starts_sentence && !is_abbreviation {
                    let sentence: String = chars[start..=i].iter().collect();
                    out.push(sentence.trim().to_string());
                    start = j;
                    i = j;
                    continue;
                }
            }
        }
        i += 1;
    }
    if start < chars.len() {
        let rest: String = chars[start..].iter().collect();
        if !rest.trim().is_empty() {
            out.push(rest.trim().to_string());
        }
    }
    out
}

/// A sentence that only annotates the requirement before it ("Rationale: ...",
/// "Note: ...", "Acceptance criteria: ..."). Whatever it says — an explanation
/// is full of "can" and "may" — it is part of that requirement, not another one.
fn is_annotation(sentence: &str) -> bool {
    regex!(
        r"(?i)^(rationale|reason|justification|notes?|remarks?|comments?|examples?|explanation|description|source|priority|acceptance criteria|acceptance criterion|see also)\s*:"
    )
    .is_match(sentence.trim_start())
}

/// One paragraph as one or more requirement texts: split so that no piece
/// carries more than one obligation, and none is longer than `MAX_ITEM_CHARS`.
fn split_paragraph(text: &str) -> Vec<String> {
    split_text(text, true)
}

/// `split_paragraph`, or — with `by_obligation` false — only the size limit: an
/// item the document itself numbers is one requirement however many
/// obligations its text has, since that number is what people refer to.
fn split_text(text: &str, by_obligation: bool) -> Vec<String> {
    let sentences = split_sentences(text);
    let modal_sentences = sentences.iter().filter(|s| has_modal(s)).count();
    let mut groups: Vec<String> = Vec::new();
    if by_obligation && modal_sentences >= 2 {
        let mut current = String::new();
        let mut current_has_modal = false;
        for s in &sentences {
            let s_modal = has_modal(s);
            if s_modal && current_has_modal && !is_annotation(s) {
                groups.push(std::mem::take(&mut current));
                current_has_modal = false;
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(s);
            current_has_modal |= s_modal;
        }
        if !current.is_empty() {
            groups.push(current);
        }
    } else {
        groups.push(text.to_string());
    }
    let mut out = Vec::new();
    for g in groups {
        if g.chars().count() <= MAX_ITEM_CHARS {
            out.push(g);
            continue;
        }
        let mut piece = String::new();
        for s in split_sentences(&g) {
            if !piece.is_empty()
                && piece.chars().count() + s.chars().count() + 1 > SPLIT_TARGET_CHARS
            {
                out.push(std::mem::take(&mut piece));
            }
            // One sentence longer than the limit is cut at the limit.
            let mut s = s;
            while s.chars().count() > MAX_ITEM_CHARS {
                let cut = cut_at_boundary(&s, SPLIT_TARGET_CHARS);
                if !piece.is_empty() {
                    out.push(std::mem::take(&mut piece));
                }
                out.push(s[..cut].trim_end().to_string());
                s = s[cut..].trim_start().to_string();
            }
            if !piece.is_empty() {
                piece.push(' ');
            }
            piece.push_str(&s);
        }
        if !piece.is_empty() {
            out.push(piece);
        }
    }
    out
}

struct Builder {
    label: String,
    requirements: Vec<ParsedRequirement>,
    keys: HashSet<String>,
    section_title: Option<String>,
    section_ref: String,
    unnumbered_sections: usize,
    item_in_section: usize,
    sections_count: usize,
    skipping_level: Option<usize>,
    skipped_sections: Vec<String>,
    furniture_lines: usize,
}

impl Builder {
    fn new(label: &str) -> Self {
        Builder {
            label: label.to_string(),
            requirements: Vec::new(),
            keys: HashSet::new(),
            section_title: None,
            section_ref: "S0".to_string(),
            unnumbered_sections: 0,
            item_in_section: 0,
            sections_count: 0,
            skipping_level: None,
            skipped_sections: Vec::new(),
            furniture_lines: 0,
        }
    }

    fn start_section(&mut self, level: usize, number: Option<&str>, title: &str) {
        self.sections_count += 1;
        if let Some(skipping) = self.skipping_level {
            if level <= skipping {
                self.skipping_level = None;
            }
        }
        if self.skipping_level.is_none() && skip_section_regex().is_match(title) {
            self.skipping_level = Some(level);
            self.skipped_sections.push(title.to_string());
        }
        self.item_in_section = 0;
        match number {
            Some(n) => self.section_ref = n.to_string(),
            None => {
                self.unnumbered_sections += 1;
                self.section_ref = format!("S{}", self.unnumbered_sections);
            }
        }
        self.section_title = Some(title.to_string());
    }

    fn skipping(&self) -> bool {
        self.skipping_level.is_some()
    }

    fn unique_key(&mut self, base: String) -> String {
        if self.keys.insert(base.clone()) {
            return base;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}~{n}");
            if self.keys.insert(candidate.clone()) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Adds one requirement. `explicit_key` is the document's own id for it.
    fn push(
        &mut self,
        explicit_key: Option<String>,
        text: &str,
        kind_hint: Option<RequirementKind>,
    ) {
        let text = collapse_spaces(text);
        let words = text.split_whitespace().count();
        if explicit_key.is_none() && (text.chars().count() < 12 || words < 3) {
            return;
        }
        if text.is_empty() {
            return;
        }
        if toc_line_regex().is_match(&text) {
            return;
        }
        self.item_in_section += 1;
        let has_own_id = explicit_key.is_some();
        let base = match explicit_key {
            Some(k) => k,
            None => format!(
                "{}-{}-{:02}",
                self.label, self.section_ref, self.item_in_section
            ),
        };
        let key = self.unique_key(base);
        let kind = match kind_hint {
            Some(k) => k,
            None => match classify(&text, self.section_title.as_deref()) {
                // "FR-3: Create a plan." has no "shall" in it, but it is a requirement.
                RequirementKind::Context if has_own_id => RequirementKind::Functional,
                k => k,
            },
        };
        let ordinal = self.requirements.len();
        self.requirements.push(ParsedRequirement {
            key,
            ordinal,
            section: self.section_title.clone(),
            kind,
            text,
        });
    }

    fn push_paragraph(&mut self, text: &str) {
        for piece in split_paragraph(&collapse_spaces(text)) {
            self.push(None, &piece, None);
        }
    }

    /// One table row as one requirement: its first cell is the document's own
    /// id when it looks like one, and each cell is labelled with its column.
    fn push_table_row(&mut self, header: Option<&[String]>, cells: &[String]) {
        let key = cells
            .first()
            .filter(|c| id_only_regex().is_match(c))
            .cloned();
        let body: &[String] = if key.is_some() { &cells[1..] } else { cells };
        let offset = header
            .map(|h| h.len().saturating_sub(body.len()))
            .unwrap_or(0);
        let text = body
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_empty())
            .map(|(i, c)| match header.and_then(|h| h.get(i + offset)) {
                Some(h) if !h.is_empty() => format!("{h}: {c}"),
                _ => c.clone(),
            })
            .collect::<Vec<_>>()
            .join("; ");
        self.push(key, &text, None);
    }
}

/// What has been read but not yet turned into a requirement.
enum Pending {
    None,
    Paragraph(String),
    /// A list item, or an id line. `lead_in` is true for a sentence that ends
    /// with a colon and introduces the list items that follow it.
    Item {
        key: Option<String>,
        text: String,
        indent: usize,
        lead_in: bool,
    },
    Code(Vec<String>),
}

fn parse_prose(label: &str, text: &str) -> ParsedDocument {
    let lines: Vec<&str> = text.lines().collect();
    let furniture = running_furniture(&lines);
    let has_md_headings = lines.iter().any(|l| md_heading_regex().is_match(l.trim()));

    let mut b = Builder::new(label);
    let mut pending = Pending::None;
    let mut previous_blank = true;
    let mut table = Table::default();

    macro_rules! flush {
        () => {{
            match std::mem::replace(&mut pending, Pending::None) {
                Pending::None => {}
                Pending::Paragraph(t) => {
                    if !b.skipping() {
                        b.push_paragraph(&t);
                    }
                }
                Pending::Item { key, text, .. } => {
                    if !b.skipping() {
                        for (i, piece) in split_text(&collapse_spaces(&text), key.is_none())
                            .into_iter()
                            .enumerate()
                        {
                            b.push(if i == 0 { key.clone() } else { None }, &piece, None);
                        }
                    }
                }
                Pending::Code(code_lines) => {
                    if !b.skipping() {
                        let joined = code_lines.join("\n");
                        let mut rest: &str = joined.trim();
                        while !rest.is_empty() {
                            let cut = cut_at_boundary(rest, MAX_ITEM_CHARS);
                            let piece = rest[..cut].trim_end();
                            b.push(None, piece, Some(RequirementKind::Reference));
                            rest = rest[cut..].trim_start();
                        }
                    }
                }
            }
        }};
    }

    // A table that ends with its first row still undecided: that row is data.
    macro_rules! end_table {
        () => {{
            if let Some(first) = table.first_row.take() {
                if !b.skipping() {
                    b.push_table_row(None, &first);
                }
            }
            table.open = false;
            table.header = None;
        }};
    }

    for raw in lines.iter() {
        let line = *raw;
        let trimmed = line.trim();

        // A fenced code block is kept whole as reference material.
        if trimmed.starts_with("```") {
            if matches!(pending, Pending::Code(_)) {
                flush!();
            } else {
                flush!();
                end_table!();
                pending = Pending::Code(Vec::new());
            }
            previous_blank = false;
            continue;
        }
        if let Pending::Code(code) = &mut pending {
            code.push(line.to_string());
            continue;
        }

        if trimmed.is_empty() {
            flush!();
            end_table!();
            previous_blank = true;
            continue;
        }

        // Page furniture.
        let key = regex!(r"\d+")
            .replace_all(trimmed, "#")
            .to_ascii_lowercase();
        if page_number_regex().is_match(trimmed)
            || (trimmed.chars().count() <= 100
                && page_footer_regex().is_match(trimmed)
                && !has_modal(trimmed))
            || furniture.contains(&key)
            || toc_line_regex().is_match(trimmed)
        {
            b.furniture_lines += 1;
            continue;
        }

        // Headings.
        if let Some((level, number, title)) = heading_of(line, previous_blank, has_md_headings) {
            flush!();
            end_table!();
            b.start_section(level, number.as_deref(), &title);
            previous_blank = false;
            continue;
        }
        previous_blank = false;

        // Table rows: Markdown (`| a | b |`) or a Word table (`a | b`).
        if let Some(cells) = table_cells(trimmed) {
            flush!();
            if is_separator_row(&cells) {
                // The row above the separator is the header.
                if let Some(first) = table.first_row.take() {
                    table.header = Some(first);
                }
                table.open = true;
                continue;
            }
            if !table.open {
                table.open = true;
                table.first_row = Some(cells);
                continue;
            }
            if let Some(first) = table.first_row.take() {
                if looks_like_header(&first) {
                    table.header = Some(first);
                } else if !b.skipping() {
                    b.push_table_row(None, &first);
                }
            }
            if !b.skipping() {
                b.push_table_row(table.header.as_deref(), &cells);
            }
            continue;
        }
        end_table!();

        // An explicit id starts a requirement.
        if let Some(c) = id_line_regex().captures(line) {
            flush!();
            pending = Pending::Item {
                key: Some(c[1].to_string()),
                text: c[2].to_string(),
                indent: indent_of(line),
                lead_in: false,
            };
            continue;
        }

        // List items: a deeper one belongs to the item above it.
        let item_match = bullet_regex()
            .captures(line)
            .or_else(|| numbered_item_regex().captures(line));
        if let Some(c) = item_match {
            let indent = c[1].len();
            let body = c[2].to_string();
            // "The form shall include:" followed by list items: the items are
            // the details of that sentence, not requirements of their own.
            if let Pending::Paragraph(t) = &pending {
                if t.trim_end().ends_with(':') {
                    pending = Pending::Item {
                        key: None,
                        text: t.clone(),
                        indent: 0,
                        lead_in: true,
                    };
                }
            }
            if let Pending::Item {
                text,
                indent: parent_indent,
                lead_in,
                ..
            } = &mut pending
            {
                if *lead_in || indent > *parent_indent || text.trim_end().ends_with(':') {
                    if text.trim_end().ends_with(':') {
                        text.push(' ');
                    } else {
                        text.push_str("; ");
                    }
                    text.push_str(&body);
                    continue;
                }
            }
            flush!();
            // A list item can carry the document's own id ("- FR-3: ...").
            pending = match id_line_regex().captures(&body) {
                Some(idc) => Pending::Item {
                    key: Some(idc[1].to_string()),
                    text: idc[2].to_string(),
                    indent,
                    lead_in: false,
                },
                None => Pending::Item {
                    key: None,
                    text: body,
                    indent,
                    lead_in: false,
                },
            };
            continue;
        }

        // Anything else continues what is pending, or starts a paragraph.
        match &mut pending {
            Pending::Item { text, .. } => {
                text.push(' ');
                text.push_str(trimmed);
            }
            Pending::Paragraph(text) => {
                text.push(' ');
                text.push_str(trimmed);
            }
            _ => pending = Pending::Paragraph(trimmed.to_string()),
        }
    }
    flush!();
    end_table!();

    ParsedDocument {
        requirements: b.requirements,
        sections_count: b.sections_count,
        skipped_sections: b.skipped_sections,
        furniture_lines: b.furniture_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> ParsedDocument {
        parse_document("FRD", "spec.md", text)
    }

    fn keys(doc: &ParsedDocument) -> Vec<&str> {
        doc.requirements.iter().map(|r| r.key.as_str()).collect()
    }

    fn texts(doc: &ParsedDocument) -> Vec<&str> {
        doc.requirements.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn explicit_ids_and_numbered_sections_are_kept_as_the_documents_own() {
        let doc = parse(
            "## 3.2 Login\n\n\
             FR-012: The system shall lock an account after 5 failed login attempts.\n\
             FR-013: A locked account shall be unlocked automatically after 15 minutes.\n\n\
             ## 3.3 Reports\n\n\
             The system shall export any report as a CSV file.\n",
        );

        assert_eq!(keys(&doc), ["FR-012", "FR-013", "FRD-3.3-01"]);
        assert_eq!(doc.requirements[0].section.as_deref(), Some("3.2 Login"));
        assert_eq!(doc.requirements[2].section.as_deref(), Some("3.3 Reports"));
        assert_eq!(
            doc.requirements[0].text,
            "The system shall lock an account after 5 failed login attempts."
        );
        assert_eq!(
            doc.requirements
                .iter()
                .map(|r| r.ordinal)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(doc.sections_count, 2);
    }

    #[test]
    fn a_document_without_headings_still_yields_requirements_with_generated_ids() {
        let doc = parse(
            "The system shall send a welcome email.\n\nThe system shall log every sign-in.\n",
        );
        assert_eq!(keys(&doc), ["FRD-S0-01", "FRD-S0-02"]);
        assert!(doc.requirements.iter().all(|r| r.section.is_none()));
    }

    #[test]
    fn ids_inside_list_items_and_table_rows_are_read_too() {
        let doc = parse(
            "## 4 Rules\n\n\
             - BR-7: A discount cannot exceed 30 percent of the order value.\n\
             - A refund must be issued within 7 days.\n\n\
             ID | Requirement\n\
             FR-9 | Passwords are stored hashed.\n",
        );
        // "ID | Requirement" is a header (Word tables have no separator line).
        assert_eq!(keys(&doc), ["BR-7", "FRD-4-02", "FR-9"]);
        assert_eq!(
            doc.requirements[2].text,
            "Requirement: Passwords are stored hashed."
        );
    }

    #[test]
    fn a_nested_list_belongs_to_the_item_above_it() {
        let doc = parse(
            "## 4 Registration\n\n\
             - The form shall collect:\n\
             \x20 - an email address\n\
             \x20 - a password\n\
             - The form shall reject a duplicate email address.\n",
        );
        assert_eq!(
            texts(&doc),
            [
                "The form shall collect: an email address; a password",
                "The form shall reject a duplicate email address."
            ]
        );
    }

    #[test]
    fn a_sentence_ending_in_a_colon_takes_the_list_after_it_as_its_detail() {
        let doc = parse(
            "The registration form shall include:\n- Full name\n- Email address\n- Password\n",
        );
        assert_eq!(
            texts(&doc),
            ["The registration form shall include: Full name; Email address; Password"]
        );
    }

    #[test]
    fn a_word_table_becomes_one_requirement_per_row_labelled_by_column() {
        let doc = parse(
            "## 5 Fields\n\n\
             Field | Rule | Priority\n\
             Email | Must be a valid address | High\n\
             Age | Must be at least 18 | Medium\n",
        );
        assert_eq!(
            texts(&doc),
            [
                "Field: Email; Rule: Must be a valid address; Priority: High",
                "Field: Age; Rule: Must be at least 18; Priority: Medium"
            ]
        );
    }

    #[test]
    fn a_markdown_table_uses_the_header_above_its_separator_line() {
        let doc = parse(
            "## 6 Routes\n\n\
             | Route | Method | Role |\n\
             |---|---|---|\n\
             | `/runs` | GET | Viewer |\n\
             | `/runs/:id/approve` | POST | Operator |\n",
        );
        assert_eq!(
            texts(&doc),
            [
                "Route: `/runs`; Method: GET; Role: Viewer",
                "Route: `/runs/:id/approve`; Method: POST; Role: Operator"
            ]
        );
    }

    #[test]
    fn a_table_that_has_no_header_is_not_given_one_by_mistake() {
        let doc = parse(
            "## 7 Endpoints\n\n\
             | 5 | `GET /a` | none | **MISSING** |\n\
             | 6 | `GET /b` | none | **MISSING** |\n",
        );
        assert_eq!(doc.requirements.len(), 2);
        assert!(
            doc.requirements[0].text.starts_with("5; `GET /a`"),
            "the first data row must stay data: {}",
            doc.requirements[0].text
        );
    }

    #[test]
    fn a_table_with_a_single_row_keeps_that_row() {
        let doc = parse("## 7 Limits\n\n| Maximum upload size is 40 MB | required |\n");
        assert_eq!(doc.requirements.len(), 1);
        assert!(doc.requirements[0].text.contains("40 MB"));
    }

    #[test]
    fn a_paragraph_with_two_obligations_is_split_and_narrative_stays_as_context() {
        let doc = parse(
            "The business wants a simpler onboarding flow for small merchants.\n\n\
             The system shall verify the merchant's email address. The system shall then create the merchant's first store.\n",
        );
        assert_eq!(
            texts(&doc),
            [
                "The business wants a simpler onboarding flow for small merchants.",
                "The system shall verify the merchant's email address.",
                "The system shall then create the merchant's first store."
            ]
        );
        assert_eq!(doc.requirements[0].kind, RequirementKind::Context);
        assert_eq!(doc.requirements[1].kind, RequirementKind::Functional);
    }

    #[test]
    fn a_rationale_or_note_stays_with_the_requirement_it_explains_even_when_it_says_may_or_can() {
        let doc = parse(
            "- The system shall show the account balance.\n  Rationale: Customers can then decide what to pay.\n\n\
             The system shall show the payment history. Note: A customer may hide it.\n",
        );
        assert_eq!(
            texts(&doc),
            [
                "The system shall show the account balance. Rationale: Customers can then decide what to pay.",
                "The system shall show the payment history. Note: A customer may hide it."
            ]
        );
    }

    #[test]
    fn a_second_real_obligation_after_a_rationale_is_still_its_own_requirement() {
        let doc = parse(
            "The system shall show the balance. Rationale: Customers may want to check it. The system shall show the history.\n",
        );
        assert_eq!(
            texts(&doc),
            [
                "The system shall show the balance. Rationale: Customers may want to check it.",
                "The system shall show the history."
            ]
        );
    }

    #[test]
    fn a_label_in_the_middle_of_a_sentence_does_not_make_it_an_annotation() {
        let doc = parse(
            "The system shall save the order. The system shall attach the customer's note: it is shown to the packer.\n",
        );
        assert_eq!(
            texts(&doc),
            [
                "The system shall save the order.",
                "The system shall attach the customer's note: it is shown to the packer."
            ]
        );
    }

    #[test]
    fn an_item_with_the_documents_own_id_stays_one_requirement_however_many_obligations_it_has() {
        let doc = parse(
            "- FR-7: The system shall lock the account. The system shall also email the owner.\n\
             - FR-8: The system shall unlock it after 30 minutes.\n",
        );
        assert_eq!(keys(&doc), ["FR-7", "FR-8"]);
        assert_eq!(
            doc.requirements[0].text,
            "The system shall lock the account. The system shall also email the owner."
        );
    }

    #[test]
    fn a_numbered_item_is_still_cut_when_it_is_longer_than_the_limit() {
        let sentence =
            "The system shall keep an audit record of every administrative change made by a user. ";
        let doc = parse(&format!("FR-9: {}\n", sentence.repeat(40).trim()));
        assert!(doc.requirements.len() > 1);
        assert_eq!(doc.requirements[0].key, "FR-9");
        assert!(doc
            .requirements
            .iter()
            .all(|r| r.text.chars().count() <= MAX_ITEM_CHARS));
        let all: String = doc
            .requirements
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(all.matches("audit record").count(), 40);
    }

    #[test]
    fn a_wrapped_paragraph_is_joined_into_one_requirement() {
        let doc = parse("The system shall keep every uploaded\nfile for seven years and then\ndelete it permanently.\n");
        assert_eq!(
            texts(&doc),
            ["The system shall keep every uploaded file for seven years and then delete it permanently."]
        );
    }

    #[test]
    fn revision_history_and_contents_are_left_out_and_reported() {
        let doc = parse(
            "# Revision History\n\n\
             1.0 Initial draft written by the product team.\n\n\
             # Table of Contents\n\n\
             Introduction ........ 3\n\
             Login ........ 5\n\n\
             # 1. Login\n\n\
             The system shall show an error for a wrong password.\n",
        );
        assert_eq!(
            doc.skipped_sections,
            ["Revision History", "Table of Contents"]
        );
        assert_eq!(
            texts(&doc),
            ["The system shall show an error for a wrong password."]
        );
    }

    #[test]
    fn contents_entries_with_dot_leaders_are_dropped_wherever_they_are() {
        let doc = parse(
            "Introduction ........ 3\nLogin . . . . . 5\n\nThe system shall show an error for a wrong password.\n",
        );
        assert_eq!(
            texts(&doc),
            ["The system shall show an error for a wrong password."]
        );
        assert_eq!(doc.furniture_lines, 2);
    }

    #[test]
    fn a_skipped_section_ends_at_the_next_heading_of_the_same_or_higher_level() {
        let doc = parse(
            "# Glossary\n\nSSO means single sign-on and is used across every product.\n\n\
             ## Terms\n\nTerm one is defined here in this glossary entry.\n\n\
             # Login\n\nThe system shall offer single sign-on.\n",
        );
        assert_eq!(doc.skipped_sections, ["Glossary"]);
        assert_eq!(texts(&doc), ["The system shall offer single sign-on."]);
    }

    #[test]
    fn page_numbers_and_running_headers_and_footers_are_dropped_and_counted() {
        let mut text = String::new();
        for page in 1..=6 {
            text.push_str(&format!("Acme Portal FRD v1.2 - Confidential {page}\n\n"));
            text.push_str(&format!(
                "The system shall support scenario number {page} for the portal.\n\n"
            ));
            text.push_str(&format!("Page {page} of 6\n\n"));
        }
        let doc = parse(&text);
        assert_eq!(doc.requirements.len(), 6, "{:?}", texts(&doc));
        assert!(doc
            .requirements
            .iter()
            .all(|r| r.text.starts_with("The system shall support scenario")));
        assert_eq!(doc.furniture_lines, 12);
    }

    #[test]
    fn a_footer_that_ends_in_the_page_number_is_dropped_even_in_a_short_document() {
        let doc = parse(
            "The system shall show a dashboard.\n\nAcme FRD v2.0 Page 1\n\nThe system shall show a report.\n\nAcme FRD v2.0 Page 2\n",
        );
        assert_eq!(
            texts(&doc),
            [
                "The system shall show a dashboard.",
                "The system shall show a report."
            ]
        );
    }

    #[test]
    fn a_code_block_is_kept_whole_as_reference_material() {
        let doc = parse(
            "## 2.5 Errors\n\nErrors must be structured.\n\n```json\n{\n  \"error\": {\n    \"code\": \"NOT_FOUND\"\n  }\n}\n```\n\nThe message must be user-safe.\n",
        );
        assert_eq!(doc.requirements.len(), 3);
        assert_eq!(doc.requirements[1].kind, RequirementKind::Reference);
        assert!(doc.requirements[1].text.contains("\"code\": \"NOT_FOUND\""));
        assert_eq!(doc.requirements[2].text, "The message must be user-safe.");
    }

    #[test]
    fn json_and_yaml_are_kept_whole_in_pieces() {
        let mut yaml = String::new();
        for i in 0..200 {
            yaml.push_str(&format!(
                "/api/v1/items/{i}:\n  get:\n    summary: Fetch item number {i} from the store\n"
            ));
        }
        let doc = parse_document("ADDITIONAL", "openapi.yaml", &yaml);

        assert!(doc.requirements.len() > 5);
        assert!(doc
            .requirements
            .iter()
            .all(|r| r.kind == RequirementKind::Reference));
        assert!(doc
            .requirements
            .iter()
            .all(|r| r.text.chars().count() <= REFERENCE_CHUNK_CHARS + 1));
        assert_eq!(keys(&doc)[0], "ADDITI-R01");
        let rejoined = doc
            .requirements
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            rejoined.trim(),
            yaml.trim(),
            "nothing may be lost or reordered"
        );
    }

    #[test]
    fn one_enormous_line_of_json_is_cut_rather_than_kept_as_one_piece() {
        let json = format!("{{\"data\":\"{}\"}}", "x".repeat(4_000));
        let doc = parse_document("ADD", "blob.json", &json);
        assert!(doc.requirements.len() >= 3);
        assert!(doc
            .requirements
            .iter()
            .all(|r| r.text.chars().count() <= REFERENCE_CHUNK_CHARS + 1));
        let total: usize = doc
            .requirements
            .iter()
            .map(|r| r.text.chars().count())
            .sum();
        assert_eq!(total, json.chars().count());
    }

    #[test]
    fn nothing_that_carries_content_is_dropped() {
        let mut text = String::from("## 1 Scope\n\n");
        let mut sentences = Vec::new();
        for i in 0..40 {
            let s = format!("Item {i} of the platform shall behave according to rule {i}.");
            text.push_str(&s);
            text.push_str(if i % 3 == 0 { "\n" } else { "\n\n" });
            sentences.push(s);
        }
        let doc = parse(&text);
        let all = texts(&doc).join(" ");
        for s in &sentences {
            assert!(all.contains(s.as_str()), "lost: {s}");
        }
    }

    #[test]
    fn the_same_document_always_parses_to_the_same_requirements() {
        let text = "## 3 A\n\nFR-1: The system shall a thing.\n\n- The system shall another thing.\n\nSome background text about the thing.\n";
        assert_eq!(parse(text), parse(text));
    }

    #[test]
    fn a_repeated_id_gets_a_distinct_key() {
        let doc = parse("FR-1: The system shall do the first thing.\n\nFR-1: The system shall do the second thing.\n");
        assert_eq!(keys(&doc), ["FR-1", "FR-1~2"]);
    }

    #[test]
    fn an_item_with_the_documents_own_id_is_a_requirement_even_without_a_modal_verb() {
        let doc = parse("FR-3: Create a project scoped plan for every request.\n");
        assert_eq!(doc.requirements[0].kind, RequirementKind::Functional);
    }

    #[test]
    fn kinds_follow_what_a_requirement_is_about() {
        let cases = [
            (
                "Users must sign in with a password and a one-time OTP.",
                RequirementKind::Security,
            ),
            (
                "The page shall load within 2 seconds for 500 concurrent users.",
                RequirementKind::Performance,
            ),
            (
                "The dashboard shall load within two seconds for a user.",
                RequirementKind::Performance,
            ),
            (
                "Search results shall appear in no more than a few seconds.",
                RequirementKind::Performance,
            ),
            (
                "The site shall meet WCAG 2.1 AA and work with a screen reader.",
                RequirementKind::Usability,
            ),
            (
                "The service shall expose a REST endpoint that returns JSON.",
                RequirementKind::Interface,
            ),
            (
                "The name field must be at most 40 characters long.",
                RequirementKind::Data,
            ),
            (
                "The system shall send an invoice to the customer.",
                RequirementKind::Functional,
            ),
            (
                "The business wants happier customers this year.",
                RequirementKind::Context,
            ),
        ];
        for (text, kind) in cases {
            assert_eq!(classify(text, None), kind, "{text}");
        }
        // The section a line sits in counts too.
        assert_eq!(
            classify("Requests are rejected.", Some("6 Security & Governance")),
            RequirementKind::Security
        );
    }

    #[test]
    fn sentences_are_not_split_on_abbreviations_numbers_or_decimals() {
        let parts = split_sentences(
            "Support formats e.g. CSV and XLSX. See section 3.2 for details. Version 2.5 is required. Then stop.",
        );
        assert_eq!(
            parts,
            [
                "Support formats e.g. CSV and XLSX.",
                "See section 3.2 for details.",
                "Version 2.5 is required.",
                "Then stop."
            ]
        );
    }

    #[test]
    fn a_very_long_paragraph_is_split_into_pieces_within_the_limit() {
        let sentence =
            "The system shall keep an audit record of every administrative change made by a user. ";
        let text = sentence.repeat(40);
        let pieces = split_paragraph(text.trim());
        assert!(pieces.len() > 1);
        assert!(
            pieces.iter().all(|p| p.chars().count() <= MAX_ITEM_CHARS),
            "{:?}",
            pieces.iter().map(|p| p.chars().count()).collect::<Vec<_>>()
        );
        let rejoined = pieces.join(" ");
        assert_eq!(rejoined.matches("audit record").count(), 40);
    }

    #[test]
    fn a_single_sentence_longer_than_the_limit_is_cut_without_losing_words() {
        let words: Vec<String> = (0..600).map(|i| format!("word{i}")).collect();
        let text = format!("The system shall {}", words.join(" "));
        let pieces = split_paragraph(&text);
        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|p| p.chars().count() <= MAX_ITEM_CHARS));
        let rejoined = pieces.join(" ");
        for w in &words {
            assert!(rejoined.contains(w.as_str()), "lost {w}");
        }
    }

    #[test]
    fn the_id_prefix_comes_from_the_document_type_or_else_the_file_name() {
        assert_eq!(clean_label("BRD", "x.docx"), "BRD");
        assert_eq!(clean_label("Business Reqs", "x.docx"), "BUSINE");
        assert_eq!(clean_label("", "login-spec.md"), "LOGINS");
        assert_eq!(clean_label("", "---.md"), "DOC");
    }

    #[test]
    fn an_all_caps_line_after_a_blank_line_is_a_heading_but_lowercase_text_is_not() {
        let doc = parse(
            "\nFUNCTIONAL REQUIREMENTS\n\nThe system shall export reports.\n\nthe system shall email reports\n",
        );
        assert_eq!(doc.sections_count, 1);
        assert_eq!(
            doc.requirements[0].section.as_deref(),
            Some("FUNCTIONAL REQUIREMENTS")
        );
        assert_eq!(doc.requirements.len(), 2);
    }

    #[test]
    fn a_numbered_list_item_is_not_mistaken_for_a_heading() {
        let doc = parse(
            "The user signs in as follows:\n\n1. Enter your email address in the form\n2. Enter your password in the form\n",
        );
        assert_eq!(doc.sections_count, 0, "{:?}", texts(&doc));
        assert_eq!(doc.requirements.len(), 3);
    }

    #[test]
    fn a_numbered_heading_needs_no_markdown_and_a_sentence_with_a_number_is_not_one() {
        let doc = parse(
            "3 Authentication\n\nThe system shall authenticate every request.\n\n3.2 Login\n\nThe system shall lock the account.\n\n2.5 seconds is the target for the login response time.\n",
        );
        assert_eq!(doc.sections_count, 2);
        assert_eq!(
            doc.requirements[0].section.as_deref(),
            Some("3 Authentication")
        );
        assert_eq!(doc.requirements[1].section.as_deref(), Some("3.2 Login"));
        // "2.5 seconds ... time." ends like a sentence, so it is a requirement of section 3.2.
        assert!(doc
            .requirements
            .iter()
            .any(|r| r.text.starts_with("2.5 seconds")));
    }

    #[test]
    fn crlf_and_tabs_are_read_like_newlines_and_spaces() {
        let doc = parse("## 1 A\r\n\r\nFR-1:\tThe system shall work.\r\n");
        assert_eq!(keys(&doc), ["FR-1"]);
        assert_eq!(doc.requirements[0].text, "The system shall work.");
    }

    #[test]
    fn blank_and_whitespace_only_input_yields_nothing() {
        assert!(parse("").requirements.is_empty());
        assert!(parse("   \n\n  \t \n").requirements.is_empty());
    }

    #[test]
    fn a_short_label_with_no_content_is_not_a_requirement() {
        let doc = parse("## 1 Scope\n\nNotes\n\nThe system shall do a real thing.\n");
        assert_eq!(texts(&doc), ["The system shall do a real thing."]);
    }

    #[test]
    fn kinds_round_trip_through_their_stored_name() {
        for kind in [
            RequirementKind::Functional,
            RequirementKind::Security,
            RequirementKind::Performance,
            RequirementKind::Usability,
            RequirementKind::Interface,
            RequirementKind::Data,
            RequirementKind::Context,
            RequirementKind::Reference,
        ] {
            assert_eq!(RequirementKind::from_db(kind.as_str()), kind);
        }
        assert_eq!(
            RequirementKind::from_db("something else"),
            RequirementKind::Context
        );
    }
}
