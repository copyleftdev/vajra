//! Structural diff between two directory trees.
//!
//! `drift` compares two documents. The question this answers is different:
//! *what structurally changed between these two releases of the same thing?*
//!
//! That distinction matters for supply-chain work. Every signal that reads a
//! package's presentation — does it declare a repository, does it have a
//! description, does it look mature — is blind to a compromised *established*
//! package, because a hijacked real package presents perfectly. What
//! distinguishes it is not its state but its **change**: version N looked one
//! way, version N+1 grew a payload.
//!
//! Comparison is by relative path and structural shape, not text, so
//! reformatting and identifier renaming do not register as changes while an
//! added branch or a new call does.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::Serialize;
use vajra_types::Document;

/// How one file differs between the two trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    /// Present only in the candidate.
    Added,
    /// Present only in the baseline.
    Removed,
    /// Present in both, different structural shape.
    Changed,
    /// Present in both, identical shape.
    Unchanged,
}

impl FileChange {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "changed",
            Self::Unchanged => "unchanged",
        }
    }
}

/// One file's structural comparison.
#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    /// Path relative to the tree root, so the two sides are comparable.
    pub path: String,
    /// What happened to this file.
    pub change: FileChange,
    /// Nodes in the baseline's parse, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_nodes: Option<u64>,
    /// Nodes in the candidate's parse, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_nodes: Option<u64>,
    /// candidate_nodes - baseline_nodes, when both parsed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_delta: Option<i64>,
}

/// Result of comparing two trees.
#[derive(Debug, Serialize)]
pub struct TreeDiff {
    /// Files analysed on the baseline side.
    pub baseline_files: usize,
    /// Files analysed on the candidate side.
    pub candidate_files: usize,
    /// Counts per change kind.
    pub summary: BTreeMap<String, usize>,
    /// Net change in total nodes across all parsed files.
    pub total_node_delta: i64,
    /// Per-file differences, unchanged files omitted, ordered added/removed/
    /// changed then by path.
    pub files: Vec<FileDiff>,
    /// Files that could not be parsed on either side.
    pub errors: Vec<TreeDiffError>,
}

/// A file that could not be read or parsed.
#[derive(Debug, Clone, Serialize)]
pub struct TreeDiffError {
    /// Relative path.
    pub path: String,
    /// Which side, and why.
    pub error: String,
}

/// Collect analysable files under `root`, keyed by path relative to it.
fn relative_index(
    root: &Path,
    accept: &dyn Fn(&Path) -> bool,
) -> Result<BTreeMap<String, PathBuf>> {
    if !root.is_dir() {
        anyhow::bail!("{} is not a directory", root.display());
    }
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        let mut dirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                dirs.push(path);
            } else if kind.is_file() && accept(&path) {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.insert(rel.display().to_string(), path);
                }
            }
        }
        dirs.sort();
        dirs.reverse();
        stack.extend(dirs);
    }
    Ok(out)
}

