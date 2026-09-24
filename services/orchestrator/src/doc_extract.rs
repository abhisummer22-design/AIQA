//! Real Product Documentation text extraction (Test Case Generator).
//!
//! Before this module, an uploaded PRD/FRD/SRD/Additional document was
//! never actually read: `TestCaseGeneratorPage.tsx`'s own
//! `buildRequirementText()` only ever referenced an uploaded file *by
//! name* ("content is not analyzed by this request"), so a real BRD/FRD
//! upload contributed nothing to what the AI model actually generates
//! from. This module makes that claim false in the true sense — it
//! genuinely reads the file's real text content, which the caller then
//! folds into the real generation prompt.
//!
//! `.txt`/`.md`/`.json`/`.yaml`/`.yml` are already plain text — no parsing
//! needed beyond a UTF-8 decode. `.docx` is a real zip archive containing
//! `word/document.xml`; this reads that entry directly (a `.docx` *is*
//! literally a zip, not a format that needs an external converter) and
//! pulls out the real `<w:t>` text runs, the same way `skill_files.rs`
//! parses its own format directly rather than shelling out. A text-based
//! `.pdf` is read with `pdf-extract`; a scanned, image-only PDF has no text
//! to read and says so. Legacy `.doc` (a pre-2007 OLE binary format,
//! structurally unrelated to `.docx`) is honestly reported as unsupported
//! rather than guessed at or silently skipped — no fake/partial extraction
//! is ever returned.
//!
//! Two entry points read the same bytes for two different consumers:
//! `extract_text` returns the bounded text the original single-prompt
//! generation folds into one request, and `extract_full_text` returns the
//! whole document (with a `.docx`'s headings, lists and tables marked the way
//! Markdown marks them) for the requirement parser, which splits it up
//! instead of cutting it off.

use std::io::{Cursor, Read};

/// Real extracted content, bounded so a single huge document cannot blow
/// out the AI prompt this ultimately feeds into.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ExtractedDocument {
    pub text: String,
    pub char_count: usize,
    /// True when the real document contained more than
    /// `MAX_EXTRACTED_CHARS` and was cut off — the caller must disclose
    /// this to the user rather than implying the whole document was used.
    pub truncated: bool,
}

/// The whole text of a document, for a consumer that splits it up rather
/// than pasting it into one prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct FullDocument {
    pub text: String,
    pub char_count: usize,
    /// True only when the document is larger than `MAX_FULL_TEXT_CHARS`, a
    /// safety ceiling well above any real requirements document.
    pub truncated: bool,
}

/// Generous enough for a real multi-page requirements document, small
/// enough that a handful of attached documents still fit comfortably
/// alongside the rest of a real generation prompt.
const MAX_EXTRACTED_CHARS: usize = 20_000;

/// Safety ceiling for `extract_full_text` — about 750 pages of text. Nothing
/// a person writes as requirements gets near it; it only stops a
/// pathological file from filling memory and the database.
pub const MAX_FULL_TEXT_CHARS: usize = 3_000_000;

