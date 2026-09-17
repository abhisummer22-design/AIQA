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
//! parses its own format directly rather than shelling out. Legacy `.doc`
//! (a pre-2007 OLE binary format, structurally unrelated to `.docx`) and
//! `.pdf` are honestly reported as unsupported rather than guessed at or
//! silently skipped — no fake/partial extraction is ever returned.

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

/// Generous enough for a real multi-page requirements document, small
/// enough that a handful of attached documents still fit comfortably
/// alongside the rest of a real generation prompt.
const MAX_EXTRACTED_CHARS: usize = 20_000;

/// Extracts real text from `bytes`, dispatching on `file_name`'s
/// extension. Returns a real, specific `Err` for anything not genuinely
/// supported — never a partially-guessed result, because that would be
/// sent to the AI model as if it were the document's real content.
pub fn extract_text(file_name: &str, bytes: &[u8]) -> Result<ExtractedDocument, String> {
    let ext = file_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    let raw = match ext.as_str() {
        "txt" | "md" | "markdown" | "json" | "yaml" | "yml" => String::from_utf8(bytes.to_vec())
            .map_err(|e| format!("this file is not valid UTF-8 text: {e}"))?,
        "docx" => extract_docx_text(bytes)?,
        "doc" => {
            return Err(
                "legacy .doc (pre-2007 Word binary format) is not supported for content \
                 extraction — save it as .docx and re-upload to have its real content analyzed."
                    .to_string(),
            )
        }
        "pdf" => {
            return Err(
                "PDF content extraction is not yet supported for AI generation.".to_string(),
            )
        }
        other => {
            return Err(format!(
                "unsupported file extension for content extraction: .{other}"
            ))
        }
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("this document has no extractable text content.".to_string());
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

/// A `.docx` is a real zip archive; `word/document.xml` holds the
/// document body. Word splits a paragraph's text across many `<w:t>`
/// runs (the same fragmentation `skills` documentation elsewhere in this
/// codebase notes for `.docx` editing), so this pulls every `<w:t>` run's
/// text out in document order and joins runs within a paragraph with no
/// added whitespace (matching how Word actually renders them back
/// together), inserting a real newline at each `</w:p>` paragraph
/// boundary.
fn extract_docx_text(bytes: &[u8]) -> Result<String, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("could not open this file as a .docx (zip) archive: {e}"))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| format!("this .docx has no word/document.xml body: {e}"))?
        .read_to_string(&mut xml)
        .map_err(|e| format!("could not read word/document.xml: {e}"))?;

    Ok(paragraphs_from_document_xml(&xml))
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

/// Splits `document.xml` on paragraph boundaries first, so text runs
/// belonging to different paragraphs never get glued together, then
/// extracts and concatenates that paragraph's own `<w:t>` runs.
fn paragraphs_from_document_xml(xml: &str) -> String {
    let re = text_run_regex();
    xml.split("</w:p>")
        .map(|para_xml| {
            re.captures_iter(para_xml)
                .map(|c| decode_xml_entities(c.get(1).map(|m| m.as_str()).unwrap_or("")))
                .collect::<String>()
        })
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
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
    fn a_pdf_is_reported_as_unsupported_not_guessed_at() {
        let err = extract_text("spec.pdf", b"%PDF-1.4").unwrap_err();
        assert!(
            err.contains("PDF"),
            "expected the error to name PDF, got: {err}"
        );
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
    fn a_non_zip_docx_is_a_real_specific_error_not_a_panic() {
        let err = extract_text("corrupt.docx", b"not a zip file at all").unwrap_err();
        assert!(err.contains("zip"), "expected a zip-open error, got: {err}");
    }
}