/// Compare two trees by relative path and structural shape.
///
/// `load` parses a file and `shape_of` reduces it to a shape hash plus node
/// count, so the caller controls format resolution. Files are matched on their
/// path relative to each root — an npm tarball nests under `package/`, so
/// comparing absolute paths would report every file as both added and removed.
///
/// # Errors
///
/// Returns an error if either root is not a directory.
pub fn diff_trees(
    baseline_root: &Path,
    candidate_root: &Path,
    accept: &dyn Fn(&Path) -> bool,
    load: &(dyn Fn(&Path) -> Result<Document> + Send + Sync),
    shape_of: &(dyn Fn(&Document) -> Result<String> + Send + Sync),
) -> Result<TreeDiff> {
    let baseline = relative_index(baseline_root, accept)
        .with_context(|| format!("baseline {}", baseline_root.display()))?;
    let candidate = relative_index(candidate_root, accept)
        .with_context(|| format!("candidate {}", candidate_root.display()))?;

    let paths: BTreeSet<&String> = baseline.keys().chain(candidate.keys()).collect();
    let ordered: Vec<&String> = paths.into_iter().collect();

    // (path, change, baseline_nodes, candidate_nodes, errors)
    type Row = (
        String,
        FileChange,
        Option<u64>,
        Option<u64>,
        Vec<TreeDiffError>,
    );

    let rows: Vec<Row> = ordered
        .par_iter()
        .map(|rel| {
            let mut errors = Vec::new();
            let mut measure = |side: &str, path: Option<&PathBuf>| -> Option<(String, u64)> {
                let path = path?;
                match load(path).and_then(|doc| {
                    let nodes = doc.metadata().total_nodes;
                    shape_of(&doc).map(|shape| (shape, nodes))
                }) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        errors.push(TreeDiffError {
                            path: (*rel).clone(),
                            error: format!("{side}: {e:#}"),
                        });
                        None
                    }
                }
            };

            let base = measure("baseline", baseline.get(*rel));
            let cand = measure("candidate", candidate.get(*rel));

            let change = match (baseline.contains_key(*rel), candidate.contains_key(*rel)) {
                (false, true) => FileChange::Added,
                (true, false) => FileChange::Removed,
                _ => match (&base, &cand) {
                    (Some((a, _)), Some((b, _))) if a == b => FileChange::Unchanged,
                    // A file that failed to parse on one side is reported as
                    // changed rather than silently unchanged.
                    _ => FileChange::Changed,
                },
            };

            (
                (*rel).clone(),
                change,
                base.map(|(_, n)| n),
                cand.map(|(_, n)| n),
                errors,
            )
        })
        .collect();

    let mut summary: BTreeMap<String, usize> = BTreeMap::new();
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut total_node_delta = 0i64;

    for (path, change, baseline_nodes, candidate_nodes, errs) in rows {
        *summary.entry(change.as_str().to_owned()).or_insert(0) += 1;
        errors.extend(errs);

        let node_delta = match (baseline_nodes, candidate_nodes) {
            (Some(a), Some(b)) => {
                let d = i64::try_from(b).unwrap_or(i64::MAX) - i64::try_from(a).unwrap_or(i64::MAX);
                Some(d)
            }
            (None, Some(b)) => Some(i64::try_from(b).unwrap_or(i64::MAX)),
            (Some(a), None) => Some(-i64::try_from(a).unwrap_or(i64::MAX)),
            (None, None) => None,
        };
        if let Some(d) = node_delta {
            total_node_delta += d;
        }

        if change != FileChange::Unchanged {
            files.push(FileDiff {
                path,
                change,
                baseline_nodes,
                candidate_nodes,
                node_delta,
            });
        }
    }

    // Added, removed, then changed; path within each. Fully specified.
    files.sort_by(|a, b| a.change.cmp(&b.change).then_with(|| a.path.cmp(&b.path)));
    errors.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.error.cmp(&b.error)));

    for kind in ["added", "removed", "changed", "unchanged"] {
        summary.entry(kind.to_owned()).or_insert(0);
    }

    Ok(TreeDiff {
        baseline_files: baseline.len(),
        candidate_files: candidate.len(),
        summary,
        total_node_delta,
        files,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_only(p: &Path) -> bool {
        p.extension().is_some_and(|e| e == "json")
    }

    fn load(p: &Path) -> Result<Document> {
        Ok(vajra_core::parse_file(p)?)
    }

    fn shape_of(doc: &Document) -> Result<String> {
        // Path set is enough for tests and is stable.
        let mut paths: Vec<String> = doc.trie().all_paths().iter().map(|p| p.as_str()).collect();
        paths.sort();
        Ok(paths.join("|"))
    }

    fn write(root: &Path, rel: &str, body: &str) -> Result<()> {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, body)?;
        Ok(())
    }

    #[test]
    fn identical_trees_report_no_changes() -> Result<(), Box<dyn std::error::Error>> {
        let a = tempfile::tempdir()?;
        let b = tempfile::tempdir()?;
        for root in [a.path(), b.path()] {
            write(root, "pkg/one.json", r#"{"a":1,"b":[2,3]}"#)?;
            write(root, "pkg/two.json", r#"{"c":{"d":4}}"#)?;
        }
        let d = diff_trees(a.path(), b.path(), &json_only, &load, &shape_of)?;
        assert!(d.files.is_empty(), "no per-file entries for unchanged");
        assert_eq!(d.summary.get("unchanged"), Some(&2));
        assert_eq!(d.total_node_delta, 0);
        Ok(())
    }

    /// Matching is by path *relative to each root*, so differently-named parent
    /// directories still line up. Without this every file would be reported as
    /// both added and removed.
    #[test]
    fn matches_on_relative_path() -> Result<(), Box<dyn std::error::Error>> {
        let a = tempfile::tempdir()?;
        let b = tempfile::tempdir()?;
        write(a.path(), "pkg/index.json", r#"{"a":1}"#)?;
        write(b.path(), "pkg/index.json", r#"{"a":1}"#)?;
        let d = diff_trees(a.path(), b.path(), &json_only, &load, &shape_of)?;
        assert_eq!(d.summary.get("unchanged"), Some(&1));
        assert_eq!(d.summary.get("added"), Some(&0));
        Ok(())
    }

    #[test]
    fn detects_added_removed_and_changed() -> Result<(), Box<dyn std::error::Error>> {
        let a = tempfile::tempdir()?;
        let b = tempfile::tempdir()?;
        write(a.path(), "same.json", r#"{"a":1}"#)?;
        write(a.path(), "gone.json", r#"{"x":1}"#)?;
        write(a.path(), "edited.json", r#"{"a":1}"#)?;
        write(b.path(), "same.json", r#"{"a":1}"#)?;
        write(
            b.path(),
            "edited.json",
            r#"{"a":1,"injected":{"deep":true}}"#,
        )?;
        write(b.path(), "new.json", r#"{"y":2}"#)?;

        let d = diff_trees(a.path(), b.path(), &json_only, &load, &shape_of)?;
        assert_eq!(d.summary.get("added"), Some(&1));
        assert_eq!(d.summary.get("removed"), Some(&1));
        assert_eq!(d.summary.get("changed"), Some(&1));
        assert_eq!(d.summary.get("unchanged"), Some(&1));

        // Added first, then removed, then changed.
        let kinds: Vec<FileChange> = d.files.iter().map(|f| f.change).collect();
        assert_eq!(
            kinds,
            vec![FileChange::Added, FileChange::Removed, FileChange::Changed]
        );

        let edited = d
            .files
            .iter()
            .find(|f| f.path == "edited.json")
            .ok_or("missing edited")?;
        assert!(
            edited.node_delta.is_some_and(|d| d > 0),
            "growth should be positive, got {:?}",
            edited.node_delta
        );
        Ok(())
    }

    /// Structural comparison must ignore changes that do not alter shape.
    #[test]
    fn value_only_edits_are_unchanged() -> Result<(), Box<dyn std::error::Error>> {
        let a = tempfile::tempdir()?;
        let b = tempfile::tempdir()?;
        write(a.path(), "x.json", r#"{"host":"alpha","port":1}"#)?;
        write(b.path(), "x.json", r#"{"host":"beta","port":2}"#)?;
        let d = diff_trees(a.path(), b.path(), &json_only, &load, &shape_of)?;
        assert_eq!(
            d.summary.get("unchanged"),
            Some(&1),
            "same shape, different values"
        );
        Ok(())
    }

    #[test]
    fn added_and_removed_contribute_node_delta() -> Result<(), Box<dyn std::error::Error>> {
        let a = tempfile::tempdir()?;
        let b = tempfile::tempdir()?;
        write(a.path(), "keep.json", r#"{"a":1}"#)?;
        write(b.path(), "keep.json", r#"{"a":1}"#)?;
        write(b.path(), "extra.json", r#"{"a":1,"b":2,"c":3}"#)?;
        let d = diff_trees(a.path(), b.path(), &json_only, &load, &shape_of)?;
        assert!(
            d.total_node_delta > 0,
            "an added file grows the tree, got {}",
            d.total_node_delta
        );
        Ok(())
    }

    #[test]
    fn unparseable_file_is_reported_and_counted_changed() -> Result<(), Box<dyn std::error::Error>>
    {
        let a = tempfile::tempdir()?;
        let b = tempfile::tempdir()?;
        write(a.path(), "x.json", r#"{"a":1}"#)?;
        write(b.path(), "x.json", "{not json{{{")?;
        let d = diff_trees(a.path(), b.path(), &json_only, &load, &shape_of)?;
        assert_eq!(d.summary.get("changed"), Some(&1));
        assert_eq!(d.errors.len(), 1, "the failure is reported");
        assert!(d.errors[0].error.contains("candidate"));
        Ok(())
    }

    #[test]
    fn non_directory_input_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let a = tempfile::tempdir()?;
        write(a.path(), "f.json", "{}")?;
        let file = a.path().join("f.json");
        assert!(diff_trees(&file, a.path(), &json_only, &load, &shape_of).is_err());
        assert!(diff_trees(a.path(), &file, &json_only, &load, &shape_of).is_err());
        Ok(())
    }

    #[test]
    fn is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let a = tempfile::tempdir()?;
        let b = tempfile::tempdir()?;
        for i in 0..8 {
            write(a.path(), &format!("d/f{i}.json"), r#"{"a":1}"#)?;
            write(b.path(), &format!("d/f{i}.json"), r#"{"a":1,"b":2}"#)?;
        }
        let one = diff_trees(a.path(), b.path(), &json_only, &load, &shape_of)?;
        let two = diff_trees(a.path(), b.path(), &json_only, &load, &shape_of)?;
        let paths =
            |d: &TreeDiff| -> Vec<String> { d.files.iter().map(|f| f.path.clone()).collect() };
        assert_eq!(paths(&one), paths(&two));
        assert_eq!(one.total_node_delta, two.total_node_delta);
        Ok(())
    }
}
