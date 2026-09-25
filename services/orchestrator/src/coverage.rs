//! The coverage engine for requirement-driven generation.
//!
//! Generating "a case per requirement" says nothing about *what kind* of case:
//! a run can hand back forty valid-input cases and not one that submits bad
//! input or probes a limit. This module says, per requirement, which
//! **techniques** it should be tested with at the depth the user asked for,
//! which it has been tested with, and what is still missing — and plans the
//! extra calls that would close the gaps (`plan_top_up`, run by
//! `requirement_generation.rs`).
//!
//! Everything here is pure (no calls, no clock), so the same input always
//! gives the same report:
//!
//! * a [`Technique`] is one way of testing: positive, negative, edge, security,
//!   performance, accessibility;
//! * [`applicable_techniques`] says which apply to a kind of requirement (a
//!   performance requirement has no "negative" case; a background paragraph
//!   has none at all);
//! * [`Depth`] turns "SMOKE / STANDARD / EXHAUSTIVE / CHAOS_EXPLORATORY" into a
//!   number: how many cases each applicable technique should give each
//!   requirement ([`target_for`]);
//! * a case counts for the technique it is labelled with (the model is asked
//!   to label it; when it does not, the technique of the skill that wrote it);
//! * [`build_matrix`] compares the two and reports the shortfalls, and
//!   [`needs`] / [`plan_top_up`] turn the shortfalls into batches of requirements
//!   that fit the provider, one technique at a time.
//!
//! A technique that no selected skill provides is never attempted — the user's
//! skill selection stays real — but it is reported, with the skill to select.

use crate::ai_generation::{normalize_title, GeneratedTestCaseDraft, SkillInput};
use crate::requirement_generation::{split_batches, BatchBudget, RequirementItem};
use crate::requirements::RequirementKind;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

// -------------------------------------------------------------------- technique

/// One way of testing a requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Technique {
    /// Valid, well-formed, authorised input; the behaviour the requirement describes.
    Positive,
    /// Invalid, malformed, missing or unauthorised input that must be rejected.
    Negative,
    /// The limits: boundary values, empty, maximum, unusual sequences.
    Edge,
    /// Authentication, authorisation, injection, data exposure, abuse.
    Security,
    /// Response time, load, volume, availability.
    Performance,
    /// Keyboard, screen reader, contrast, responsive layout, clear feedback.
    Accessibility,
}

impl Technique {
    pub const ALL: [Technique; 6] = [
        Technique::Positive,
        Technique::Negative,
        Technique::Edge,
        Technique::Security,
        Technique::Performance,
        Technique::Accessibility,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Technique::Positive => "positive",
            Technique::Negative => "negative",
            Technique::Edge => "edge",
            Technique::Security => "security",
            Technique::Performance => "performance",
            Technique::Accessibility => "accessibility",
        }
    }

    /// Reads a technique as a model writes it: "Negative testing", "edge-case",
    /// "boundary value", "a11y". `None` for anything else.
    pub fn parse(label: &str) -> Option<Technique> {
        let lower = label.trim().to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        let has = |prefix: &str| words.iter().any(|w| w.starts_with(prefix));
        if has("negative") {
            Some(Technique::Negative)
        } else if has("edge") || has("boundary") {
            Some(Technique::Edge)
        } else if has("security") || has("penetration") {
            Some(Technique::Security)
        } else if has("performance") || has("load") || has("spike") || has("stress") {
            Some(Technique::Performance)
        } else if has("accessib") || has("a11y") || has("usab") {
            Some(Technique::Accessibility)
        } else if has("positive") || has("happy") {
            Some(Technique::Positive)
        } else {
            None
        }
    }

    /// What the technique means, in the words a generation prompt uses.
    pub fn definition(self) -> &'static str {
        match self {
            Technique::Positive => {
                "the valid, expected behaviour the requirement describes, with well-formed and authorised input"
            }
            Technique::Negative => {
                "invalid, malformed, missing or unauthorised input, or a forbidden action, that the system must reject or refuse, \
                 checking the failure behaviour the requirement (or its context) gives a basis for"
            }
            Technique::Edge => {
                "the limits of what the requirement describes: boundary values, empty or null values, maximum and minimum, \
                 unusual sequences and combinations"
            }
            Technique::Security => {
                "authentication, authorisation, injection, data-exposure and abuse scenarios for the security-relevant \
                 behaviour the requirement describes"
            }
            Technique::Performance => {
                "response time, load, volume, availability or capacity, using only the figures the requirement states"
            }
            Technique::Accessibility => {
                "keyboard use, screen-reader labelling, contrast, responsive layout and clear feedback for the user-facing \
                 behaviour the requirement describes"
            }
        }
    }
}

