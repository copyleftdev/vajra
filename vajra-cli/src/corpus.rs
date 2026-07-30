//! Corpus-wide structural fingerprint indexing.
//!
//! `fingerprint` answers "what is this document's shape?". The question that
//! comes up in practice is "**who else** has this shape?" — reuse of a
//! structural hash across otherwise unrelated documents is the signal, and
//! answering it needs an inverted index over a whole tree rather than N
//! independent invocations.
//!
//! Two views are produced from one pass:
//!
//! - **reuse groups**: shape -> the documents carrying it, for shapes seen in
//!   more than one document.
//! - **clusters**: documents linked transitively through *any* shared shape,
//!   because related documents typically share several files rather than one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::Serialize;
use vajra_types::Document;

/// One document's contribution to the index.
struct Indexed {
    path: String,
    /// The unit that clustering treats as "one thing" — see [`group_key`].
    group: String,
    shape: String,
    node_count: u64,
}

/// Derive the clustering unit for a file.
///
/// Clustering a *file* against other files is a no-op: each file has exactly one
/// shape, so no file can ever link two shapes together. The transitive view only
/// means something at a coarser unit — a package, a checkout, a submission —
/// which typically contains several files.
///
/// `depth` is how many path components below `root` identify that unit. With a
/// corpus laid out as `root/<package>/…`, depth 1 groups by package. Depth 0
/// falls back to the file itself, which disables transitive clustering.
fn group_key(root: &Path, path: &Path, depth: usize) -> String {
    if depth == 0 {
        return path.display().to_string();
    }
    let Ok(rel) = path.strip_prefix(root) else {
        return path.display().to_string();
    };
    let mut taken = PathBuf::new();
    for component in rel.components().take(depth) {
        taken.push(component);
    }
    if taken.as_os_str().is_empty() {
        path.display().to_string()
    } else {
        root.join(taken).display().to_string()
    }
}

/// A shape carried by more than one document.
#[derive(Serialize)]
pub struct ReuseGroup {
    /// The shared structural hash.
    pub shape: String,
    /// How many documents carry it.
    pub count: usize,
    /// Nodes in the hashed tree — low values are weak evidence, see `--min-nodes`.
    pub node_count: u64,
    /// The documents, sorted.
    pub members: Vec<String>,
}

/// Groups linked transitively through shared shapes.
#[derive(Serialize)]
pub struct Cluster {
    /// Number of groups in the cluster.
    pub size: usize,
    /// How many distinct shared shapes link them. More is stronger evidence.
    pub shared_shapes: usize,
    /// Smallest hashed tree among the shared shapes — the weakest link.
    pub min_node_count: u64,
    /// The groups, sorted.
    pub members: Vec<String>,
}

/// Result of indexing a corpus.
#[derive(Serialize)]
pub struct CorpusIndex {
    /// Files walked, before selection.
    pub files_scanned: usize,
    /// Files whose shape entered the index.
    pub documents_indexed: usize,
    /// Distinct clustering units among those files (see `--corpus-group-depth`).
    pub groups_indexed: usize,
    /// Files the input-format selector rejected.
    pub skipped: usize,
    /// Files withheld by the `--min-nodes` floor.
    pub suppressed: usize,
    /// Distinct shapes across the indexed documents.
    pub distinct_shapes: usize,
    /// How many of those appear in more than one document.
    pub shapes_in_multiple_documents: usize,
    /// How many appear in more than one *group* — the ones that link anything.
    pub shapes_in_multiple_groups: usize,
    /// Shape -> documents, for reused shapes only, sorted by count descending.
    pub reuse_groups: Vec<ReuseGroup>,
    /// Transitive groupings, sorted by size descending.
    pub clusters: Vec<Cluster>,
    /// Files that failed to parse, with messages.
    pub errors: Vec<CorpusError>,
}

/// A per-file failure, reported rather than dropped.
#[derive(Serialize)]
pub struct CorpusError {
    /// The file that failed.
    pub file: String,
    /// Why.
    pub error: String,
}

