//! JSON value type enumeration with support for semantic lifting.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The seven JSON value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonType {
    Null,
    Bool,
    Integer,
    Float,
    String,
    Array,
    Object,
}

impl JsonType {
    /// Classify a `serde_json::Value` into its `JsonType`.
    #[must_use]
    pub fn from_value(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(_) => Self::Bool,
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    Self::Integer
                } else {
                    Self::Float
                }
            }
            serde_json::Value::String(_) => Self::String,
            serde_json::Value::Array(_) => Self::Array,
            serde_json::Value::Object(_) => Self::Object,
        }
    }

    /// Returns `true` if this type is a scalar (not object or array).
    #[must_use]
    pub const fn is_scalar(self) -> bool {
        matches!(
            self,
            Self::Null | Self::Bool | Self::Integer | Self::Float | Self::String
        )
    }

    /// Returns `true` if this type is a container (object or array).
    #[must_use]
    pub const fn is_container(self) -> bool {
        matches!(self, Self::Array | Self::Object)
    }

    /// Returns a stable numeric index for this type (for fixed-size arrays).
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Null => 0,
            Self::Bool => 1,
            Self::Integer => 2,
            Self::Float => 3,
            Self::String => 4,
            Self::Array => 5,
            Self::Object => 6,
        }
    }

    /// Total number of type variants.
    pub const COUNT: usize = 7;
}

impl fmt::Display for JsonType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool => write!(f, "boolean"),
            Self::Integer => write!(f, "integer"),
            Self::Float => write!(f, "float"),
            Self::String => write!(f, "string"),
            Self::Array => write!(f, "array"),
            Self::Object => write!(f, "object"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_json_values() {
        assert_eq!(JsonType::from_value(&serde_json::Value::Null), JsonType::Null);
        assert_eq!(JsonType::from_value(&serde_json::json!(true)), JsonType::Bool);
        assert_eq!(JsonType::from_value(&serde_json::json!(42)), JsonType::Integer);
        assert_eq!(JsonType::from_value(&serde_json::json!(3.14)), JsonType::Float);
        assert_eq!(JsonType::from_value(&serde_json::json!("hello")), JsonType::String);
        assert_eq!(JsonType::from_value(&serde_json::json!([1, 2])), JsonType::Array);
        assert_eq!(JsonType::from_value(&serde_json::json!({"a": 1})), JsonType::Object);
    }

    #[test]
    fn indices_are_unique() {
        let types = [
            JsonType::Null,
            JsonType::Bool,
            JsonType::Integer,
            JsonType::Float,
            JsonType::String,
            JsonType::Array,
            JsonType::Object,
        ];
        let indices: Vec<usize> = types.iter().map(|t| t.index()).collect();
        for (i, idx) in indices.iter().enumerate() {
            for (j, jdx) in indices.iter().enumerate() {
                if i != j {
                    assert_ne!(idx, jdx, "types {:?} and {:?} share index", types[i], types[j]);
                }
            }
        }
    }

    #[test]
    fn scalar_vs_container() {
        assert!(JsonType::Null.is_scalar());
        assert!(JsonType::String.is_scalar());
        assert!(JsonType::Integer.is_scalar());
        assert!(!JsonType::Array.is_scalar());
        assert!(!JsonType::Object.is_scalar());
        assert!(JsonType::Array.is_container());
        assert!(JsonType::Object.is_container());
    }
}