/// The techniques a built-in skill is about. A custom skill provides none:
/// nothing is assumed about what it does, so it is never picked to fill a gap.
pub fn techniques_of_skill(skill_name: &str) -> &'static [Technique] {
    use Technique::*;
    match skill_name.trim().to_lowercase().as_str() {
        "functional testing"
        | "positive testing"
        | "business case testing"
        | "api testing"
        | "database testing" => &[Positive],
        "negative testing" => &[Negative],
        "edge case analysis" => &[Edge],
        "security testing" | "rbac testing" => &[Security],
        "performance testing" | "load testing" | "spike testing" => &[Performance],
        "ui/ux testing" => &[Accessibility],
        "non-functional testing" => &[Performance, Security, Accessibility],
        _ => &[],
    }
}

/// The technique a case counts for: the model's own label when it is one of
/// ours, otherwise `fallback` (what the skill that wrote it is about, or the
/// technique a top-up call asked for).
pub fn resolve_technique(label: Option<&str>, fallback: Technique) -> Technique {
    label.and_then(Technique::parse).unwrap_or(fallback)
}

/// For each technique, the first selected skill (in selection order) that
/// provides it.
pub fn providers(skills: &[SkillInput]) -> BTreeMap<Technique, SkillInput> {
    let mut out = BTreeMap::new();
    for skill in skills {
        for &technique in techniques_of_skill(&skill.name) {
            out.entry(technique).or_insert_with(|| skill.clone());
        }
    }
    out
}

// ------------------------------------------------------------------------ depth

/// How much coverage the user asked for (the page's "Exploration Depth").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// One case per requirement, of the technique that matters most for it.
    Smoke,
    /// Every technique that applies, once.
    Standard,
    /// Every technique that applies, twice.
    Exhaustive,
    /// Every technique that applies, three times.
    Chaos,
}

impl Depth {
    pub fn parse(text: &str) -> Option<Depth> {
        match text.trim().to_uppercase().replace(['-', ' '], "_").as_str() {
            "SMOKE" => Some(Depth::Smoke),
            "STANDARD" => Some(Depth::Standard),
            "EXHAUSTIVE" => Some(Depth::Exhaustive),
            "CHAOS_EXPLORATORY" | "CHAOS" | "EXPLORATORY" => Some(Depth::Chaos),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Depth::Smoke => "SMOKE",
            Depth::Standard => "STANDARD",
            Depth::Exhaustive => "EXHAUSTIVE",
            Depth::Chaos => "CHAOS_EXPLORATORY",
        }
    }

    /// Cases each technique that applies should give one requirement.
    pub fn per_technique(self) -> usize {
        match self {
            Depth::Smoke | Depth::Standard => 1,
            Depth::Exhaustive => 2,
            Depth::Chaos => 3,
        }
    }
}

/// The techniques that make sense for a kind of requirement, the most
/// important first. Background text and raw reference material have none.
pub fn applicable_techniques(kind: RequirementKind) -> &'static [Technique] {
    use Technique as T;
    match kind {
        RequirementKind::Functional | RequirementKind::Data | RequirementKind::Interface => {
            &[T::Positive, T::Negative, T::Edge]
        }
        RequirementKind::Security => &[T::Security, T::Negative],
        RequirementKind::Performance => &[T::Performance],
        RequirementKind::Usability => &[T::Accessibility],
        RequirementKind::Context | RequirementKind::Reference => &[],
    }
}

/// How many cases `technique` should give a requirement of `kind` at `depth`.
pub fn target_for(depth: Depth, kind: RequirementKind, technique: Technique) -> usize {
    let applicable = applicable_techniques(kind);
    if !applicable.contains(&technique) {
        return 0;
    }
    match depth {
        // A smoke run wants the requirement exercised once, by its main technique.
        Depth::Smoke => usize::from(applicable[0] == technique),
        other => other.per_technique(),
    }
}

// ----------------------------------------------------------------------- matrix

/// One requirement's standing for one technique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub technique: Technique,
    pub have: usize,
    pub target: usize,
}

impl Cell {
    pub fn short(&self) -> usize {
        self.target.saturating_sub(self.have)
    }
}

