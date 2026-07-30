//! Contributor identity resolution.
//!
//! Git records an author as a free-text `(name, email)` pair chosen per commit,
//! so one person routinely appears as several. On a real repository the
//! maintainer committed under five pairs across three addresses, and the alias
//! ranked second in `core-team` output was the same human as the alias ranked
//! first. Keying contributors on a single field counts them separately, which
//! moves every concentration metric: deduplicating that repository took
//! `bus_factor_50` from 3 to 1 and `top1_share` from 0.369 to 0.519. See #88.
//!
//! Two `(name, email)` pairs belong to the same identity when they share a name
//! or share an email, transitively. That is deliberately aggressive — two
//! distinct people called "John Smith" merge — so resolution is opt-in and
//! reports every merge it made rather than folding them in silently.

use std::collections::BTreeMap;

/// One resolved contributor identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The name this identity is reported under.
    pub canonical: String,
    /// Every observed name folded into it, sorted, including the canonical one.
    pub names: Vec<String>,
    /// Every observed email, normalised and sorted.
    pub emails: Vec<String>,
    /// Observations backing this identity.
    pub occurrences: usize,
}

/// The outcome of resolving a set of `(name, email)` observations.
#[derive(Debug, Clone, Default)]
pub struct IdentityResolution {
    /// Observed name to the canonical name it resolves to.
    ///
    /// Contains every observed name, including those that resolve to
    /// themselves, so a caller can map without a fallback branch.
    pub canonical_of: BTreeMap<String, String>,
    /// Identities that folded in more than one name, sorted by canonical name.
    /// The ones that did not are omitted: they are not merges to report.
    pub merged: Vec<Identity>,
    /// Distinct identities after resolution.
    pub identity_count: usize,
    /// Distinct names before resolution.
    pub observed_names: usize,
    /// Canonical name to a single representative email for that identity.
    ///
    /// Consumers that key a contributor on `(name, email)` — `core-team` does —
    /// must rewrite both halves, or one identity with three addresses still
    /// counts three times after its name is unified.
    pub representative_email: BTreeMap<String, String>,
}

impl IdentityResolution {
    /// Canonical name for an observed name, or the name itself if unknown.
    #[must_use]
    pub fn canonical<'a>(&'a self, name: &'a str) -> &'a str {
        self.canonical_of.get(name).map_or(name, String::as_str)
    }

    /// Representative email for an identity, given any of its observed names.
    #[must_use]
    pub fn email_for(&self, name: &str) -> Option<&str> {
        self.representative_email
            .get(self.canonical(name))
            .map(String::as_str)
    }

    /// How many names were absorbed into another identity.
    #[must_use]
    pub fn names_merged(&self) -> usize {
        self.observed_names.saturating_sub(self.identity_count)
    }
}

/// A node in the bipartite name/email graph.
///
/// Names and emails share one union-find, so a name links the emails it was
/// seen with and an email links the names, transitively.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Node {
    Name(String),
    Email(String),
}

/// Union-find over `Node`, keyed by a `BTreeMap` so iteration is deterministic.
struct Union {
    parent: BTreeMap<Node, Node>,
}

impl Union {
    fn new() -> Self {
        Self {
            parent: BTreeMap::new(),
        }
    }

    fn find(&mut self, node: &Node) -> Node {
        let mut current = node.clone();
        loop {
            let parent = self
                .parent
                .entry(current.clone())
                .or_insert_with(|| current.clone())
                .clone();
            if parent == current {
                return current;
            }
            // Path halving, so repeated lookups stay near-constant without
            // recursion.
            let grandparent = self
                .parent
                .get(&parent)
                .cloned()
                .unwrap_or_else(|| parent.clone());
            self.parent.insert(current.clone(), grandparent.clone());
            current = grandparent;
        }
    }

    fn union(&mut self, a: &Node, b: &Node) {
        let (root_a, root_b) = (self.find(a), self.find(b));
        if root_a != root_b {
            // Smaller root wins, so the component's representative does not
            // depend on the order observations arrived in.
            let (keep, drop) = if root_a < root_b {
                (root_a, root_b)
            } else {
                (root_b, root_a)
            };
            self.parent.insert(drop, keep);
        }
    }
}

