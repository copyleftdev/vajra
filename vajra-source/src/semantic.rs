//! Maps tree-sitter node kinds to human-readable semantic categories.
//!
//! Each mapping transforms a raw tree-sitter `kind` string (like
//! `"function_item"`) into a readable semantic label (like `"function"`).
//! Mappings are per-language because different grammars use different
//! node kind names for equivalent constructs.

/// Returns a human-readable semantic label for a tree-sitter node kind,
/// or `None` if no mapping exists.
///
/// The `lang` parameter should be the lowercase language name as produced
/// by `SourceLanguage::to_string()` (e.g. `"rust"`, `"python"`, `"go"`).
pub fn semantic_label(lang: &str, kind: &str) -> Option<&'static str> {
    match (lang, kind) {
        // -----------------------------------------------------------------
        // Rust
        // -----------------------------------------------------------------
        ("rust", "function_item") => Some("function"),
        ("rust", "struct_item") => Some("struct"),
        ("rust", "enum_item") => Some("enum"),
        ("rust", "impl_item") => Some("impl"),
        ("rust", "trait_item") => Some("trait"),
        ("rust", "mod_item") => Some("module"),
        ("rust", "use_declaration") => Some("import"),
        ("rust", "let_declaration") => Some("variable"),
        ("rust", "const_item") => Some("constant"),
        ("rust", "type_alias") => Some("type_alias"),
        ("rust", "macro_definition") => Some("macro"),

        // -----------------------------------------------------------------
        // Python
        // -----------------------------------------------------------------
        ("python", "function_definition") => Some("function"),
        ("python", "class_definition") => Some("class"),
        ("python", "import_statement") => Some("import"),
        ("python", "import_from_statement") => Some("import"),
        ("python", "assignment") => Some("variable"),
        ("python", "decorated_definition") => Some("decorated"),

        // -----------------------------------------------------------------
        // Go
        // -----------------------------------------------------------------
        ("go", "function_declaration") => Some("function"),
        ("go", "method_declaration") => Some("method"),
        ("go", "type_declaration") => Some("type"),
        ("go", "import_declaration") => Some("import"),
        ("go", "package_clause") => Some("package"),
        ("go", "struct_type") => Some("struct"),
        ("go", "interface_type") => Some("interface"),
        ("go", "var_declaration") => Some("variable"),
        ("go", "const_declaration") => Some("constant"),

        // -----------------------------------------------------------------
        // JavaScript / TypeScript
        // -----------------------------------------------------------------
        ("javascript" | "typescript", "function_declaration") => Some("function"),
        ("javascript" | "typescript", "arrow_function") => Some("function"),
        ("javascript" | "typescript", "class_declaration") => Some("class"),
        ("javascript" | "typescript", "import_statement") => Some("import"),
        ("javascript" | "typescript", "export_statement") => Some("export"),
        ("javascript" | "typescript", "variable_declaration") => Some("variable"),
        ("javascript" | "typescript", "interface_declaration") => Some("interface"),
        ("javascript" | "typescript", "type_alias_declaration") => Some("type_alias"),
        ("javascript" | "typescript", "enum_declaration") => Some("enum"),

        // -----------------------------------------------------------------
        // Java
        // -----------------------------------------------------------------
        ("java", "class_declaration") => Some("class"),
        ("java", "method_declaration") => Some("method"),
        ("java", "interface_declaration") => Some("interface"),
        ("java", "import_declaration") => Some("import"),
        ("java", "field_declaration") => Some("field"),
        ("java", "enum_declaration") => Some("enum"),
        ("java", "constructor_declaration") => Some("constructor"),

        // -----------------------------------------------------------------
        // C / C++
        // -----------------------------------------------------------------
        ("c" | "cpp", "function_definition") => Some("function"),
        ("c" | "cpp", "struct_specifier") => Some("struct"),
        ("c" | "cpp", "enum_specifier") => Some("enum"),
        ("c" | "cpp", "declaration") => Some("declaration"),
        ("cpp", "class_specifier") => Some("class"),
        ("cpp", "namespace_definition") => Some("namespace"),

        // -----------------------------------------------------------------
        // Ruby
        // -----------------------------------------------------------------
        ("ruby", "method") => Some("method"),
        ("ruby", "class") => Some("class"),
        ("ruby", "module") => Some("module"),
        ("ruby", "assignment") => Some("variable"),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Rust mappings
    // -----------------------------------------------------------------

    #[test]
    fn rust_function_item() {
        assert_eq!(semantic_label("rust", "function_item"), Some("function"));
    }

    #[test]
    fn rust_struct_item() {
        assert_eq!(semantic_label("rust", "struct_item"), Some("struct"));
    }

    #[test]
    fn rust_enum_item() {
        assert_eq!(semantic_label("rust", "enum_item"), Some("enum"));
    }

    #[test]
    fn rust_impl_item() {
        assert_eq!(semantic_label("rust", "impl_item"), Some("impl"));
    }

    #[test]
    fn rust_trait_item() {
        assert_eq!(semantic_label("rust", "trait_item"), Some("trait"));
    }

    #[test]
    fn rust_use_declaration() {
        assert_eq!(semantic_label("rust", "use_declaration"), Some("import"));
    }

    #[test]
    fn rust_const_item() {
        assert_eq!(semantic_label("rust", "const_item"), Some("constant"));
    }

    #[test]
    fn rust_macro_definition() {
        assert_eq!(semantic_label("rust", "macro_definition"), Some("macro"));
    }

    // -----------------------------------------------------------------
    // Python mappings
    // -----------------------------------------------------------------

    #[test]
    fn python_function_definition() {
        assert_eq!(
            semantic_label("python", "function_definition"),
            Some("function")
        );
    }

    #[test]
    fn python_class_definition() {
        assert_eq!(semantic_label("python", "class_definition"), Some("class"));
    }

    #[test]
    fn python_import_statement() {
        assert_eq!(semantic_label("python", "import_statement"), Some("import"));
    }

    #[test]
    fn python_import_from_statement() {
        assert_eq!(
            semantic_label("python", "import_from_statement"),
            Some("import")
        );
    }

    #[test]
    fn python_assignment() {
        assert_eq!(semantic_label("python", "assignment"), Some("variable"));
    }

    #[test]
    fn python_decorated_definition() {
        assert_eq!(
            semantic_label("python", "decorated_definition"),
            Some("decorated")
        );
    }

    // -----------------------------------------------------------------
    // Go mappings
    // -----------------------------------------------------------------

    #[test]
    fn go_function_declaration() {
        assert_eq!(
            semantic_label("go", "function_declaration"),
            Some("function")
        );
    }

    #[test]
    fn go_method_declaration() {
        assert_eq!(semantic_label("go", "method_declaration"), Some("method"));
    }

    #[test]
    fn go_type_declaration() {
        assert_eq!(semantic_label("go", "type_declaration"), Some("type"));
    }

    #[test]
    fn go_import_declaration() {
        assert_eq!(semantic_label("go", "import_declaration"), Some("import"));
    }

    #[test]
    fn go_package_clause() {
        assert_eq!(semantic_label("go", "package_clause"), Some("package"));
    }

    #[test]
    fn go_struct_type() {
        assert_eq!(semantic_label("go", "struct_type"), Some("struct"));
    }

    #[test]
    fn go_interface_type() {
        assert_eq!(semantic_label("go", "interface_type"), Some("interface"));
    }

    // -----------------------------------------------------------------
    // JavaScript / TypeScript mappings
    // -----------------------------------------------------------------

    #[test]
    fn js_function_declaration() {
        assert_eq!(
            semantic_label("javascript", "function_declaration"),
            Some("function")
        );
    }

    #[test]
    fn js_arrow_function() {
        assert_eq!(
            semantic_label("javascript", "arrow_function"),
            Some("function")
        );
    }

    #[test]
    fn js_class_declaration() {
        assert_eq!(
            semantic_label("javascript", "class_declaration"),
            Some("class")
        );
    }

    #[test]
    fn js_import_statement() {
        assert_eq!(
            semantic_label("javascript", "import_statement"),
            Some("import")
        );
    }

    #[test]
    fn js_export_statement() {
        assert_eq!(
            semantic_label("javascript", "export_statement"),
            Some("export")
        );
    }

    #[test]
    fn js_variable_declaration() {
        assert_eq!(
            semantic_label("javascript", "variable_declaration"),
            Some("variable")
        );
    }

    #[test]
    fn ts_function_declaration() {
        assert_eq!(
            semantic_label("typescript", "function_declaration"),
            Some("function")
        );
    }

    #[test]
    fn ts_class_declaration() {
        assert_eq!(
            semantic_label("typescript", "class_declaration"),
            Some("class")
        );
    }

    #[test]
    fn ts_interface_declaration() {
        assert_eq!(
            semantic_label("typescript", "interface_declaration"),
            Some("interface")
        );
    }

    // -----------------------------------------------------------------
    // Java mappings
    // -----------------------------------------------------------------

    #[test]
    fn java_class_declaration() {
        assert_eq!(semantic_label("java", "class_declaration"), Some("class"));
    }

    #[test]
    fn java_method_declaration() {
        assert_eq!(semantic_label("java", "method_declaration"), Some("method"));
    }

    #[test]
    fn java_interface_declaration() {
        assert_eq!(
            semantic_label("java", "interface_declaration"),
            Some("interface")
        );
    }

    #[test]
    fn java_constructor_declaration() {
        assert_eq!(
            semantic_label("java", "constructor_declaration"),
            Some("constructor")
        );
    }

    // -----------------------------------------------------------------
    // C / C++ mappings
    // -----------------------------------------------------------------

    #[test]
    fn c_function_definition() {
        assert_eq!(semantic_label("c", "function_definition"), Some("function"));
    }

    #[test]
    fn c_struct_specifier() {
        assert_eq!(semantic_label("c", "struct_specifier"), Some("struct"));
    }

    #[test]
    fn cpp_class_specifier() {
        assert_eq!(semantic_label("cpp", "class_specifier"), Some("class"));
    }

    #[test]
    fn cpp_namespace_definition() {
        assert_eq!(
            semantic_label("cpp", "namespace_definition"),
            Some("namespace")
        );
    }

    #[test]
    fn cpp_inherits_c_mappings() {
        assert_eq!(
            semantic_label("cpp", "function_definition"),
            Some("function")
        );
        assert_eq!(semantic_label("cpp", "struct_specifier"), Some("struct"));
    }

    // -----------------------------------------------------------------
    // Ruby mappings
    // -----------------------------------------------------------------

    #[test]
    fn ruby_method() {
        assert_eq!(semantic_label("ruby", "method"), Some("method"));
    }

    #[test]
    fn ruby_class() {
        assert_eq!(semantic_label("ruby", "class"), Some("class"));
    }

    #[test]
    fn ruby_module() {
        assert_eq!(semantic_label("ruby", "module"), Some("module"));
    }

    // -----------------------------------------------------------------
    // Unknown / fallback
    // -----------------------------------------------------------------

    #[test]
    fn unknown_kind_returns_none() {
        assert_eq!(semantic_label("rust", "totally_made_up_kind"), None);
    }

    #[test]
    fn unknown_language_returns_none() {
        assert_eq!(semantic_label("brainfuck", "function_item"), None);
    }

    #[test]
    fn empty_inputs_return_none() {
        assert_eq!(semantic_label("", ""), None);
        assert_eq!(semantic_label("rust", ""), None);
        assert_eq!(semantic_label("", "function_item"), None);
    }

    #[test]
    fn determinism_100_runs() {
        let cases = vec![
            ("rust", "function_item"),
            ("python", "class_definition"),
            ("go", "method_declaration"),
            ("javascript", "arrow_function"),
            ("java", "class_declaration"),
            ("c", "struct_specifier"),
            ("ruby", "method"),
            ("rust", "unknown_kind"),
        ];
        let baseline: Vec<Option<&str>> = cases.iter().map(|(l, k)| semantic_label(l, k)).collect();
        for _ in 0..100 {
            let run: Vec<Option<&str>> = cases.iter().map(|(l, k)| semantic_label(l, k)).collect();
            assert_eq!(baseline, run);
        }
    }
}
