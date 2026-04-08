//! JSON event model for streaming analysis.
//!
//! Events represent the structural skeleton of a JSON document as a flat
//! sequence: container open/close brackets and scalar values, each tagged
//! with their [`WildcardPath`] position.  This lets streaming analyzers
//! process data without holding the entire DOM in memory.

use vajra_types::path::WildcardPath;

/// A JSON parsing event for streaming analysis.
#[derive(Debug, Clone)]
pub enum JsonEvent {
    /// Start of an object at the given path.
    ObjectStart { path: WildcardPath },
    /// End of an object.
    ObjectEnd { path: WildcardPath },
    /// Start of an array.
    ArrayStart { path: WildcardPath },
    /// End of an array.
    ArrayEnd { path: WildcardPath },
    /// A scalar value at the given path.
    Value {
        path: WildcardPath,
        value: ScalarValue,
    },
}

impl JsonEvent {
    /// Returns a reference to the path associated with this event.
    #[must_use]
    pub fn path(&self) -> &WildcardPath {
        match self {
            Self::ObjectStart { path }
            | Self::ObjectEnd { path }
            | Self::ArrayStart { path }
            | Self::ArrayEnd { path }
            | Self::Value { path, .. } => path,
        }
    }
}

/// A scalar JSON value without containers.
#[derive(Debug, Clone)]
pub enum ScalarValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
}

impl ScalarValue {
    /// Convert to a deterministic canonical string for frequency counting.
    #[must_use]
    pub fn to_canonical_string(&self) -> String {
        match self {
            Self::Null => "null".to_owned(),
            Self::Bool(b) => {
                if *b {
                    "true".to_owned()
                } else {
                    "false".to_owned()
                }
            }
            Self::Integer(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::String(s) => s.clone(),
        }
    }

    /// Returns the numeric value as `f64`, if this is numeric.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Integer(i) => Some(*i as f64),
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns the JSON type index consistent with `JsonType::index()`.
    #[must_use]
    pub const fn type_index(&self) -> usize {
        match self {
            Self::Null => 0,
            Self::Bool(_) => 1,
            Self::Integer(_) => 2,
            Self::Float(_) => 3,
            Self::String(_) => 4,
        }
    }

    /// Returns `true` if this is a null value.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_canonical_strings() {
        assert_eq!(ScalarValue::Null.to_canonical_string(), "null");
        assert_eq!(ScalarValue::Bool(true).to_canonical_string(), "true");
        assert_eq!(ScalarValue::Bool(false).to_canonical_string(), "false");
        assert_eq!(ScalarValue::Integer(42).to_canonical_string(), "42");
        assert_eq!(ScalarValue::String("hi".into()).to_canonical_string(), "hi");
    }

    #[test]
    fn scalar_as_f64() {
        assert_eq!(ScalarValue::Integer(10).as_f64(), Some(10.0));
        assert_eq!(ScalarValue::Float(2.75).as_f64(), Some(2.75));
        assert_eq!(ScalarValue::Null.as_f64(), None);
        assert_eq!(ScalarValue::String("x".into()).as_f64(), None);
    }

    #[test]
    fn scalar_type_index() {
        assert_eq!(ScalarValue::Null.type_index(), 0);
        assert_eq!(ScalarValue::Bool(true).type_index(), 1);
        assert_eq!(ScalarValue::Integer(1).type_index(), 2);
        assert_eq!(ScalarValue::Float(1.0).type_index(), 3);
        assert_eq!(ScalarValue::String("a".into()).type_index(), 4);
    }

    #[test]
    fn event_path_accessor() {
        let p = WildcardPath::root().push_key("a");
        let event = JsonEvent::Value {
            path: p.clone(),
            value: ScalarValue::Null,
        };
        assert_eq!(event.path(), &p);
    }
}
