//! Wildcard path representation for JSON structural analysis.
//!
//! A `WildcardPath` normalizes concrete array indices to `[*]`, creating
//! a structural signature that groups all elements at the same position
//! in the schema. For example, `$.claims[0].code` and `$.claims[1].code`
//! both normalize to `$.claims[*].code`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A single segment in a wildcard path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PathSegment {
    /// The root `$` segment.
    Root,
    /// An object key.
    Key(String),
    /// A wildcard array index `[*]`.
    ArrayWildcard,
}

impl fmt::Display for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root => write!(f, "$"),
            Self::Key(k) => write!(f, ".{k}"),
            Self::ArrayWildcard => write!(f, "[*]"),
        }
    }
}

/// A normalized path through a JSON document where all array indices
/// are replaced with wildcards.
///
/// Ordering is lexicographic over segments, giving deterministic
/// iteration order in `BTreeMap`/`BTreeSet`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WildcardPath {
    segments: Vec<PathSegment>,
}

impl WildcardPath {
    /// Create a new root path (`$`).
    #[must_use]
    pub fn root() -> Self {
        Self {
            segments: vec![PathSegment::Root],
        }
    }

    /// Append an object key segment.
    #[must_use]
    pub fn push_key(&self, key: &str) -> Self {
        let mut segments = self.segments.clone();
        segments.push(PathSegment::Key(key.to_owned()));
        Self { segments }
    }

    /// Append an array wildcard segment.
    #[must_use]
    pub fn push_array_wildcard(&self) -> Self {
        let mut segments = self.segments.clone();
        segments.push(PathSegment::ArrayWildcard);
        Self { segments }
    }

    /// Returns the depth (number of segments excluding root).
    #[must_use]
    pub fn depth(&self) -> u32 {
        self.segments.len().saturating_sub(1) as u32
    }

    /// Returns the segments.
    #[must_use]
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// Returns the parent path, or `None` if this is the root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.segments.len() <= 1 {
            return None;
        }
        Some(Self {
            segments: self.segments[..self.segments.len() - 1].to_vec(),
        })
    }

    /// Returns the last segment, or `None` if this is empty.
    #[must_use]
    pub fn last_segment(&self) -> Option<&PathSegment> {
        self.segments.last()
    }

    /// Returns the canonical string representation.
    #[must_use]
    pub fn as_str(&self) -> String {
        format!("{self}")
    }

    /// Parse a dot-path string into a `WildcardPath`.
    #[must_use]
    pub fn from_dotpath(path: &str) -> Self {
        let stripped = path.strip_prefix('$').unwrap_or(path);
        let stripped = stripped.strip_prefix('.').unwrap_or(stripped);
        let mut segments = vec![PathSegment::Root];
        if stripped.is_empty() {
            return Self { segments };
        }
        for part in stripped.split('.') {
            if part.is_empty() {
                continue;
            }
            if let Some(key) = part.strip_suffix("[*]") {
                if !key.is_empty() {
                    segments.push(PathSegment::Key(key.to_owned()));
                }
                segments.push(PathSegment::ArrayWildcard);
            } else {
                segments.push(PathSegment::Key(part.to_owned()));
            }
        }
        Self { segments }
    }
}

impl fmt::Display for WildcardPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.segments {
            write!(f, "{segment}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for WildcardPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WildcardPath({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_path() {
        let p = WildcardPath::root();
        assert_eq!(p.to_string(), "$");
        assert_eq!(p.depth(), 0);
        assert!(p.parent().is_none());
    }

    #[test]
    fn nested_path() {
        let p = WildcardPath::root()
            .push_key("claims")
            .push_array_wildcard()
            .push_key("diagnosis")
            .push_array_wildcard()
            .push_key("code");
        assert_eq!(p.to_string(), "$.claims[*].diagnosis[*].code");
        assert_eq!(p.depth(), 5);
    }

    #[test]
    fn parent_navigation() {
        let p = WildcardPath::root().push_key("a").push_key("b");
        let parent = p.parent();
        assert_eq!(
            parent.as_ref().map(|p| p.to_string()),
            Some("$.a".to_owned())
        );
        let grandparent = parent.and_then(|p| p.parent());
        assert_eq!(
            grandparent.as_ref().map(|p| p.to_string()),
            Some("$".to_owned())
        );
    }

    #[test]
    fn ordering_is_deterministic() {
        let a = WildcardPath::root().push_key("alpha");
        let b = WildcardPath::root().push_key("beta");
        let arr = WildcardPath::root().push_key("alpha").push_array_wildcard();

        // Key ordering: alpha < beta (lexicographic)
        assert!(a < b);
        // Depth extends ordering: $.alpha < $.alpha[*]
        assert!(a < arr);
    }
}
