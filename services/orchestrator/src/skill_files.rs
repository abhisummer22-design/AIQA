//! Filesystem-backed skills — real `SKILL.md` files a user drops directly
//! into `skills_dir()` (`services/orchestrator/skills/`), read from disk
//! rather than typed into a form or uploaded through the browser.
//!
//! A published SKILL.md is a YAML frontmatter block followed by a markdown
//! body. Both halves carry meaning, so this module keeps the split the
//! format itself defines:
//!
//!   - **frontmatter `name`** becomes the skill's name — the author already
//!     named it, so making a user re-type a name the file already states
//!     invites a mismatch between the file and the thing shown;
//!   - **the markdown body** becomes the instruction. That is what flows
//!     into a real generation prompt (via the same `SkillInput.description`
//!     field a database-backed custom skill already uses — see
//!     `ai_generation::resolve_skill_instructions`), so it must be the body
//!     alone: shipping the YAML header to the model would send bookkeeping
//!     (`license`, `agents`) as if it were guidance;
//!   - **frontmatter `description`** is kept as a short, honestly-labeled
//!     summary — never confused with the instruction;
//!   - **the rest of the frontmatter** is preserved as metadata: real
//!     information the file asserts about itself (version, author,
//!     frameworks, ...), kept beside the instruction, not inside it.
//!
//! The YAML subset parsed here is deliberately small (scalars and inline
//! `[a, b]` lists) because that is what skill frontmatter actually uses.
//! Anything else is a real, reported parse failure for that one file —
//! never a silent guess, and never something that stops every other file in
//! the folder from loading.

use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::OnceLock;

/// One real skill read from a `.md` file in `skills_dir()`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileSkill {
    /// Stable id derived from the file name, so re-scanning the folder
    /// (this module scans on every request — see `scan_skills_dir`) keeps
    /// the same id for the same file across requests.
    pub id: String,
    pub file_name: String,
    /// `name` from frontmatter, or the filename when the file states none.
    pub name: String,
    /// `description` from frontmatter — a short summary, never the
    /// instruction actually sent to the model.
    pub description: Option<String>,
    /// The markdown body: the real instruction used for generation.
    pub instruction: String,
    /// Every other frontmatter key, as the file wrote it.
    pub metadata: Value,
    pub had_frontmatter: bool,
    pub instruction_bytes: usize,
}

/// One file in `skills_dir()` that could not be read as a skill — kept
/// separate from `FileSkill` so one bad file never hides every other real
/// skill in the folder.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileSkillError {
    pub file_name: String,
    pub error: String,
}

/// The full, real state of `skills_dir()` right now.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct FileSkillScan {
    pub directory: String,
    pub skills: Vec<FileSkill>,
    pub errors: Vec<FileSkillError>,
}

/// `services/orchestrator/skills/` — resolved the same way every other
/// real on-disk location in this crate is (`main.rs::db_path`,
/// `evidence::default_root`): relative to the crate manifest, not the
/// process's current directory, so it is the same real folder regardless of
/// where the binary is launched from.
pub fn skills_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills")
}

fn frontmatter_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$").unwrap())
}

/// A block-style YAML list item ("- foo"). Only inline `[a, b]` lists are
/// supported; a block list is reported, not approximated.
fn is_block_list_line(line: &str) -> bool {
    line.strip_prefix('-')
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_whitespace()))
}

/// Strips one leading and one trailing quote character (independently —
/// mismatched quote characters each still get stripped, matching how a
/// hand-written frontmatter line is actually read rather than requiring a
/// perfectly paired quote).
fn strip_quotes(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    if matches!(chars.first(), Some('\'') | Some('"')) {
        chars.remove(0);
    }
    if matches!(chars.last(), Some('\'') | Some('"')) {
        chars.pop();
    }
    chars.into_iter().collect()
}

/// Parses the tiny YAML subset skill frontmatter actually uses: `key:
/// value` scalars and inline `key: [a, b, c]` lists. Nested maps and
/// block-style lists are not supported and are reported as a real error,
/// not approximated.
fn parse_frontmatter(block: &str) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();

    for raw in block.split('\n') {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if is_block_list_line(line) {
            return Err(
                "This file uses block-style YAML lists in its frontmatter, which this importer \
                 does not parse. Convert them to inline form (key: [a, b]) or import the skill by hand."
                    .to_string(),
            );
        }

        let Some(sep) = line.find(':') else {
            return Err(format!("Could not read the frontmatter line: \"{line}\""));
        };
        let key = line[..sep].trim();
        if key.is_empty() {
            return Err(format!("Frontmatter line has no key: \"{line}\""));
        }
        let value_raw = line[sep + 1..].trim();

        let value = if value_raw.is_empty() {
            // A declared-but-empty value is a real, valid "declared but unset".
            Value::Null
        } else if let Some(inner) = value_raw.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let items: Vec<Value> = inner
                .split(',')
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(|v| Value::String(strip_quotes(v)))
                .collect();
            Value::Array(items)
        } else {
            Value::String(strip_quotes(value_raw))
        };

        out.insert(key.to_string(), value);
    }

    Ok(out)
}