/// Normalise an email for comparison.
///
/// Lowercases, and strips GitHub's `<user-id>+` prefix so
/// `137048761+alice@users.noreply.github.com` and
/// `alice@users.noreply.github.com` are the same address. Only an all-digit
/// prefix is stripped; `first+last@example.com` is left alone.
#[must_use]
pub fn normalise_email(email: &str) -> String {
    let lower = email.trim().to_lowercase();
    let Some((local, domain)) = lower.split_once('@') else {
        return lower;
    };
    let local = match local.split_once('+') {
        Some((prefix, rest))
            if !prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit()) =>
        {
            rest
        }
        _ => local,
    };
    format!("{local}@{domain}")
}

/// Resolve `(name, email)` observations into identities.
///
/// Observations with an empty name are skipped: there is nothing to report them
/// under. An empty email simply contributes no email edge, so such a name only
/// merges with others of the same name.
///
/// The canonical name of an identity is its most frequently observed name, ties
/// broken lexicographically so the result does not depend on input order.
#[must_use]
pub fn resolve_identities<'a, I>(observations: I) -> IdentityResolution
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut union = Union::new();
    let mut name_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut name_emails: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (name, email) in observations {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        *name_counts.entry(name.to_owned()).or_insert(0) += 1;

        let name_node = Node::Name(name.to_owned());
        union.find(&name_node);

        let email = normalise_email(email);
        if !email.is_empty() {
            union.union(&name_node, &Node::Email(email.clone()));
            name_emails.entry(name.to_owned()).or_default().push(email);
        }
    }

    // Group names by component root.
    let mut components: BTreeMap<Node, Vec<String>> = BTreeMap::new();
    for name in name_counts.keys() {
        let root = union.find(&Node::Name(name.clone()));
        components.entry(root).or_default().push(name.clone());
    }

    let observed_names = name_counts.len();
    let identity_count = components.len();
    let mut canonical_of = BTreeMap::new();
    let mut representative_email = BTreeMap::new();
    let mut merged = Vec::new();

    for names in components.values() {
        // Most observations wins; ties go to the lexicographically smaller
        // name so the label is stable.
        let canonical = names
            .iter()
            .max_by(|a, b| {
                let count_a = name_counts.get(*a).copied().unwrap_or(0);
                let count_b = name_counts.get(*b).copied().unwrap_or(0);
                count_a.cmp(&count_b).then_with(|| b.cmp(a))
            })
            .cloned()
            .unwrap_or_default();

        for name in names {
            canonical_of.insert(name.clone(), canonical.clone());
        }

        let mut emails: Vec<String> = names
            .iter()
            .filter_map(|n| name_emails.get(n))
            .flatten()
            .cloned()
            .collect();
        emails.sort();
        emails.dedup();
        // First lexicographically, so the representative is stable and does not
        // depend on which commit happened to come first.
        if let Some(first) = emails.first() {
            representative_email.insert(canonical.clone(), first.clone());
        }

        if names.len() > 1 {
            merged.push(Identity {
                canonical,
                names: names.clone(),
                emails,
                occurrences: names
                    .iter()
                    .map(|n| name_counts.get(n).copied().unwrap_or(0))
                    .sum(),
            });
        }
    }

    merged.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then(a.canonical.cmp(&b.canonical))
    });

    IdentityResolution {
        canonical_of,
        merged,
        identity_count,
        observed_names,
        representative_email,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real case from #88: one person, five (name, email) pairs across
    /// three addresses, linked through shared names and shared emails.
    #[test]
    fn a_maintainer_with_five_aliases_resolves_to_one_identity() {
        let observations = vec![
            (
                "DietrichGebert",
                "137048761+DietrichGebert@users.noreply.github.com",
            ),
            ("DietrichGebert", "dietrich.gebert@gmail.com"),
            ("DietrichGebert", "dietrich_gebert@trimble.com"),
            ("dgebert", "dietrich_gebert@trimble.com"),
            ("Emeriko", "dietrich.gebert@gmail.com"),
            ("Alice", "alice@example.com"),
        ];
        let r = resolve_identities(observations.iter().map(|(n, e)| (*n, *e)));

        assert_eq!(
            r.observed_names, 4,
            "DietrichGebert, dgebert, Emeriko, Alice"
        );
        assert_eq!(r.identity_count, 2, "the maintainer plus Alice");
        assert_eq!(r.canonical("Emeriko"), "DietrichGebert");
        assert_eq!(r.canonical("dgebert"), "DietrichGebert");
        assert_eq!(r.canonical("Alice"), "Alice");
        assert_eq!(r.names_merged(), 2);

        assert_eq!(r.merged.len(), 1, "only the maintainer is a merge");
        assert_eq!(r.merged[0].canonical, "DietrichGebert");
        assert_eq!(
            r.merged[0].names,
            vec!["DietrichGebert", "Emeriko", "dgebert"]
        );
    }

    /// `137048761+alice@users.noreply.github.com` is the same address as
    /// `alice@users.noreply.github.com`, so two names carrying them merge.
    #[test]
    fn the_github_id_prefix_does_not_split_an_identity() {
        let r = resolve_identities(
            [
                ("Alice A", "137048761+alice@users.noreply.github.com"),
                ("Alice B", "alice@users.noreply.github.com"),
            ]
            .into_iter(),
        );
        assert_eq!(r.identity_count, 1);
    }

    #[test]
    fn a_non_numeric_plus_tag_is_not_stripped() {
        assert_eq!(
            normalise_email("first+last@example.com"),
            "first+last@example.com"
        );
        assert_eq!(
            normalise_email("Alice+Work@Example.COM"),
            "alice+work@example.com"
        );
        assert_eq!(normalise_email("49699333+bot@x.com"), "bot@x.com");
        assert_eq!(normalise_email("no-at-sign"), "no-at-sign");
    }

    #[test]
    fn distinct_people_are_not_merged() {
        let r = resolve_identities(
            [
                ("Alice", "alice@example.com"),
                ("Bob", "bob@example.com"),
                ("Carol", "carol@example.com"),
            ]
            .into_iter(),
        );
        assert_eq!(r.identity_count, 3);
        assert!(r.merged.is_empty(), "nothing to report when nothing merged");
        assert_eq!(r.names_merged(), 0);
    }

    /// The canonical name is the most-observed one, not the first seen.
    #[test]
    fn the_canonical_name_is_the_most_frequent() {
        let r = resolve_identities(
            [
                ("rare", "x@example.com"),
                ("common", "x@example.com"),
                ("common", "x@example.com"),
                ("common", "x@example.com"),
            ]
            .into_iter(),
        );
        assert_eq!(r.canonical("rare"), "common");
    }

    /// Order must not change the outcome — determinism is the point.
    #[test]
    fn resolution_is_order_independent() {
        let forward = vec![
            ("A", "shared@example.com"),
            ("B", "shared@example.com"),
            ("B", "other@example.com"),
            ("C", "other@example.com"),
        ];
        let mut reversed = forward.clone();
        reversed.reverse();

        let a = resolve_identities(forward.iter().map(|(n, e)| (*n, *e)));
        let b = resolve_identities(reversed.iter().map(|(n, e)| (*n, *e)));
        assert_eq!(a.canonical_of, b.canonical_of);
        assert_eq!(a.identity_count, 1, "A-B-C chain through shared emails");
    }

    #[test]
    fn a_missing_email_only_merges_by_name() {
        let r = resolve_identities(
            [("Alice", ""), ("Alice", "alice@example.com"), ("Bob", "")].into_iter(),
        );
        assert_eq!(r.identity_count, 2);
        assert_eq!(r.canonical("Alice"), "Alice");
        assert_eq!(r.canonical("Bob"), "Bob");
    }

    #[test]
    fn an_empty_name_is_skipped() {
        let r =
            resolve_identities([("", "ghost@example.com"), ("Alice", "a@example.com")].into_iter());
        assert_eq!(r.observed_names, 1);
        assert_eq!(r.identity_count, 1);
    }

    #[test]
    fn empty_input_resolves_to_nothing() {
        let r = resolve_identities(std::iter::empty());
        assert_eq!(r.identity_count, 0);
        assert_eq!(r.observed_names, 0);
        assert_eq!(r.names_merged(), 0);
    }
}
