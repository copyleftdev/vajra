pub fn extract_field(value: &serde_json::Value, path: &str) -> Option<String> {
    let clean_path = path.strip_prefix("$.").unwrap_or(path);
    let mut current = value;
    for segment in clean_path.split('.') {
        if segment.is_empty() {
            continue;
        }
        match current.get(segment) {
            Some(child) => current = child,
            None => return None,
        }
    }
    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null => None,
        other => Some(other.to_string()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn simple_key() {
        assert_eq!(
            extract_field(&json!({"file":"main.rs"}), "file"),
            Some("main.rs".to_owned())
        );
    }
    #[test]
    fn nested_key() {
        assert_eq!(
            extract_field(&json!({"meta":{"file":"main.rs"}}), "meta.file"),
            Some("main.rs".to_owned())
        );
    }
    #[test]
    fn jsonpath_prefix() {
        assert_eq!(
            extract_field(&json!({"file":"main.rs"}), "$.file"),
            Some("main.rs".to_owned())
        );
    }
    #[test]
    fn missing_key() {
        assert_eq!(extract_field(&json!({"file":"main.rs"}), "missing"), None);
    }
    #[test]
    fn null_value() {
        assert_eq!(extract_field(&json!({"file":null}), "file"), None);
    }
}