/// Mirrors the JS regex `/\.(SKILL\.)?(md|markdown)$/i` + `/[-_]+/g -> ' '`
/// fallback naming rule the browser-side importer used before this backend
/// folder replaced it: strip exactly one trailing extension, then turn
/// separators into spaces — never an invented label.
fn fallback_name_from_filename(file_name: &str) -> String {
    let lower = file_name.to_ascii_lowercase();
    let stem = [".skill.md", ".md", ".markdown"]
        .iter()
        .find_map(|suffix| {
            lower
                .ends_with(suffix)
                .then(|| &file_name[..file_name.len() - suffix.len()])
        })
        .unwrap_or(file_name);

    let mut out = String::new();
    let mut last_was_sep = false;
    for c in stem.chars() {
        if c == '-' || c == '_' {
            if !last_was_sep {
                out.push(' ');
            }
            last_was_sep = true;
        } else {
            out.push(c);
            last_was_sep = false;
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        file_name.to_string()
    } else {
        trimmed.to_string()
    }
}

/// A stable, filesystem-derived id — the same file name always produces the
/// same id across scans (this module re-scans on every request; see
/// `scan_skills_dir`), so a page that keeps a file skill's id in local
/// selection state does not lose that selection on the next fetch.
fn id_from_filename(file_name: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = false;
    for c in file_name.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    format!("file-skill-{}", out.trim_matches('-'))
}

/// Parses one real SKILL.md file's content. Returns `Err` with a real,
/// specific reason when the file cannot be read as a skill — never a
/// partially-guessed skill, because that would be shown in the UI and sent
/// to the model as if it were correct.
pub fn parse_skill_file(file_name: &str, content: &str) -> Result<FileSkill, String> {
    let text = content.strip_prefix('\u{feff}').unwrap_or(content);

    let (frontmatter, body, had_frontmatter) = match frontmatter_regex().captures(text) {
        Some(caps) => (
            parse_frontmatter(caps.get(1).map(|m| m.as_str()).unwrap_or(""))?,
            caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string(),
            true,
        ),
        // A plain markdown file is still a usable skill — its whole content
        // is the instruction. Reported as having no frontmatter so the UI
        // can say so rather than implying metadata was found.
        None => (Map::new(), text.to_string(), false),
    };

    let instruction = body.trim().to_string();
    if instruction.is_empty() {
        return Err(
            "This file has no content below its frontmatter, so there is no instruction to store."
                .to_string(),
        );
    }

    let name_value = frontmatter
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let description = frontmatter
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut metadata = Map::new();
    for (k, v) in frontmatter.into_iter() {
        if k == "name" || k == "description" {
            continue;
        }
        metadata.insert(k, v);
    }

    let instruction_bytes = instruction.len();
    Ok(FileSkill {
        id: id_from_filename(file_name),
        file_name: file_name.to_string(),
        name: name_value.unwrap_or_else(|| fallback_name_from_filename(file_name)),
        description,
        instruction,
        metadata: Value::Object(metadata),
        had_frontmatter,
        instruction_bytes,
    })
}

/// A drop-folder doc file, not a skill — never parsed as one regardless of
/// its `.md` extension.
fn is_readme(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case("README.md")
}

/// Scans `skills_dir()` right now and parses every real `.md` file in it.
/// Called fresh on every `GET /api/v1/skills/files` (not cached, not
/// scanned only at startup) — the whole point of a drop folder is that a
/// file the user just added shows up without a restart. The folder not
/// existing yet is a real, ordinary state (nothing has been dropped in
/// yet), not an error: it comes back as an empty scan rather than a 500.
pub fn scan_skills_dir() -> FileSkillScan {
    scan_dir(&skills_dir())
}

/// The real scan logic, parameterized on directory so tests exercise it
/// directly against a real, disposable temp directory instead of a second
/// copy of this logic drifting out of sync with it.
fn scan_dir(dir: &std::path::Path) -> FileSkillScan {
    let directory = dir.display().to_string();

    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return FileSkillScan { directory, skills: Vec::new(), errors: Vec::new() },
    };
    // Deterministic order so the UI list does not reshuffle between
    // otherwise-identical scans.
    entries.sort();

    let mut skills = Vec::new();
    let mut errors = Vec::new();

    for path in entries {
        if !path.is_file() {
            continue;
        }
        let is_markdown = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"));
        if !is_markdown {
            continue;
        }
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        if is_readme(&file_name) {
            continue;
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => match parse_skill_file(&file_name, &content) {
                Ok(skill) => skills.push(skill),
                Err(error) => errors.push(FileSkillError { file_name, error }),
            },
            Err(e) => errors.push(FileSkillError {
                file_name,
                error: format!("could not read file: {e}"),
            }),
        }
    }

    FileSkillScan { directory, skills, errors }
}