/// For every requirement, its cell for each technique that has a target at
/// `depth`. Only cases tied to a requirement count; a case with no technique
/// label counts as positive.
pub fn analyze(
    items: &[RequirementItem],
    cases: &[GeneratedTestCaseDraft],
    depth: Depth,
) -> Vec<Vec<Cell>> {
    let index: HashMap<&str, usize> = items
        .iter()
        .enumerate()
        .map(|(i, r)| (r.key.as_str(), i))
        .collect();
    let mut have: HashMap<(usize, Technique), usize> = HashMap::new();
    for case in cases {
        let Some(&i) = case.requirement_id.as_deref().and_then(|id| index.get(id)) else {
            continue;
        };
        let technique = resolve_technique(case.technique.as_deref(), Technique::Positive);
        *have.entry((i, technique)).or_insert(0) += 1;
    }
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            Technique::ALL
                .iter()
                .filter_map(|&technique| {
                    let target = target_for(depth, item.kind, technique);
                    (target > 0).then(|| Cell {
                        technique,
                        have: have.get(&(i, technique)).copied().unwrap_or(0),
                        target,
                    })
                })
                .collect()
        })
        .collect()
}

/// A shortfall a top-up call can try to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need {
    pub item: usize,
    pub technique: Technique,
    pub missing: usize,
}

/// The shortfalls in techniques some selected skill provides, in requirement
/// order.
pub fn needs(
    items: &[RequirementItem],
    cases: &[GeneratedTestCaseDraft],
    depth: Depth,
    provided: &HashSet<Technique>,
) -> Vec<Need> {
    analyze(items, cases, depth)
        .into_iter()
        .enumerate()
        .flat_map(|(item, cells)| {
            cells
                .into_iter()
                .filter(|c| c.short() > 0 && provided.contains(&c.technique))
                .map(move |c| Need {
                    item,
                    technique: c.technique,
                    missing: c.short(),
                })
        })
        .collect()
}

/// One technique's standing across the whole requirement set.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TechniqueSummary {
    pub technique: Technique,
    /// Requirements this technique has a target for at this depth.
    pub applicable: usize,
    /// Of those, the ones that reached the target.
    pub met: usize,
    /// Of those, the ones that did not.
    pub short: usize,
    /// The selected skill that provides it, or `None` when none does (then it
    /// is never attempted, and `short` is what selecting one would address).
    pub provided_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MissingTechnique {
    pub technique: Technique,
    pub have: usize,
    pub target: usize,
}

/// A requirement that is below target for a technique a selected skill provides.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RequirementGap {
    pub key: String,
    pub kind: &'static str,
    pub missing: Vec<MissingTechnique>,
}

/// What a top-up did, if one ran.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TopUpSummary {
    pub rounds: usize,
    pub calls: usize,
    pub cases_added: usize,
    /// Why it stopped, in words.
    pub stopped: String,
}

/// The coverage report of a run.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CoverageMatrix {
    pub depth: &'static str,
    pub techniques: Vec<TechniqueSummary>,
    /// Requirements below target for an attemptable technique (at most
    /// [`MAX_LISTED_GAPS`]; `gap_total` is the real count).
    pub gaps: Vec<RequirementGap>,
    pub gap_total: usize,
    /// Requirements that reached the target for every technique that applies.
    pub complete: usize,
    /// Requirements no technique applies to (background text, reference material).
    pub not_applicable: usize,
    /// Cases that could not be tied to a requirement, so count for none.
    pub unlinked_cases: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_up: Option<TopUpSummary>,
}

pub const MAX_LISTED_GAPS: usize = 200;

pub fn build_matrix(
    items: &[RequirementItem],
    cases: &[GeneratedTestCaseDraft],
    depth: Depth,
    skills: &[SkillInput],
    top_up: Option<TopUpSummary>,
) -> CoverageMatrix {
    let provided = providers(skills);
    let rows = analyze(items, cases, depth);
    let mut applicable: HashMap<Technique, (usize, usize)> = HashMap::new(); // (applicable, met)
    let mut gaps = Vec::new();
    let mut gap_total = 0usize;
    let mut complete = 0usize;
    let mut not_applicable = 0usize;
    for (item, cells) in items.iter().zip(&rows) {
        if cells.is_empty() {
            not_applicable += 1;
            continue;
        }
        let mut all_met = true;
        let mut missing = Vec::new();
        for cell in cells {
            let entry = applicable.entry(cell.technique).or_insert((0, 0));
            entry.0 += 1;
            if cell.short() == 0 {
                entry.1 += 1;
            } else {
                all_met = false;
                if provided.contains_key(&cell.technique) {
                    missing.push(MissingTechnique {
                        technique: cell.technique,
                        have: cell.have,
                        target: cell.target,
                    });
                }
            }
        }
        if all_met {
            complete += 1;
        }
        if !missing.is_empty() {
            gap_total += 1;
            if gaps.len() < MAX_LISTED_GAPS {
                gaps.push(RequirementGap {
                    key: item.key.clone(),
                    kind: item.kind.as_str(),
                    missing,
                });
            }
        }
    }
    let techniques = Technique::ALL
        .iter()
        .filter_map(|&technique| {
            let (count, met) = applicable.get(&technique).copied()?;
            Some(TechniqueSummary {
                technique,
                applicable: count,
                met,
                short: count - met,
                provided_by: provided.get(&technique).map(|s| s.name.clone()),
            })
        })
        .collect();
    CoverageMatrix {
        depth: depth.label(),
        techniques,
        gaps,
        gap_total,
        complete,
        not_applicable,
        unlinked_cases: cases.iter().filter(|c| c.requirement_id.is_none()).count(),
        top_up,
    }
}