fn extension_of(file_name: &str) -> String {
    file_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Reads `bytes` as the format `ext` names. `structured` only matters for a
/// `.docx`: headings, list items and table rows are marked (see
/// `structured_docx_text`) instead of every paragraph being flattened to a
/// bare line.
fn read_source(ext: &str, bytes: &[u8], structured: bool) -> Result<String, String> {
    match ext {
        "txt" | "md" | "markdown" | "json" | "yaml" | "yml" => String::from_utf8(bytes.to_vec())
            .map_err(|e| format!("this file is not valid UTF-8 text: {e}")),
        "docx" => extract_docx_text(bytes, structured),
        "pdf" => extract_pdf_text(bytes),
        "doc" => Err(
            "legacy .doc (pre-2007 Word binary format) is not supported for content \
             extraction — save it as .docx and re-upload to have its real content analyzed."
                .to_string(),
        ),
        other => Err(format!(
            "unsupported file extension for content extraction: .{other}"
        )),
    }
}

fn no_text_error(ext: &str) -> String {
    if ext == "pdf" {
        "this PDF has no extractable text content — it may be a scan (an image of a page); \
         scanned PDFs are not read, export it with selectable text or save it as .docx."
            .to_string()
    } else {
        "this document has no extractable text content.".to_string()
    }
}

/// Extracts real text from `bytes`, dispatching on `file_name`'s
/// extension. Returns a real, specific `Err` for anything not genuinely
/// supported — never a partially-guessed result, because that would be
/// sent to the AI model as if it were the document's real content.
pub fn extract_text(file_name: &str, bytes: &[u8]) -> Result<ExtractedDocument, String> {
    let ext = extension_of(file_name);
    let raw = read_source(&ext, bytes, false)?;

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(no_text_error(&ext));
    }

    let total_chars = trimmed.chars().count();
    let truncated = total_chars > MAX_EXTRACTED_CHARS;
    let text: String = if truncated {
        trimmed.chars().take(MAX_EXTRACTED_CHARS).collect()
    } else {
        trimmed.to_string()
    };
    let char_count = text.chars().count();

    Ok(ExtractedDocument {
        text,
        char_count,
        truncated,
    })
}

/// Like `extract_text`, but for a consumer that splits the document up
/// instead of pasting it into one prompt: nothing is cut at the
/// 20,000-character mark, and a `.docx` keeps its headings (`# Title`), list
/// items (`- item`) and table rows (`cell | cell`).
pub fn extract_full_text(file_name: &str, bytes: &[u8]) -> Result<FullDocument, String> {
    let ext = extension_of(file_name);
    let raw = read_source(&ext, bytes, true)?;

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(no_text_error(&ext));
    }

    let total_chars = trimmed.chars().count();
    let truncated = total_chars > MAX_FULL_TEXT_CHARS;
    let text: String = if truncated {
        trimmed.chars().take(MAX_FULL_TEXT_CHARS).collect()
    } else {
        trimmed.to_string()
    };
    let char_count = text.chars().count();

    Ok(FullDocument {
        text,
        char_count,
        truncated,
    })
}

/// A text-based PDF, read with `pdf-extract`. A malformed file can make that
/// parser panic; an upload must never take the request down, so a panic is
/// reported the same way an unreadable file is.
fn extract_pdf_text(bytes: &[u8]) -> Result<String, String> {
    let owned = bytes.to_vec();
    let outcome = std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&owned));
    match outcome {
        Ok(Ok(text)) => Ok(text.replace('\u{c}', "\n")),
        Ok(Err(e)) => Err(format!("this PDF could not be read: {e}")),
        Err(_) => Err(
            "this PDF could not be read: its structure is not one this reader understands."
                .to_string(),
        ),
    }
}

/// A `.docx` is a real zip archive; `word/document.xml` holds the
/// document body. Word splits a paragraph's text across many `<w:t>`
/// runs (the same fragmentation `skills` documentation elsewhere in this
/// codebase notes for `.docx` editing), so this pulls every `<w:t>` run's
/// text out in document order and joins runs within a paragraph with no
/// added whitespace (matching how Word actually renders them back
/// together), inserting a real newline at each `</w:p>` paragraph
/// boundary.
fn extract_docx_text(bytes: &[u8], structured: bool) -> Result<String, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("could not open this file as a .docx (zip) archive: {e}"))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| format!("this .docx has no word/document.xml body: {e}"))?
        .read_to_string(&mut xml)
        .map_err(|e| format!("could not read word/document.xml: {e}"))?;

    Ok(if structured {
        structured_docx_text(&xml)
    } else {
        paragraphs_from_document_xml(&xml)
    })
}

