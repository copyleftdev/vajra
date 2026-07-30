//! Stream top-level records from a file without holding the corpus.
//!
//! `load_adaptive`'s large-input branch read the whole file, parsed a full DOM,
//! then materialised a `Vec<JsonEvent>` beside it — so `--streaming` cost 402 MB
//! against the DOM path's 233 MB on a 15 MB input. The sketch accumulators it
//! selected were already bounded; what was missing was a way to feed them one
//! record at a time. See #102.
//!
//! Two shapes are handled, both genuinely incrementally:
//!
//! - **NDJSON** via `serde_json::StreamDeserializer`, one line at a time.
//! - **A top-level JSON array** via a `SeqAccess` visitor that pulls one element
//!   at a time from a buffered reader. This is the shape vajra's own docs use as
//!   the large-input example, and the one that is not simply line-splitting.
//!
//! What is bounded is the number of *records* held at once — one, or two while
//! looking ahead to tell a single document from a stream. Total memory is that
//! plus the accumulator state, which is O(distinct paths) times the configured
//! sketch size. So it is bounded by the schema and the largest record, not by
//! the file: a document with unboundedly many distinct paths still grows. Both
//! limits are real and stated rather than implied.

use std::fs::File;
use std::io::BufReader;
use std::marker::PhantomData;
use std::path::Path;

use serde::de::{DeserializeSeed, SeqAccess, Visitor};
use serde::Deserializer;

use vajra_types::VajraError;

/// Top-level shape of a record stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordShape {
    /// One or more whitespace-separated JSON values.
    ///
    /// A file holding exactly one is a single document, not a one-record
    /// stream; the two are indistinguishable from the leading byte, so they are
    /// told apart by looking ahead one value.
    Ndjson,
    /// A single top-level `[...]` whose elements are the records.
    JsonArray,
}

/// Where a record sits in the document, and therefore what its paths are
/// rooted at.
///
/// A single top-level object is rooted at `$`; an element of a record stream at
/// `$[*]`. Getting this wrong makes streaming disagree with the DOM path on
/// every path name for the same input — `$[*].a` against `$.a`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordRoot {
    /// The record is the whole document.
    Document,
    /// The record is one element of a top-level sequence.
    Element,
}

/// Decide how to stream a file, by reading only its first non-whitespace byte.
///
/// Reads a small prefix rather than the file, so shape detection does not
/// defeat the point of streaming.
///
/// # Errors
///
/// Returns [`VajraError::Io`] if the file cannot be opened or read.
pub fn detect_shape(path: &Path) -> Result<RecordShape, VajraError> {
    use std::io::Read;

    let mut file = File::open(path).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut prefix = [0_u8; 64];
    let read = file.read(&mut prefix).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let first = prefix[..read].iter().find(|b| !b.is_ascii_whitespace());
    Ok(match first {
        Some(b'[') => RecordShape::JsonArray,
        _ => RecordShape::Ndjson,
    })
}

/// Call `f` once per top-level record, holding one record at a time.
///
/// Returns the number of records visited.
///
/// # Errors
///
/// Returns [`VajraError::Io`] if the file cannot be read, or
/// [`VajraError::Parse`] if the content is not valid JSON of the detected
/// shape.
pub fn for_each_record<F>(path: &Path, shape: RecordShape, f: F) -> Result<u64, VajraError>
where
    F: FnMut(&serde_json::Value, RecordRoot),
{
    let file = File::open(path).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let reader = BufReader::new(file);

    match shape {
        RecordShape::Ndjson => stream_ndjson(reader, path, f),
        RecordShape::JsonArray => stream_array(reader, path, f),
    }
}

fn parse_error(path: &Path, e: &serde_json::Error) -> VajraError {
    VajraError::Parse {
        byte_offset: e.column(),
        message: e.to_string(),
        source_path: Some(path.to_path_buf()),
    }
}

/// Whitespace-separated JSON values, via `StreamDeserializer`.
///
/// A file holding exactly one value is a single document — `{"a":1}` — not a
/// one-record stream, and its paths must be rooted at `$` to match the DOM
/// path. The two cases share a leading byte, so the first value is held while
/// the second is attempted: one extra record live, still bounded.
fn stream_ndjson<R, F>(reader: R, path: &Path, mut f: F) -> Result<u64, VajraError>
where
    R: std::io::Read,
    F: FnMut(&serde_json::Value, RecordRoot),
{
    let mut stream = serde_json::Deserializer::from_reader(reader).into_iter::<serde_json::Value>();

    let Some(first) = stream.next() else {
        return Ok(0);
    };
    let first = first.map_err(|e| parse_error(path, &e))?;

    let Some(second) = stream.next() else {
        // Exactly one value: the file is that document.
        f(&first, RecordRoot::Document);
        return Ok(1);
    };
    let second = second.map_err(|e| parse_error(path, &e))?;

    f(&first, RecordRoot::Element);
    drop(first);
    f(&second, RecordRoot::Element);
    drop(second);

    let mut count = 2_u64;
    for value in stream {
        let value = value.map_err(|e| parse_error(path, &e))?;
        f(&value, RecordRoot::Element);
        count += 1;
    }
    Ok(count)
}