// ----------------------------------------------------------------------- top-up

/// The order techniques are topped up in when the call limit cannot cover all
/// of them: the ones people most often find missing first.
pub const TOP_UP_ORDER: [Technique; 6] = [
    Technique::Negative,
    Technique::Edge,
    Technique::Security,
    Technique::Performance,
    Technique::Accessibility,
    Technique::Positive,
];

/// One top-up call: a technique and the requirements it is asked for, each with
/// how many more cases it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopUpBatch {
    pub technique: Technique,
    /// `(index into the requirement list, cases still missing)`.
    pub entries: Vec<(usize, usize)>,
}

/// Groups `needs` by technique and cuts each group into batches that fit
/// `budget`, in `TOP_UP_ORDER`, requirements in document order.
pub fn plan_top_up(
    items: &[RequirementItem],
    needs: &[Need],
    budget: BatchBudget,
) -> Vec<TopUpBatch> {
    let mut batches = Vec::new();
    for technique in TOP_UP_ORDER {
        let entries: Vec<&Need> = needs.iter().filter(|n| n.technique == technique).collect();
        if entries.is_empty() {
            continue;
        }
        let indices: Vec<usize> = entries.iter().map(|n| n.item).collect();
        for group in split_batches(items, &indices, budget) {
            batches.push(TopUpBatch {
                technique,
                entries: group
                    .iter()
                    .filter_map(|&i| entries.iter().find(|n| n.item == i))
                    .map(|n| (n.item, n.missing))
                    .collect(),
            });
        }
    }
    batches
}

// ---------------------------------------------------------------- near duplicates

/// Words that say nothing about what a case tests.
const FILLER: [&str; 12] = [
    "the", "and", "for", "with", "that", "when", "then", "from", "into", "verify", "test", "check",
];

fn title_words(title: &str) -> HashSet<String> {
    normalize_title(title)
        .split_whitespace()
        .filter(|w| w.chars().count() >= 3 && !FILLER.contains(w))
        .map(|w| {
            // "passwords" and "password" are the same word here.
            if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
                w[..w.len() - 1].to_string()
            } else {
                w.to_string()
            }
        })
        .collect()
}

/// Two titles that say the same thing in slightly different words: at least
/// four in five of their significant words are shared.
pub fn is_near_duplicate(a: &str, b: &str) -> bool {
    let (wa, wb) = (title_words(a), title_words(b));
    if wa.is_empty() || wb.is_empty() {
        return false;
    }
    let shared = wa.intersection(&wb).count();
    let union = wa.len() + wb.len() - shared;
    shared * 5 >= union * 4
}

// -------------------------------------------------------------------- plan preview

/// One technique's line in the plan shown before a run.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TechniquePlan {
    pub technique: Technique,
    /// Requirements it has a target for at this depth.
    pub requirements: usize,
    /// Cases each of those should get.
    pub per_requirement: usize,
    pub provided_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CoveragePlan {
    pub depth: &'static str,
    pub techniques: Vec<TechniquePlan>,
    pub top_up: bool,
    /// Rounds of gap-filling a top-up may make, and the most calls all of them
    /// together may add (what is left of the request's call limit).
    pub top_up_rounds: usize,
    pub top_up_call_limit: usize,
}