/// Recursively collect files under `dir`, partitioned by `accept`.
///
/// Unlike `batch` and `cluster`, this walk **recurses**: a corpus is normally a
/// tree of extracted packages or checkouts, so the interesting files are nested.
///
/// # Errors
///
/// Returns an error if `dir` is not a directory or cannot be read.
pub fn collect_corpus_files(
    dir: &Path,
    accept: &dyn Fn(&Path) -> bool,
) -> Result<(Vec<PathBuf>, usize)> {
    if !dir.is_dir() {
        anyhow::bail!("{} is not a directory", dir.display());
    }
    let mut selected = Vec::new();
    let mut scanned = 0usize;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .with_context(|| format!("failed to read directory {}", current.display()))?;
        let mut dirs = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            // Do not follow symlinks: a cycle would hang the walk.
            let Ok(meta) = entry.file_type() else {
                continue;
            };
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                dirs.push(path);
            } else if meta.is_file() {
                scanned += 1;
                if accept(&path) {
                    selected.push(path);
                }
            }
        }
        // Deterministic descent order.
        dirs.sort();
        dirs.reverse();
        stack.extend(dirs);
    }

    selected.sort();
    Ok((selected, scanned))
}

/// Build the shape-reuse index over `files`.
///
/// `load` parses each file and `shape_of` extracts its shape hash and node
/// count, so the caller controls format resolution. Documents whose node count
/// is below `min_nodes` are counted as suppressed and excluded — indexing a
/// hash that collides across unrelated trivial documents is worse than not
/// indexing it.
pub fn build_index(
    root: &Path,
    files: &[PathBuf],
    scanned: usize,
    min_nodes: u64,
    group_depth: usize,
    load: &(dyn Fn(&Path) -> Result<Document> + Send + Sync),
    shape_of: &(dyn Fn(&Document) -> Result<String> + Send + Sync),
) -> CorpusIndex {
    let outcomes: Vec<(PathBuf, Result<Option<Indexed>>)> = files
        .par_iter()
        .map(|path| {
            let outcome = load(path)
                .with_context(|| format!("failed to parse {}", path.display()))
                .and_then(|doc| {
                    let node_count = doc.metadata().total_nodes;
                    if node_count < min_nodes {
                        return Ok(None);
                    }
                    let shape = shape_of(&doc)?;
                    Ok(Some(Indexed {
                        path: path.display().to_string(),
                        group: group_key(root, path, group_depth),
                        shape,
                        node_count,
                    }))
                });
            (path.clone(), outcome)
        })
        .collect();

    let mut indexed = Vec::new();
    let mut errors = Vec::new();
    let mut suppressed = 0usize;
    for (path, outcome) in outcomes {
        match outcome {
            Ok(Some(entry)) => indexed.push(entry),
            Ok(None) => suppressed += 1,
            Err(e) => errors.push(CorpusError {
                file: path.display().to_string(),
                error: format!("{e:#}"),
            }),
        }
    }
    indexed.sort_by(|a, b| a.path.cmp(&b.path));
    errors.sort_by(|a, b| a.file.cmp(&b.file));

    // Inverted index: shape -> documents.
    let mut by_shape: BTreeMap<String, (u64, Vec<String>)> = BTreeMap::new();
    for entry in &indexed {
        let slot = by_shape
            .entry(entry.shape.clone())
            .or_insert((entry.node_count, Vec::new()));
        slot.0 = slot.0.min(entry.node_count);
        slot.1.push(entry.path.clone());
    }

    let distinct_shapes = by_shape.len();
    let mut reuse_groups: Vec<ReuseGroup> = by_shape
        .iter()
        .filter(|(_, (_, members))| members.len() > 1)
        .map(|(shape, (node_count, members))| ReuseGroup {
            shape: shape.clone(),
            count: members.len(),
            node_count: *node_count,
            members: members.clone(),
        })
        .collect();
    // Count descending, then shape, so ordering is fully specified.
    reuse_groups.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.shape.cmp(&b.shape)));
    let shapes_in_multiple_documents = reuse_groups.len();

    // Clustering links *groups*, not files: shape -> the distinct groups
    // carrying it. A shape confined to one group links nothing.
    let mut shape_to_groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for entry in &indexed {
        let slot = shape_to_groups.entry(&entry.shape).or_default();
        if !slot.contains(&entry.group.as_str()) {
            slot.push(&entry.group);
        }
    }
    let cross_group: Vec<(&str, Vec<&str>, u64)> = shape_to_groups
        .iter()
        .filter(|(_, groups)| groups.len() > 1)
        .map(|(shape, groups)| {
            let nodes = by_shape.get(*shape).map_or(0, |(n, _)| *n);
            let mut g = groups.clone();
            g.sort_unstable();
            (*shape, g, nodes)
        })
        .collect();

    let groups_indexed = indexed
        .iter()
        .map(|e| e.group.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let shapes_in_multiple_groups = cross_group.len();
    let clusters = build_clusters(&cross_group);

    CorpusIndex {
        files_scanned: scanned,
        documents_indexed: indexed.len(),
        groups_indexed,
        skipped: scanned.saturating_sub(files.len()),
        suppressed,
        distinct_shapes,
        shapes_in_multiple_documents,
        shapes_in_multiple_groups,
        reuse_groups,
        clusters,
        errors,
    }
}

