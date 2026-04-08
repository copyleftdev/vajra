//! PDF → structured JSON → [`Document`] adapter.
//!
//! Extracts text from PDF files on a per-page basis, builds a structured
//! JSON representation with metadata (page count, word counts, paragraphs),
//! and feeds the JSON through the standard parse pipeline.
//!
//! Requires the `pdf` cargo feature (enabled by default).

use std::path::Path;

use serde_json::json;
use vajra_types::{Document, VajraError};

use crate::parse::parse_str;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a PDF file at the given path and extract text into a structured
/// JSON [`Document`].
///
/// The resulting JSON has the shape:
///
/// ```json
/// {
///   "type": "pdf_document",
///   "metadata": { "page_count": 10, "word_count": 5000 },
///   "pages": [ { "number": 1, "text": "…", "word_count": 450, "paragraphs": ["…", "…"] } ]
/// }
/// ```
///
/// # Errors
///
/// Returns [`VajraError::Parse`] if the file cannot be read or is not a
/// valid PDF.
#[cfg(feature = "pdf")]
pub fn parse_pdf_file(path: &Path) -> Result<Document, VajraError> {
    let pages = pdf_extract::extract_text_by_pages(path).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("PDF extraction failed: {e}"),
        source_path: Some(path.to_path_buf()),
    })?;

    build_document_from_pages(&pages)
}

/// Parse PDF content from an in-memory byte slice.
///
/// # Errors
///
/// Returns [`VajraError::Parse`] if the data is not a valid PDF.
#[cfg(feature = "pdf")]
pub fn parse_pdf_bytes(data: &[u8]) -> Result<Document, VajraError> {
    let pages =
        pdf_extract::extract_text_from_mem_by_pages(data).map_err(|e| VajraError::Parse {
            byte_offset: 0,
            message: format!("PDF extraction from memory failed: {e}"),
            source_path: None,
        })?;

    build_document_from_pages(&pages)
}

/// Stub for when the `pdf` feature is disabled.
///
/// # Errors
///
/// Always returns an error indicating the feature is missing.
#[cfg(not(feature = "pdf"))]
pub fn parse_pdf_file(_path: &Path) -> Result<Document, VajraError> {
    Err(VajraError::Config {
        message: "PDF parsing requires the 'pdf' feature. \
                  Install vajra with: cargo install vajra --features pdf"
            .to_string(),
    })
}

/// Stub for when the `pdf` feature is disabled.
///
/// # Errors
///
/// Always returns an error indicating the feature is missing.
#[cfg(not(feature = "pdf"))]
pub fn parse_pdf_bytes(_data: &[u8]) -> Result<Document, VajraError> {
    Err(VajraError::Config {
        message: "PDF parsing requires the 'pdf' feature. \
                  Install vajra with: cargo install vajra --features pdf"
            .to_string(),
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Split text into paragraphs by double-newline boundaries.
fn split_paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn count_words(text: &str) -> u64 {
    text.split_whitespace().count() as u64
}

/// Build a [`Document`] from a vec of per-page text strings.
fn build_document_from_pages(pages: &[String]) -> Result<Document, VajraError> {
    let page_count = pages.len();
    let mut total_word_count: u64 = 0;
    let mut page_values = Vec::with_capacity(page_count);

    for (idx, page_text) in pages.iter().enumerate() {
        let wc = count_words(page_text);
        total_word_count += wc;
        let paragraphs = split_paragraphs(page_text);

        page_values.push(json!({
            "number": idx + 1,
            "text": page_text,
            "word_count": wc,
            "paragraphs": paragraphs,
        }));
    }

    let doc_json = json!({
        "type": "pdf_document",
        "metadata": {
            "page_count": page_count,
            "word_count": total_word_count,
        },
        "pages": page_values,
    });

    let json_text = serde_json::to_string(&doc_json).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("PDF->JSON serialization: {e}"),
        source_path: None,
    })?;

    parse_str(&json_text)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Error handling tests (work regardless of pdf feature) ----

    #[test]
    fn pdf_parse_error_on_non_pdf_data() {
        let result = parse_pdf_bytes(b"this is not a PDF file at all");
        assert!(result.is_err(), "expected error for non-PDF data");
        // Should not panic.
    }

    #[test]
    fn pdf_parse_error_on_empty_data() {
        let result = parse_pdf_bytes(b"");
        assert!(result.is_err(), "expected error for empty data");
    }

    #[test]
    fn pdf_parse_file_nonexistent() {
        let result = parse_pdf_file(Path::new("/nonexistent/file.pdf"));
        assert!(result.is_err(), "expected error for nonexistent file");
    }

    // ---- Internal helper tests ----

    #[test]
    fn split_paragraphs_basic() {
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird.";
        let paras = split_paragraphs(text);
        assert_eq!(paras.len(), 3);
        assert_eq!(paras[0], "First paragraph.");
        assert_eq!(paras[1], "Second paragraph.");
        assert_eq!(paras[2], "Third.");
    }

    #[test]
    fn split_paragraphs_empty() {
        let paras = split_paragraphs("");
        assert!(paras.is_empty());
    }

    #[test]
    fn split_paragraphs_single() {
        let text = "Just one paragraph.";
        let paras = split_paragraphs(text);
        assert_eq!(paras.len(), 1);
    }

    #[test]
    fn count_words_basic() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("one"), 1);
    }

    #[test]
    fn build_document_from_empty_pages() -> Result<(), Box<dyn std::error::Error>> {
        let pages: Vec<String> = vec![];
        let doc = build_document_from_pages(&pages)?;
        let v = doc.value();
        assert_eq!(v["type"], "pdf_document");
        assert_eq!(v["metadata"]["page_count"], 0);
        assert_eq!(v["metadata"]["word_count"], 0);
        let pages_arr = v["pages"].as_array().ok_or("no pages")?;
        assert!(pages_arr.is_empty());
        Ok(())
    }

    #[test]
    fn build_document_from_multiple_pages() -> Result<(), Box<dyn std::error::Error>> {
        let pages = vec![
            "Hello world.\n\nSecond paragraph.".to_string(),
            "Page two content here with more words.".to_string(),
        ];
        let doc = build_document_from_pages(&pages)?;
        let v = doc.value();
        assert_eq!(v["metadata"]["page_count"], 2);
        let total_wc = v["metadata"]["word_count"].as_u64().unwrap_or(0);
        assert!(
            total_wc > 5,
            "expected more than 5 words total, got {total_wc}"
        );

        let pages_arr = v["pages"].as_array().ok_or("no pages")?;
        assert_eq!(pages_arr[0]["number"], 1);
        assert_eq!(pages_arr[1]["number"], 2);

        let paras = pages_arr[0]["paragraphs"]
            .as_array()
            .ok_or("no paragraphs")?;
        assert_eq!(paras.len(), 2);
        Ok(())
    }

    #[test]
    fn build_document_produces_valid_trie() -> Result<(), Box<dyn std::error::Error>> {
        let pages = vec!["Some test content on page one.".to_string()];
        let doc = build_document_from_pages(&pages)?;
        assert!(doc.metadata().total_nodes > 0);
        assert!(doc.metadata().distinct_paths > 0);
        assert!(doc.trie().path_count() > 0);
        Ok(())
    }
}