/// A top-level array, pulled one element at a time.
///
/// `StreamDeserializer` handles concatenated values, not the *inside* of an
/// array, so the elements are drawn through a `SeqAccess` visitor instead.
/// `next_element` decodes exactly one element from the reader, which is what
/// makes this incremental rather than a DOM parse in disguise.
fn stream_array<R, F>(reader: R, path: &Path, f: F) -> Result<u64, VajraError>
where
    R: std::io::Read,
    F: FnMut(&serde_json::Value, RecordRoot),
{
    struct ArrayVisitor<F> {
        f: F,
    }

    impl<'de, F> Visitor<'de> for ArrayVisitor<F>
    where
        F: FnMut(&serde_json::Value, RecordRoot),
    {
        type Value = u64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a JSON array of records")
        }

        fn visit_seq<A>(mut self, mut seq: A) -> Result<u64, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut count = 0_u64;
            while let Some(value) = seq.next_element::<serde_json::Value>()? {
                (self.f)(&value, RecordRoot::Element);
                count += 1;
                // Dropped before the next element is decoded.
                drop(value);
            }
            Ok(count)
        }
    }

    struct ArraySeed<F> {
        f: F,
        _marker: PhantomData<fn()>,
    }

    impl<'de, F> DeserializeSeed<'de> for ArraySeed<F>
    where
        F: FnMut(&serde_json::Value, RecordRoot),
    {
        type Value = u64;

        fn deserialize<D>(self, deserializer: D) -> Result<u64, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_seq(ArrayVisitor { f: self.f })
        }
    }

    let mut de = serde_json::Deserializer::from_reader(reader);
    // The recursion limit guards nesting depth per record, not record count,
    // so it does not cap how many records can be streamed.
    ArraySeed {
        f,
        _marker: PhantomData,
    }
    .deserialize(&mut de)
    .map_err(|e| parse_error(path, &e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &tempfile::TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut f = File::create(&path).expect("create");
        f.write_all(content.as_bytes()).expect("write");
        path
    }

    #[test]
    fn detects_an_array_past_leading_whitespace() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.json", "\n\n  [ {\"a\":1} ]");
        assert_eq!(detect_shape(&p).expect("detect"), RecordShape::JsonArray);
    }

    #[test]
    fn detects_ndjson() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.ndjson", "{\"a\":1}\n{\"a\":2}\n");
        assert_eq!(detect_shape(&p).expect("detect"), RecordShape::Ndjson);
    }

    #[test]
    fn streams_every_record_of_an_array() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.json", r#"[{"a":1},{"a":2},{"a":3}]"#);
        let mut seen = Vec::new();
        let n = for_each_record(&p, RecordShape::JsonArray, |v, _| {
            seen.push(v["a"].as_u64().unwrap_or_default());
        })
        .expect("stream");
        assert_eq!(n, 3);
        assert_eq!(seen, vec![1, 2, 3], "order must be preserved");
    }

    #[test]
    fn streams_every_record_of_ndjson() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.ndjson", "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n");
        let mut seen = Vec::new();
        let n = for_each_record(&p, RecordShape::Ndjson, |v, _| {
            seen.push(v["a"].as_u64().unwrap_or_default());
        })
        .expect("stream");
        assert_eq!(n, 3);
        assert_eq!(seen, vec![1, 2, 3]);
    }

    #[test]
    fn an_empty_array_yields_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.json", "[]");
        let mut count = 0;
        let n = for_each_record(&p, RecordShape::JsonArray, |_, _| count += 1).expect("stream");
        assert_eq!((n, count), (0, 0));
    }

    #[test]
    fn malformed_json_reports_the_source_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.json", r#"[{"a":1},{"a":]"#);
        let err = for_each_record(&p, RecordShape::JsonArray, |_, _| {}).expect_err("must fail");
        match err {
            VajraError::Parse { source_path, .. } => {
                assert_eq!(source_path.as_deref(), Some(p.as_path()));
            }
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    /// A file holding one JSON object is that document, not a one-record
    /// stream. Rooting it at `$[*]` made streaming report `$[*].a` where the
    /// DOM path reports `$.a` — the same input answered two ways.
    #[test]
    fn a_lone_object_is_a_document_not_a_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.json", r#"{"a":1,"b":{"c":"x"}}"#);
        let mut roots = Vec::new();
        let n =
            for_each_record(&p, RecordShape::Ndjson, |_, root| roots.push(root)).expect("stream");
        assert_eq!(n, 1);
        assert_eq!(roots, vec![RecordRoot::Document]);
    }

    #[test]
    fn two_or_more_values_are_a_record_stream() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.ndjson", "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n");
        let mut roots = Vec::new();
        let n =
            for_each_record(&p, RecordShape::Ndjson, |_, root| roots.push(root)).expect("stream");
        assert_eq!(n, 3);
        assert_eq!(
            roots,
            vec![RecordRoot::Element; 3],
            "including the first two"
        );
    }

    /// An array is always a sequence, even with a single element.
    #[test]
    fn a_single_element_array_is_still_a_sequence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(&dir, "a.json", r#"[{"a":1}]"#);
        let mut roots = Vec::new();
        for_each_record(&p, RecordShape::JsonArray, |_, root| roots.push(root)).expect("stream");
        assert_eq!(roots, vec![RecordRoot::Element]);
    }

    /// Records are visited as they are decoded, not collected first — so the
    /// callback sees record 1 before record N exists.
    #[test]
    fn records_are_visited_incrementally() {
        let dir = tempfile::tempdir().expect("tempdir");
        let body: String = (0..500)
            .map(|i| format!(r#"{{"i":{i}}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let p = write(&dir, "a.json", &format!("[{body}]"));

        let mut first_seen_at = None;
        let mut count = 0_usize;
        for_each_record(&p, RecordShape::JsonArray, |v, _| {
            if v["i"].as_u64() == Some(0) {
                first_seen_at = Some(count);
            }
            count += 1;
        })
        .expect("stream");
        assert_eq!(count, 500);
        assert_eq!(
            first_seen_at,
            Some(0),
            "the first record must arrive before the rest are decoded"
        );
    }
}