/// Resolve the root of `x`, compressing the path as it goes.
fn find<'a>(parent: &mut BTreeMap<&'a str, &'a str>, x: &'a str) -> &'a str {
    let mut root = x;
    while let Some(&p) = parent.get(root) {
        if p == root {
            break;
        }
        root = p;
    }
    let mut cur = x;
    while let Some(&p) = parent.get(cur) {
        if p == root {
            break;
        }
        parent.insert(cur, root);
        cur = p;
    }
    parent.entry(x).or_insert(root);
    root
}

/// Link groups transitively through shared shapes via union-find.
///
/// Input is `(shape, groups carrying it, node count)` for shapes that span more
/// than one group. A shape confined to a single group links nothing and is not
/// passed in.
fn build_clusters(cross_group: &[(&str, Vec<&str>, u64)]) -> Vec<Cluster> {
    let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
    for (_, groups, _) in cross_group {
        for g in groups {
            parent.entry(g).or_insert(g);
        }
    }
    for (_, groups, _) in cross_group {
        let Some(first) = groups.first() else {
            continue;
        };
        for g in groups.iter().skip(1) {
            let ra = find(&mut parent, first);
            let rb = find(&mut parent, g);
            if ra != rb {
                parent.insert(ra, rb);
            }
        }
    }

    let mut members: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (_, groups, _) in cross_group {
        for g in groups {
            let root = find(&mut parent, g);
            members.entry(root).or_default().push((*g).to_owned());
        }
    }
    // Attribute each linking shape to the cluster it belongs to.
    let mut shape_nodes: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    for (_, groups, nodes) in cross_group {
        if let Some(first) = groups.first() {
            let root = find(&mut parent, first);
            shape_nodes.entry(root).or_default().push(*nodes);
        }
    }

    let mut clusters: Vec<Cluster> = members
        .into_iter()
        .map(|(root, mut mem)| {
            mem.sort();
            mem.dedup();
            let node_counts = shape_nodes.get(root).cloned().unwrap_or_default();
            Cluster {
                size: mem.len(),
                shared_shapes: node_counts.len(),
                min_node_count: node_counts.iter().copied().min().unwrap_or(0),
                members: mem,
            }
        })
        .collect();
    clusters.sort_by(|a, b| {
        b.size
            .cmp(&a.size)
            .then_with(|| b.shared_shapes.cmp(&a.shared_shapes))
            .then_with(|| a.members.first().cmp(&b.members.first()))
    });
    clusters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clusters_merge_through_shared_shapes() {
        // pkg-a + pkg-b share shape1; pkg-b + pkg-c share shape2.
        let cross = vec![
            ("shape1", vec!["pkg-a", "pkg-b"], 500u64),
            ("shape2", vec!["pkg-b", "pkg-c"], 600u64),
        ];
        let clusters = build_clusters(&cross);
        assert_eq!(clusters.len(), 1, "transitive link must merge them");
        assert_eq!(clusters[0].size, 3);
        assert_eq!(clusters[0].members, vec!["pkg-a", "pkg-b", "pkg-c"]);
        assert_eq!(clusters[0].shared_shapes, 2);
        assert_eq!(clusters[0].min_node_count, 500, "weakest link reported");
    }

    #[test]
    fn unrelated_groups_stay_separate() {
        let cross = vec![
            ("shape1", vec!["a", "b"], 500u64),
            ("shape2", vec!["c", "d"], 600u64),
        ];
        let clusters = build_clusters(&cross);
        assert_eq!(clusters.len(), 2);
        assert!(clusters.iter().all(|c| c.size == 2));
    }

    #[test]
    fn clusters_are_deterministic() {
        let cross = vec![
            ("s3", vec!["z", "y"], 100u64),
            ("s1", vec!["y", "x"], 200u64),
            ("s2", vec!["m", "n"], 300u64),
        ];
        let a = build_clusters(&cross);
        let b = build_clusters(&cross);
        let names =
            |c: &[Cluster]| -> Vec<Vec<String>> { c.iter().map(|x| x.members.clone()).collect() };
        assert_eq!(names(&a), names(&b));
        assert_eq!(a[0].size, 3);
        assert_eq!(a[0].members, vec!["x", "y", "z"]);
    }

    #[test]
    fn empty_input_yields_no_clusters() {
        assert!(build_clusters(&[]).is_empty());
    }

    #[test]
    fn group_key_uses_depth_below_root() {
        let root = Path::new("/corpus");
        let file = Path::new("/corpus/pkg-a/package/lib/index.js");
        assert_eq!(group_key(root, file, 1), "/corpus/pkg-a");
        assert_eq!(group_key(root, file, 2), "/corpus/pkg-a/package");
        // Depth 0 disables grouping: the file is its own unit.
        assert_eq!(
            group_key(root, file, 0),
            "/corpus/pkg-a/package/lib/index.js"
        );
    }

    /// Two files in the same package must land in one group, so a shape they
    /// both carry does not look like cross-package reuse.
    #[test]
    fn files_in_one_package_share_a_group() {
        let root = Path::new("/corpus");
        let a = Path::new("/corpus/pkg-a/lib/one.js");
        let b = Path::new("/corpus/pkg-a/lib/two.js");
        assert_eq!(group_key(root, a, 1), group_key(root, b, 1));
    }

    /// A depth deeper than the path itself must not panic or produce an empty
    /// key.
    #[test]
    fn group_key_handles_excess_depth() {
        let root = Path::new("/corpus");
        let file = Path::new("/corpus/a.js");
        assert_eq!(group_key(root, file, 9), "/corpus/a.js");
    }

    #[test]
    fn corpus_walk_recurses_and_counts_skips() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a.json"), "{}")?;
        std::fs::create_dir(dir.path().join("nested"))?;
        std::fs::write(dir.path().join("nested").join("b.json"), "{}")?;
        std::fs::create_dir(dir.path().join("nested").join("deep"))?;
        std::fs::write(dir.path().join("nested").join("deep").join("c.json"), "{}")?;
        std::fs::write(dir.path().join("readme.txt"), "x")?;

        let json_only = |p: &Path| p.extension().is_some_and(|e| e == "json");
        let (files, scanned) = collect_corpus_files(dir.path(), &json_only)?;
        assert_eq!(files.len(), 3, "walk must recurse");
        assert_eq!(scanned, 4, "scanned counts every file seen");
        assert!(files.windows(2).all(|w| w[0] <= w[1]), "sorted");
        Ok(())
    }

    #[test]
    fn corpus_walk_rejects_non_directory() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("a.json");
        std::fs::write(&file, "{}")?;
        let accept = |_: &Path| true;
        assert!(collect_corpus_files(&file, &accept).is_err());
        Ok(())
    }
}