/// Creates `skills_dir()` if it does not exist yet, and drops a real README
/// into it (only if one is not already there — never overwrites a user's
/// own edits to it) explaining the format. Called once at startup
/// (`main.rs`); best-effort — a permissions failure here should not stop
/// the app from serving real traffic, so it logs rather than panics.
pub fn ensure_skills_dir() -> std::io::Result<PathBuf> {
    let dir = skills_dir();
    std::fs::create_dir_all(&dir)?;

    let readme = dir.join("README.md");
    if !readme.exists() {
        std::fs::write(
            &readme,
            "# AIQA skills folder\n\n\
             Drop real `SKILL.md` files in this folder to make them available for AI test-case \
             generation — no restart needed, this folder is re-read on every request.\n\n\
             Each file is a YAML frontmatter block followed by a markdown body:\n\n\
             ```\n\
             ---\n\
             name: My Skill\n\
             description: One-line summary shown in the UI\n\
             version: 1.0.0\n\
             frameworks: [playwright]\n\
             ---\n\n\
             The markdown body below the frontmatter is the real instruction sent to the model.\n\
             ```\n\n\
             The frontmatter block is optional — a plain `.md` file with no `---` header is still \
             read, using its entire content as the instruction and its filename as the name.\n\n\
             Only `key: value` scalars and inline `key: [a, b]` lists are supported in the \
             frontmatter; block-style YAML lists (`key:\\n  - a`) are reported as a parse error for \
             that file rather than guessed at.\n\n\
             This file (`README.md`) is never itself read as a skill.\n",
        )?;
    }

    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frontmatter copied verbatim from a real published skill file
    /// (`playwright-e2e.SKILL.md`), with a shortened body — the point of
    /// this parser is that an actual downloaded skill file imports
    /// correctly, not an invented sample.
    const REAL_SKILL: &str = "---\n\
        name: Playwright E2E Testing\n\
        description: Comprehensive Playwright end-to-end testing patterns with Page Object Model, fixtures, and best practices\n\
        version: 1.0.0\n\
        author: thetestingacademy\n\
        license: MIT\n\
        testingTypes: [e2e, visual]\n\
        frameworks: [playwright]\n\
        languages: [typescript, javascript]\n\
        domains: [web]\n\
        agents: [claude-code, cursor]\n\
        ---\n\
        \n\
        # Playwright E2E Testing Skill\n\
        \n\
        You are an expert QA automation engineer specializing in Playwright.\n\
        \n\
        ## Core Principles\n\
        \n\
        1. **User-centric testing** -- mirror real user journeys.\n";

    #[test]
    fn takes_the_name_and_description_from_the_real_frontmatter() {
        let p = parse_skill_file("playwright-e2e.SKILL.md", REAL_SKILL).unwrap();
        assert_eq!(p.name, "Playwright E2E Testing");
        assert_eq!(
            p.description.as_deref(),
            Some("Comprehensive Playwright end-to-end testing patterns with Page Object Model, fixtures, and best practices")
        );
        assert!(p.had_frontmatter);
    }

    #[test]
    fn uses_the_markdown_body_as_the_instruction_and_never_ships_the_yaml_header() {
        let p = parse_skill_file("playwright-e2e.SKILL.md", REAL_SKILL).unwrap();
        assert!(p.instruction.starts_with("# Playwright E2E Testing Skill"));
        assert!(p.instruction.contains("expert QA automation engineer"));
        assert!(!p.instruction.contains("license: MIT"));
        assert!(!p.instruction.contains("agents:"));
        assert!(!p.instruction.contains("---"));
    }

    #[test]
    fn preserves_the_rest_of_the_real_frontmatter_as_metadata_including_inline_lists() {
        let p = parse_skill_file("playwright-e2e.SKILL.md", REAL_SKILL).unwrap();
        let m = p.metadata.as_object().unwrap();
        assert_eq!(m.get("version").unwrap(), "1.0.0");
        assert_eq!(m.get("author").unwrap(), "thetestingacademy");
        assert_eq!(m.get("license").unwrap(), "MIT");
        assert_eq!(m.get("frameworks").unwrap(), &Value::Array(vec![Value::String("playwright".into())]));
        assert_eq!(
            m.get("testingTypes").unwrap(),
            &Value::Array(vec![Value::String("e2e".into()), Value::String("visual".into())])
        );
        assert_eq!(
            m.get("languages").unwrap(),
            &Value::Array(vec![Value::String("typescript".into()), Value::String("javascript".into())])
        );
        assert!(!m.contains_key("name"));
        assert!(!m.contains_key("description"));
    }

    #[test]
    fn reports_the_real_instruction_size() {
        let p = parse_skill_file("playwright-e2e.SKILL.md", REAL_SKILL).unwrap();
        assert_eq!(p.instruction_bytes, p.instruction.len());
        assert!(p.instruction_bytes > 50);
    }

    #[test]
    fn accepts_a_plain_markdown_file_with_no_frontmatter() {
        let p = parse_skill_file("my-notes.md", "# Rules\n\nAlways check currency rounding.\n").unwrap();
        assert!(!p.had_frontmatter);
        assert_eq!(p.description, None);
        assert!(p.instruction.contains("Always check currency rounding."));
        assert_eq!(p.name, "my notes");
    }

    #[test]
    fn falls_back_to_the_filename_only_when_the_file_genuinely_states_no_name() {
        let p = parse_skill_file("rbac-rules.SKILL.md", "---\nversion: 2\n---\n\nCheck role boundaries.\n").unwrap();
        assert_eq!(p.name, "rbac rules");
        assert_eq!(p.metadata.as_object().unwrap().get("version").unwrap(), "2");
    }

    #[test]
    fn rejects_a_file_whose_body_is_empty() {
        let err = parse_skill_file("empty.SKILL.md", "---\nname: Nothing\n---\n\n   \n").unwrap_err();
        assert!(err.contains("no content"));
    }

    #[test]
    fn reports_block_style_yaml_lists_instead_of_silently_mis_parsing_them() {
        let block_list = "---\nname: X\nframeworks:\n  - playwright\n---\n\nBody.\n";
        let err = parse_skill_file("x.SKILL.md", block_list).unwrap_err();
        assert!(err.contains("block-style YAML lists"));
    }

    #[test]
    fn treats_a_declared_but_empty_frontmatter_value_as_null() {
        let p = parse_skill_file("x.SKILL.md", "---\nname: X\nauthor:\n---\n\nBody.\n").unwrap();
        assert_eq!(p.metadata.as_object().unwrap().get("author").unwrap(), &Value::Null);
    }

    #[test]
    fn scan_of_a_missing_directory_is_empty_not_an_error() {
        let dir = std::env::temp_dir().join(format!("aiqa-skills-test-missing-{}", uuid::Uuid::new_v4()));
        assert!(!dir.exists());
        let scan = scan_dir(&dir);
        assert!(scan.skills.is_empty());
        assert!(scan.errors.is_empty());
        assert_eq!(scan.directory, dir.display().to_string());
    }

    #[test]
    fn scan_reads_real_files_skips_readme_and_non_markdown_and_sorts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), "# Not a skill\n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "ignored, wrong extension").unwrap();
        std::fs::write(dir.path().join("b-skill.md"), "Body B.\n").unwrap();
        std::fs::write(dir.path().join("a-skill.md"), "Body A.\n").unwrap();
        std::fs::write(dir.path().join("broken.md"), "---\nname: Bad\nlist:\n  - x\n---\n\nBody.\n").unwrap();

        let scan = scan_dir(dir.path());

        assert_eq!(scan.skills.len(), 2);
        assert_eq!(scan.skills[0].file_name, "a-skill.md");
        assert_eq!(scan.skills[1].file_name, "b-skill.md");
        assert_eq!(scan.errors.len(), 1);
        assert_eq!(scan.errors[0].file_name, "broken.md");
    }

    #[test]
    fn ids_are_stable_across_repeated_parses_of_the_same_filename() {
        let a = parse_skill_file("playwright-e2e.SKILL.md", "Body.\n").unwrap();
        let b = parse_skill_file("playwright-e2e.SKILL.md", "Body.\n").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.id, "file-skill-playwright-e2e-skill-md");
    }
}