pub fn plan_info(
    items: &[RequirementItem],
    skills: &[SkillInput],
    depth: Depth,
    top_up: bool,
    rounds: usize,
    call_limit: usize,
) -> CoveragePlan {
    let provided = providers(skills);
    let techniques = Technique::ALL
        .iter()
        .filter_map(|&technique| {
            let requirements = items
                .iter()
                .filter(|r| target_for(depth, r.kind, technique) > 0)
                .count();
            (requirements > 0).then(|| TechniquePlan {
                technique,
                requirements,
                per_requirement: depth.per_technique(),
                provided_by: provided.get(&technique).map(|s| s.name.clone()),
            })
        })
        .collect();
    CoveragePlan {
        depth: depth.label(),
        techniques,
        top_up,
        top_up_rounds: rounds,
        top_up_call_limit: call_limit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_generation::{DraftStep, EvidenceKind};

    fn item(key: &str, kind: RequirementKind) -> RequirementItem {
        RequirementItem {
            key: key.to_string(),
            section: Some("3 Scope".to_string()),
            kind,
            text: format!("The system shall satisfy {key} in every case the specification lists."),
        }
    }

    fn case(requirement: Option<&str>, technique: Option<&str>) -> GeneratedTestCaseDraft {
        GeneratedTestCaseDraft {
            title: format!("Case for {requirement:?} {technique:?}"),
            category: "Functional".to_string(),
            skill_id: "skill-functional-testing".to_string(),
            preconditions: vec![],
            steps: vec![DraftStep {
                action: "Do it".to_string(),
                expected_result: "It works".to_string(),
            }],
            expected_result: "It works".to_string(),
            priority: "HIGH".to_string(),
            severity: "MAJOR".to_string(),
            evidence: EvidenceKind::Inferred,
            source_reference: None,
            requirement_id: requirement.map(str::to_string),
            technique: technique.map(str::to_string),
        }
    }

    fn skill(name: &str) -> SkillInput {
        SkillInput {
            id: format!("skill-{}", name.to_lowercase().replace(' ', "-")),
            name: name.to_string(),
            description: String::new(),
        }
    }

    fn budget(max_chars: usize) -> BatchBudget {
        BatchBudget {
            max_chars,
            max_items: 30,
        }
    }

    // --------------------------------------------------------------- vocabulary

    #[test]
    fn a_technique_is_read_as_a_model_writes_it() {
        for (label, technique) in [
            ("positive", Technique::Positive),
            ("Negative testing", Technique::Negative),
            ("edge-case", Technique::Edge),
            ("Boundary value", Technique::Edge),
            ("SECURITY", Technique::Security),
            ("load", Technique::Performance),
            ("Performance / stress", Technique::Performance),
            ("a11y", Technique::Accessibility),
            ("Accessibility", Technique::Accessibility),
            ("usability", Technique::Accessibility),
            ("  Happy path ", Technique::Positive),
        ] {
            assert_eq!(Technique::parse(label), Some(technique), "{label}");
        }
        for label in ["", "  ", "exploratory", "knowledge base", "functional"] {
            assert_eq!(Technique::parse(label), None, "{label:?}");
        }
        for t in Technique::ALL {
            assert_eq!(
                Technique::parse(t.as_str()),
                Some(t),
                "{t:?} must read back"
            );
        }
    }

    #[test]
    fn a_depth_is_read_case_insensitively_and_each_names_a_number_of_cases() {
        assert_eq!(Depth::parse("standard"), Some(Depth::Standard));
        assert_eq!(Depth::parse(" Chaos-Exploratory "), Some(Depth::Chaos));
        assert_eq!(Depth::parse("EXHAUSTIVE"), Some(Depth::Exhaustive));
        assert_eq!(Depth::parse("deep"), None);
        assert_eq!(
            [
                Depth::Smoke,
                Depth::Standard,
                Depth::Exhaustive,
                Depth::Chaos
            ]
            .map(Depth::per_technique),
            [1, 1, 2, 3]
        );
        for d in [
            Depth::Smoke,
            Depth::Standard,
            Depth::Exhaustive,
            Depth::Chaos,
        ] {
            assert_eq!(Depth::parse(d.label()), Some(d));
        }
    }

    #[test]
    fn what_applies_follows_the_kind_and_a_smoke_run_wants_only_the_main_technique() {
        use RequirementKind as K;
        use Technique as T;
        assert_eq!(
            applicable_techniques(K::Functional),
            &[T::Positive, T::Negative, T::Edge]
        );
        assert_eq!(applicable_techniques(K::Performance), &[T::Performance]);
        assert!(applicable_techniques(K::Context).is_empty());
        assert!(applicable_techniques(K::Reference).is_empty());

        assert_eq!(target_for(Depth::Standard, K::Functional, T::Negative), 1);
        assert_eq!(target_for(Depth::Exhaustive, K::Functional, T::Edge), 2);
        assert_eq!(target_for(Depth::Chaos, K::Data, T::Positive), 3);
        assert_eq!(
            target_for(Depth::Standard, K::Performance, T::Negative),
            0,
            "no negative case for a performance requirement"
        );
        assert_eq!(target_for(Depth::Standard, K::Context, T::Positive), 0);
        assert_eq!(target_for(Depth::Smoke, K::Functional, T::Positive), 1);
        assert_eq!(target_for(Depth::Smoke, K::Functional, T::Negative), 0);
        assert_eq!(
            target_for(Depth::Smoke, K::Security, T::Security),
            1,
            "a security requirement's main technique is security"
        );
        assert_eq!(target_for(Depth::Smoke, K::Security, T::Negative), 0);
    }

    #[test]
    fn a_skill_provides_the_techniques_it_is_about_and_a_custom_skill_provides_none() {
        use Technique::*;
        assert_eq!(techniques_of_skill("Negative Testing"), &[Negative]);
        assert_eq!(techniques_of_skill("  edge case ANALYSIS"), &[Edge]);
        assert_eq!(techniques_of_skill("Functional Testing"), &[Positive]);
        assert_eq!(
            techniques_of_skill("Non-Functional Testing"),
            &[Performance, Security, Accessibility]
        );
        assert_eq!(techniques_of_skill("UI/UX Testing"), &[Accessibility]);
        assert!(techniques_of_skill("Boundary Testing for Indian PIN Codes").is_empty());
    }

    #[test]
    fn the_first_selected_skill_that_provides_a_technique_is_its_provider() {
        let picked = providers(&[
            skill("Functional Testing"),
            skill("Positive Testing"),
            skill("Non-Functional Testing"),
            skill("Security Testing"),
            skill("My Custom Skill"),
        ]);
        assert_eq!(picked[&Technique::Positive].name, "Functional Testing");
        assert_eq!(picked[&Technique::Security].name, "Non-Functional Testing");
        assert_eq!(
            picked[&Technique::Performance].name,
            "Non-Functional Testing"
        );
        assert!(!picked.contains_key(&Technique::Negative));
        assert!(!picked.contains_key(&Technique::Edge));
    }

    #[test]
    fn a_case_counts_for_its_own_label_and_else_for_the_fallback() {
        assert_eq!(
            resolve_technique(Some("negative"), Technique::Positive),
            Technique::Negative
        );
        assert_eq!(
            resolve_technique(Some("nonsense"), Technique::Edge),
            Technique::Edge
        );
        assert_eq!(
            resolve_technique(None, Technique::Security),
            Technique::Security
        );
    }

    // ------------------------------------------------------------------ matrix

    #[test]
    fn analysis_counts_the_cases_of_each_requirement_by_technique() {
        let items = vec![
            item("FR-1", RequirementKind::Functional),
            item("NFR-1", RequirementKind::Performance),
        ];
        let cases = vec![
            case(Some("FR-1"), Some("positive")),
            case(Some("FR-1"), Some("negative")),
            case(Some("FR-1"), Some("negative")),
            case(Some("FR-1"), None),
            case(Some("NFR-1"), Some("performance")),
            case(None, Some("edge")),
            case(Some("GHOST"), Some("edge")),
        ];

        let rows = analyze(&items, &cases, Depth::Standard);

        assert_eq!(
            rows[0],
            vec![
                Cell {
                    technique: Technique::Positive,
                    have: 2,
                    target: 1
                },
                Cell {
                    technique: Technique::Negative,
                    have: 2,
                    target: 1
                },
                Cell {
                    technique: Technique::Edge,
                    have: 0,
                    target: 1
                },
            ],
            "an unlabelled case counts as positive; an unlinked or unknown one counts for nothing"
        );
        assert_eq!(
            rows[1],
            vec![Cell {
                technique: Technique::Performance,
                have: 1,
                target: 1
            }]
        );
    }

    #[test]
    fn a_shortfall_is_the_target_less_what_there_is_and_never_negative() {
        let cell = Cell {
            technique: Technique::Edge,
            have: 1,
            target: 3,
        };
        assert_eq!(cell.short(), 2);
        assert_eq!(
            Cell {
                technique: Technique::Edge,
                have: 5,
                target: 3
            }
            .short(),
            0
        );
    }

    #[test]
    fn needs_list_only_the_techniques_a_selected_skill_provides() {
        let items = vec![
            item("FR-1", RequirementKind::Functional),
            item("FR-2", RequirementKind::Functional),
        ];
        let cases = vec![
            case(Some("FR-1"), Some("positive")),
            case(Some("FR-2"), Some("positive")),
            case(Some("FR-2"), Some("negative")),
        ];
        let all: HashSet<Technique> = Technique::ALL.into_iter().collect();
        let only_negative: HashSet<Technique> = [Technique::Negative].into_iter().collect();

        let n = needs(&items, &cases, Depth::Standard, &all);
        assert_eq!(
            n,
            vec![
                Need {
                    item: 0,
                    technique: Technique::Negative,
                    missing: 1
                },
                Need {
                    item: 0,
                    technique: Technique::Edge,
                    missing: 1
                },
                Need {
                    item: 1,
                    technique: Technique::Edge,
                    missing: 1
                },
            ]
        );
        assert_eq!(
            needs(&items, &cases, Depth::Standard, &only_negative),
            vec![Need {
                item: 0,
                technique: Technique::Negative,
                missing: 1
            }]
        );
        assert_eq!(
            needs(&items, &cases, Depth::Exhaustive, &only_negative).len(),
            2,
            "two are wanted at exhaustive depth"
        );
    }

    #[test]
    fn the_matrix_reports_each_technique_the_gaps_and_what_no_technique_applies_to() {
        let items = vec![
            item("FR-1", RequirementKind::Functional),
            item("FR-2", RequirementKind::Functional),
            item("NFR-1", RequirementKind::Performance),
            item("BG-1", RequirementKind::Context),
        ];
        let cases = vec![
            case(Some("FR-1"), Some("positive")),
            case(Some("FR-1"), Some("negative")),
            case(Some("FR-1"), Some("edge")),
            case(Some("FR-2"), Some("positive")),
            case(Some("NFR-1"), Some("performance")),
            case(None, None),
        ];
        // Negative and Functional are selected; Edge is not.
        let skills = [
            skill("Functional Testing"),
            skill("Negative Testing"),
            skill("Performance Testing"),
        ];

        let m = build_matrix(&items, &cases, Depth::Standard, &skills, None);

        assert_eq!(m.depth, "STANDARD");
        assert_eq!(m.complete, 2, "FR-1 and NFR-1 reached every target");
        assert_eq!(m.not_applicable, 1);
        assert_eq!(m.unlinked_cases, 1);
        let by = |t: Technique| {
            m.techniques
                .iter()
                .find(|s| s.technique == t)
                .unwrap()
                .clone()
        };
        assert_eq!(
            (
                by(Technique::Positive).applicable,
                by(Technique::Positive).met,
                by(Technique::Positive).short
            ),
            (2, 2, 0)
        );
        assert_eq!(
            (
                by(Technique::Negative).applicable,
                by(Technique::Negative).met,
                by(Technique::Negative).short
            ),
            (2, 1, 1)
        );
        assert_eq!(
            by(Technique::Negative).provided_by.as_deref(),
            Some("Negative Testing")
        );
        assert_eq!(
            (by(Technique::Edge).applicable, by(Technique::Edge).short),
            (2, 1)
        );
        assert_eq!(
            by(Technique::Edge).provided_by,
            None,
            "no selected skill provides edge cases"
        );
        assert!(
            m.techniques
                .iter()
                .all(|s| s.technique != Technique::Security),
            "a technique nothing applies to is not listed"
        );
        // FR-2 lacks negative (attemptable) and edge (not provided): only the first is a gap.
        assert_eq!(m.gap_total, 1);
        assert_eq!(
            m.gaps,
            vec![RequirementGap {
                key: "FR-2".to_string(),
                kind: "functional",
                missing: vec![MissingTechnique {
                    technique: Technique::Negative,
                    have: 0,
                    target: 1
                }],
            }]
        );
    }

    #[test]
    fn the_gap_list_is_capped_but_the_count_is_real() {
        let items: Vec<RequirementItem> = (0..250)
            .map(|i| item(&format!("R{i}"), RequirementKind::Functional))
            .collect();
        let m = build_matrix(
            &items,
            &[],
            Depth::Standard,
            &[skill("Functional Testing")],
            None,
        );
        assert_eq!(m.gap_total, 250);
        assert_eq!(m.gaps.len(), MAX_LISTED_GAPS);
        assert_eq!(m.complete, 0);
    }

    // ------------------------------------------------------------------ top-up

    fn wide_items(n: usize) -> Vec<RequirementItem> {
        (1..=n)
            .map(|i| item(&format!("R{i}"), RequirementKind::Functional))
            .collect()
    }

    #[test]
    fn top_up_batches_are_one_technique_each_in_the_order_people_miss_them_and_lose_nobody() {
        let items = wide_items(40);
        let all: HashSet<Technique> = Technique::ALL.into_iter().collect();
        let n = needs(&items, &[], Depth::Standard, &all);
        assert_eq!(n.len(), 120, "positive, negative and edge for each of 40");

        let batches = plan_top_up(&items, &n, budget(1_500));

        let order: Vec<Technique> = batches.iter().map(|b| b.technique).collect();
        let first_negative = order
            .iter()
            .position(|t| *t == Technique::Negative)
            .unwrap();
        let first_edge = order.iter().position(|t| *t == Technique::Edge).unwrap();
        let first_positive = order
            .iter()
            .position(|t| *t == Technique::Positive)
            .unwrap();
        assert!(first_negative < first_edge && first_edge < first_positive);
        for technique in [Technique::Negative, Technique::Edge, Technique::Positive] {
            let entries: Vec<usize> = batches
                .iter()
                .filter(|b| b.technique == technique)
                .flat_map(|b| b.entries.iter().map(|e| e.0))
                .collect();
            assert_eq!(
                entries,
                (0..40).collect::<Vec<_>>(),
                "{technique:?}: every requirement once, in document order"
            );
        }
        assert!(batches.len() > 3, "a small budget means several calls");
        assert!(batches.iter().all(|b| !b.entries.is_empty()));
    }

    #[test]
    fn each_entry_carries_how_many_more_cases_it_needs() {
        let items = wide_items(2);
        let cases = vec![case(Some("R1"), Some("negative"))];
        let all: HashSet<Technique> = [Technique::Negative].into_iter().collect();

        let n = needs(&items, &cases, Depth::Chaos, &all);
        let batches = plan_top_up(&items, &n, budget(10_000));

        assert_eq!(batches.len(), 1);
        assert_eq!(
            batches[0].entries,
            vec![(0, 2), (1, 3)],
            "three are wanted at chaos depth, R1 already has one"
        );
    }

    #[test]
    fn nothing_needed_plans_nothing() {
        assert!(plan_top_up(&wide_items(3), &[], budget(1_000)).is_empty());
    }

    // ------------------------------------------------------------ near duplicates

    #[test]
    fn titles_that_say_the_same_thing_in_other_words_are_near_duplicates() {
        assert!(is_near_duplicate(
            "Reject a password shorter than eight characters",
            "Reject passwords shorter than eight characters"
        ));
        assert!(is_near_duplicate(
            "Verify login fails with a wrong password",
            "Test login fails with wrong password"
        ));
        assert!(!is_near_duplicate(
            "Reject a password shorter than eight characters",
            "Reject a password without a digit"
        ));
        assert!(!is_near_duplicate(
            "Lock the account after five failed attempts",
            "Unlock the account after thirty minutes"
        ));
        assert!(!is_near_duplicate("", "Anything at all"));
        assert!(!is_near_duplicate("a", "b"));
    }

    // ------------------------------------------------------------- plan preview

    #[test]
    fn the_plan_says_which_techniques_apply_to_how_many_requirements_and_who_provides_them() {
        let items = vec![
            item("FR-1", RequirementKind::Functional),
            item("FR-2", RequirementKind::Data),
            item("NFR-1", RequirementKind::Performance),
            item("BG-1", RequirementKind::Context),
        ];
        let skills = [skill("Functional Testing"), skill("Edge Case Analysis")];

        let p = plan_info(&items, &skills, Depth::Exhaustive, true, 2, 40);

        assert_eq!(
            (p.depth, p.top_up, p.top_up_rounds, p.top_up_call_limit),
            ("EXHAUSTIVE", true, 2, 40)
        );
        let of = |t: Technique| p.techniques.iter().find(|x| x.technique == t).cloned();
        assert_eq!(
            of(Technique::Positive),
            Some(TechniquePlan {
                technique: Technique::Positive,
                requirements: 2,
                per_requirement: 2,
                provided_by: Some("Functional Testing".to_string())
            })
        );
        assert_eq!(
            of(Technique::Edge).unwrap().provided_by.as_deref(),
            Some("Edge Case Analysis")
        );
        assert_eq!(of(Technique::Negative).unwrap().provided_by, None);
        assert_eq!(of(Technique::Performance).unwrap().requirements, 1);
        assert!(of(Technique::Security).is_none() && of(Technique::Accessibility).is_none());

        let smoke = plan_info(&items, &skills, Depth::Smoke, false, 2, 0);
        assert_eq!(
            smoke
                .techniques
                .iter()
                .find(|x| x.technique == Technique::Negative)
                .map(|x| x.requirements),
            None,
            "a smoke run has no negative target"
        );
    }
}