/// Matches a real `<w:t>` text run only.
///
/// The tag name must end at the `>` or at whitespace before its
/// attributes (`<w:t>` / `<w:t xml:space="preserve">`) — a naive
/// `<w:t[^>]*>` also matches every *other* `w:t...`-prefixed element Word
/// emits (`<w:tblPr>`, `<w:tblW>`, `<w:tblBorders>`, `<w:tc>`, ...), which
/// leaked raw table-properties XML straight into the extracted text of any
/// real document containing a table.
fn text_run_regex() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"<w:t(?:\s[^>]*)?>([\s\S]*?)</w:t>").unwrap())
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The text of one paragraph's `<w:t>` runs, in order, joined with no gap.
fn paragraph_text(paragraph_xml: &str) -> String {
    text_run_regex()
        .captures_iter(paragraph_xml)
        .map(|c| decode_xml_entities(c.get(1).map(|m| m.as_str()).unwrap_or("")))
        .collect::<String>()
}

/// Splits `document.xml` on paragraph boundaries first, so text runs
/// belonging to different paragraphs never get glued together, then
/// extracts and concatenates that paragraph's own `<w:t>` runs.
fn paragraphs_from_document_xml(xml: &str) -> String {
    xml.split("</w:p>")
        .map(paragraph_text)
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The heading level a Word paragraph style stands for (`Heading1` /
/// `heading 2` -> 1 / 2, `Title` -> 1, `Subtitle` -> 2), if it is one.
fn heading_level(style: &str) -> Option<usize> {
    let s = style.to_ascii_lowercase().replace(' ', "");
    match s.as_str() {
        "title" => return Some(1),
        "subtitle" => return Some(2),
        _ => {}
    }
    let n: usize = s.strip_prefix("heading")?.parse().ok()?;
    (1..=6).contains(&n).then_some(n)
}

/// Table boundaries and whole paragraphs, in document order. `<w:p[ >]` so
/// `<w:pPr>` is never taken for a paragraph, and `<w:tbl>` exactly so
/// `<w:tblPr>` is never taken for a table.
fn block_regex() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?s)(?P<tbl_open><w:tbl>)|(?P<tbl_close></w:tbl>)|(?P<tr_open><w:tr[ >])|(?P<tr_close></w:tr>)|(?P<tc_open><w:tc[ >])|(?P<tc_close></w:tc>)|(?P<p><w:p[ >].*?</w:p>)",
        )
        .unwrap()
    })
}

