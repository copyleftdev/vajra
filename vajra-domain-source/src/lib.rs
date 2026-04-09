//! Source code domain plugin for Vajra.
//!
//! Provides type recognizers for common code patterns (naming conventions,
//! import paths, file paths) and relationship hints for tree-sitter CST
//! structures produced by vajra-source.

pub mod hints;
pub mod recognizers;

use vajra_types::traits::{RelationshipHint, TypeRecognizer, VajraPlugin};

use crate::hints::source_hints;
use crate::recognizers::{
    CamelCaseRecognizer, ImportPathRecognizer, PascalCaseRecognizer, ScreamingSnakeCaseRecognizer,
    SnakeCaseRecognizer, SourceFilePathRecognizer,
};

/// The source code domain plugin.
///
/// Registers code-specific type recognizers and relationship hints
/// to enhance Vajra's analysis of tree-sitter CST JSON produced by
/// vajra-source.
pub struct SourcePlugin;

impl VajraPlugin for SourcePlugin {
    fn name(&self) -> &str {
        "source"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![
            Box::new(SnakeCaseRecognizer),
            Box::new(CamelCaseRecognizer),
            Box::new(PascalCaseRecognizer),
            Box::new(ScreamingSnakeCaseRecognizer),
            Box::new(ImportPathRecognizer),
            Box::new(SourceFilePathRecognizer),
        ]
    }

    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        source_hints()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::VajraPlugin;

    #[test]
    fn plugin_name_and_version() {
        let p = SourcePlugin;
        assert_eq!(p.name(), "source");
        assert_eq!(p.version(), "0.1.0");
    }

    #[test]
    fn plugin_returns_correct_recognizer_count() {
        assert_eq!(SourcePlugin.type_recognizers().len(), 6);
    }

    #[test]
    fn plugin_returns_correct_hint_count() {
        assert_eq!(SourcePlugin.relationship_hints().len(), 6);
    }

    #[test]
    fn recognizer_names_are_unique() {
        let recognizers = SourcePlugin.type_recognizers();
        let mut names: Vec<&str> = recognizers.iter().map(|r| r.type_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), recognizers.len());
    }

    #[test]
    fn recognizers_have_positive_priority() {
        for r in &SourcePlugin.type_recognizers() {
            assert!(r.priority() > 0, "{} priority must be > 0", r.type_name());
        }
    }

    #[test]
    fn plugin_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SourcePlugin>();
    }

    #[test]
    fn hint_names_match_expected() {
        let hints = SourcePlugin.relationship_hints();
        let names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"function_definition"));
        assert!(names.contains(&"class_definition"));
        assert!(names.contains(&"import_statement"));
        assert!(names.contains(&"parameter_list"));
        assert!(names.contains(&"conditional_block"));
        assert!(names.contains(&"loop_block"));
    }

    #[test]
    fn determinism_recognizers_10_runs() {
        let test_values = [
            "my_function",
            "myFunction",
            "MyClass",
            "MAX_SIZE",
            "std::collections::HashMap",
            "src/main.rs",
            "not-a-pattern",
            "",
        ];
        let mut results: Vec<Vec<Vec<bool>>> = Vec::new();
        for _ in 0..10 {
            let recognizers = SourcePlugin.type_recognizers();
            let run: Vec<Vec<bool>> = test_values
                .iter()
                .map(|v| recognizers.iter().map(|r| r.matches(v)).collect())
                .collect();
            results.push(run);
        }
        for (i, run) in results.iter().enumerate().skip(1) {
            assert_eq!(&results[0], run, "run 0 and run {} differ", i);
        }
    }

    #[test]
    fn determinism_hints_10_runs() {
        let mut results: Vec<Vec<String>> = Vec::new();
        for _ in 0..10 {
            let hints = SourcePlugin.relationship_hints();
            let names: Vec<String> = hints.iter().map(|h| h.name.clone()).collect();
            results.push(names);
        }
        for (i, run) in results.iter().enumerate().skip(1) {
            assert_eq!(&results[0], run, "run 0 and run {} differ", i);
        }
    }
}