fn paragraph_style_regex() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"<w:pStyle\s+w:val="([^"]*)""#).unwrap())
}

/// A `.docx` body as lines that keep its structure: a heading paragraph
/// becomes `# Title` (as many `#` as its level), a list paragraph `- item`,
/// a table row `cell | cell`, anything else its bare text. Same runs, same
/// entity decoding as `paragraphs_from_document_xml`; only what surrounds
/// the text differs, so a requirement parser can tell a section title from a
/// requirement, and a row of a requirements table from a paragraph.
fn structured_docx_text(xml: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut cells: Vec<String> = Vec::new();
    let mut cell: Vec<String> = Vec::new();

    for caps in block_regex().captures_iter(xml) {
        if caps.name("tbl_open").is_some() {
            depth += 1;
        } else if caps.name("tbl_close").is_some() {
            depth = depth.saturating_sub(1);
        } else if caps.name("tr_open").is_some() {
            if depth == 1 {
                cells.clear();
            }
        } else if caps.name("tc_open").is_some() {
            if depth == 1 {
                cell.clear();
            }
        } else if caps.name("tc_close").is_some() {
            if depth == 1 {
                cells.push(cell.join(" ").trim().to_string());
                cell.clear();
            }
        } else if caps.name("tr_close").is_some() {
            if depth == 1 {
                if cells.iter().any(|c| !c.is_empty()) {
                    lines.push(cells.join(" | "));
                }
                cells.clear();
            }
        } else if let Some(p) = caps.name("p") {
            let paragraph_xml = p.as_str();
            let text = paragraph_text(paragraph_xml);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            if depth > 0 {
                cell.push(text.to_string());
                continue;
            }
            let heading = paragraph_style_regex()
                .captures(paragraph_xml)
                .and_then(|c| heading_level(c.get(1).map(|m| m.as_str()).unwrap_or("")));
            if let Some(level) = heading {
                lines.push(format!("{} {text}", "#".repeat(level)));
            } else if paragraph_xml.contains("<w:numPr>") {
                lines.push(format!("- {text}"));
            } else {
                lines.push(text.to_string());
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Builds a real, valid minimal .docx (a genuine zip archive with a
    /// real word/document.xml body) in-memory — the same structure a real
    /// Word document has, not an invented stand-in format.
    fn build_test_docx(paragraphs: &[&str]) -> Vec<u8> {
        let body: String = paragraphs
            .iter()
            .map(|p| format!("<w:p><w:r><w:t>{p}</w:t></w:r></w:p>"))
            .collect();
        docx_from_body(&body)
    }

    fn docx_from_body(body: &str) -> Vec<u8> {
        let document_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>{body}</w:body>
</w:document>"#
        );

        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zip = zip::ZipWriter::new(cursor);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("word/document.xml", options).unwrap();
            zip.write_all(document_xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    /// A tiny but genuine one-page text PDF (a real cross-reference table,
    /// Helvetica text), so PDF reading is exercised on a real file format and
    /// not on a stand-in.
    fn build_test_pdf(lines: &[&str]) -> Vec<u8> {
        let mut content = String::from("BT\n/F1 12 Tf\n14 TL\n72 720 Td\n");
        for l in lines {
            let escaped = l
                .replace('\\', "\\\\")
                .replace('(', "\\(")
                .replace(')', "\\)");
            content.push_str(&format!("({escaped}) Tj T*\n"));
        }
        content.push_str("ET\n");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, obj) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{obj}\nendobj\n", i + 1).as_bytes());
        }
        let xref_pos = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for off in offsets {
            out.extend(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    #[test]
    fn extracts_real_paragraph_text_from_a_real_docx_zip_archive() {
        let bytes = build_test_docx(&[
            "First requirement paragraph.",
            "Second requirement paragraph.",
        ]);
        let extracted = extract_text("BRD_Test.docx", &bytes).expect("real docx must extract");
        assert_eq!(
            extracted.text,
            "First requirement paragraph.\nSecond requirement paragraph."
        );
        assert!(!extracted.truncated);
        assert_eq!(extracted.char_count, extracted.text.chars().count());
    }

    #[test]
    fn runs_split_across_multiple_w_t_tags_in_the_same_paragraph_are_joined_with_no_gap() {
        // Word genuinely fragments one visible sentence across several
        // <w:t> runs (spell-check/revision boundaries) — the same real
        // fragmentation the docx-editing skill's own docs describe.
        let xml = "<w:p><w:r><w:t>Hello, </w:t></w:r><w:r><w:t>world.</w:t></w:r></w:p>";
        assert_eq!(paragraphs_from_document_xml(xml), "Hello, world.");
    }

    /// Regression: a naive `<w:t[^>]*>` pattern also matched `<w:tblPr>`,
    /// `<w:tblW>`, `<w:tblBorders>` and friends, so every real document
    /// containing a table leaked raw table-properties XML into its own
    /// extracted text. Caught against a real BRD upload, pinned here.
    #[test]
    fn table_property_elements_are_never_mistaken_for_text_runs() {
        let xml = concat!(
            "<w:p><w:r><w:t>Before table.</w:t></w:r></w:p>",
            r#"<w:tbl><w:tblPr><w:tblW w:type="dxa" w:w="9720"/>"#,
            r#"<w:tblBorders><w:top w:val="single" w:color="auto" w:sz="4"/></w:tblBorders>"#,
            "</w:tblPr>",
            "<w:tr><w:tc><w:p><w:r><w:t>Real cell text.</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
        );
        let out = paragraphs_from_document_xml(xml);
        assert!(
            !out.contains("<w:tbl"),
            "raw table XML leaked into extracted text: {out}"
        );
        assert!(
            !out.contains("dxa"),
            "raw table attributes leaked into extracted text: {out}"
        );
        assert!(out.contains("Before table."), "real text was lost: {out}");
        assert!(
            out.contains("Real cell text."),
            "real table-cell text was lost: {out}"
        );
    }

    #[test]
    fn a_text_run_with_real_attributes_is_still_extracted() {
        let xml = r#"<w:p><w:r><w:t xml:space="preserve">Kept  spacing.</w:t></w:r></w:p>"#;
        assert_eq!(paragraphs_from_document_xml(xml), "Kept  spacing.");
    }

    #[test]
    fn xml_entities_inside_real_text_are_decoded() {
        let xml = "<w:p><w:r><w:t>Rates &amp; commissions &lt;= 20%</w:t></w:r></w:p>";
        assert_eq!(
            paragraphs_from_document_xml(xml),
            "Rates & commissions <= 20%"
        );
    }

    #[test]
    fn plain_text_formats_are_read_verbatim_with_no_parsing() {
        let extracted = extract_text("notes.md", b"# Real notes\n\nJust markdown.").unwrap();
        assert_eq!(extracted.text, "# Real notes\n\nJust markdown.");
    }

    #[test]
    fn a_legacy_doc_file_is_reported_as_unsupported_not_guessed_at() {
        let err = extract_text("Requirements.doc", b"anything").unwrap_err();
        assert!(
            err.contains(".doc"),
            "expected the error to explain the .doc limitation, got: {err}"
        );
    }

    #[test]
    fn a_file_that_is_not_a_readable_pdf_is_a_real_error_never_a_guess_or_a_panic() {
        for name in ["spec.pdf", "SPEC.PDF"] {
            let err = extract_text(name, b"%PDF-1.4 this is not really a pdf").unwrap_err();
            assert!(
                err.contains("PDF"),
                "expected the error to name PDF, got: {err}"
            );
            let err = extract_full_text(name, b"").unwrap_err();
            assert!(err.contains("PDF"), "got: {err}");
        }
    }

    #[test]
    fn a_text_pdf_is_read_line_by_line() {
        let bytes = build_test_pdf(&[
            "3.2 Login",
            "The system shall lock an account after 5 failed attempts.",
            "A session token expires after 30 minutes (idle).",
        ]);
        let extracted = extract_text("FRD.pdf", &bytes).expect("a real text PDF must extract");
        assert!(
            extracted.text.contains("3.2 Login"),
            "got: {}",
            extracted.text
        );
        assert!(
            extracted
                .text
                .contains("lock an account after 5 failed attempts"),
            "got: {}",
            extracted.text
        );
        assert!(
            extracted.text.contains("30 minutes (idle)"),
            "parentheses inside a PDF string must survive: {}",
            extracted.text
        );
        let full = extract_full_text("FRD.pdf", &bytes).unwrap();
        assert!(full.text.contains("3.2 Login"));
    }

    #[test]
    fn a_pdf_page_with_no_text_says_it_may_be_a_scan() {
        let bytes = build_test_pdf(&[]);
        let err = extract_text("scan.pdf", &bytes).unwrap_err();
        assert!(err.contains("no extractable text"), "got: {err}");
        assert!(err.contains("scan"), "got: {err}");
    }

    #[test]
    fn an_unknown_extension_is_a_real_specific_error() {
        let err = extract_text("file.xyz", b"data").unwrap_err();
        assert!(
            err.contains(".xyz"),
            "expected the error to name the real extension, got: {err}"
        );
    }

    #[test]
    fn empty_extractable_content_is_a_real_error_not_a_silent_empty_success() {
        let bytes = build_test_docx(&[]);
        let err = extract_text("empty.docx", &bytes).unwrap_err();
        assert!(err.contains("no extractable text"));
    }

    #[test]
    fn a_document_longer_than_the_cap_is_truncated_and_says_so() {
        let long_paragraph = "word ".repeat(10_000); // ~50,000 chars, well over the 20,000 cap
        let bytes = build_test_docx(&[&long_paragraph]);
        let extracted = extract_text("long.docx", &bytes).unwrap();
        assert!(extracted.truncated);
        assert_eq!(extracted.char_count, MAX_EXTRACTED_CHARS);
        assert_eq!(extracted.text.chars().count(), MAX_EXTRACTED_CHARS);
    }

    #[test]
    fn the_full_text_is_never_cut_at_the_prompt_cap() {
        // Same 50,000-character document the capped reader cuts at 20,000.
        let long_paragraph = "word ".repeat(10_000);
        let bytes = build_test_docx(&[&long_paragraph]);

        let capped = extract_text("long.docx", &bytes).unwrap();
        let full = extract_full_text("long.docx", &bytes).unwrap();

        assert!(capped.truncated);
        assert!(!full.truncated);
        assert_eq!(full.text.chars().count(), full.char_count);
        assert!(
            full.char_count > 49_000,
            "the whole document must be there: {}",
            full.char_count
        );
    }

    #[test]
    fn the_full_text_of_a_plain_file_is_the_same_text() {
        let full = extract_full_text("notes.md", b"  # Real notes\n\nJust markdown.  ").unwrap();
        assert_eq!(full.text, "# Real notes\n\nJust markdown.");
        assert!(extract_full_text("notes.md", b"   ").is_err());
        assert!(extract_full_text("notes.xyz", b"x").is_err());
    }

    #[test]
    fn a_non_zip_docx_is_a_real_specific_error_not_a_panic() {
        let err = extract_text("corrupt.docx", b"not a zip file at all").unwrap_err();
        assert!(err.contains("zip"), "expected a zip-open error, got: {err}");
    }

    fn styled(style: &str, text: &str) -> String {
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    }

    #[test]
    fn a_docx_keeps_its_headings_list_items_and_table_rows_in_the_full_text() {
        let body = [
            styled("Heading1", "3. Authentication"),
            styled("Heading2", "3.2 Login"),
            "<w:p><w:r><w:t>Users sign in with an email address.</w:t></w:r></w:p>".to_string(),
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Five failed attempts lock the account.</w:t></w:r></w:p>"#.to_string(),
            concat!(
                "<w:tbl><w:tblPr><w:tblW w:w=\"9720\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1000\"/></w:tblGrid>",
                "<w:tr><w:trPr><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:tcW w:w=\"1000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>ID</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Requirement</w:t></w:r></w:p></w:tc></w:tr>",
                "<w:tr><w:trPr><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:tcW w:w=\"1000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>FR-1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Passwords are </w:t></w:r></w:p><w:p><w:r><w:t>hashed.</w:t></w:r></w:p></w:tc></w:tr>",
                "</w:tbl>"
            )
            .to_string(),
            "<w:p><w:r><w:t>After the table.</w:t></w:r></w:p>".to_string(),
        ]
        .concat();
        let bytes = docx_from_body(&body);

        let full = extract_full_text("FRD.docx", &bytes).unwrap();

        assert_eq!(
            full.text,
            [
                "# 3. Authentication",
                "## 3.2 Login",
                "Users sign in with an email address.",
                "- Five failed attempts lock the account.",
                "ID | Requirement",
                "FR-1 | Passwords are hashed.",
                "After the table.",
            ]
            .join("\n")
        );
        // The bounded reader the single-prompt path uses is exactly what it was: bare lines.
        let plain = extract_text("FRD.docx", &bytes).unwrap();
        assert!(!plain.text.contains('#'), "got: {}", plain.text);
        assert!(!plain.text.contains(" | "), "got: {}", plain.text);
    }

    #[test]
    fn heading_styles_are_recognised_by_their_level_in_any_spelling() {
        assert_eq!(heading_level("Heading1"), Some(1));
        assert_eq!(heading_level("heading 2"), Some(2));
        assert_eq!(heading_level("Heading6"), Some(6));
        assert_eq!(heading_level("Heading7"), None);
        assert_eq!(heading_level("Title"), Some(1));
        assert_eq!(heading_level("Subtitle"), Some(2));
        assert_eq!(heading_level("Normal"), None);
        assert_eq!(heading_level("HeadingChar"), None);
    }
}
