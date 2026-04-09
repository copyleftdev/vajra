//! Chaos/adversarial tests for vajra-source.
//!
//! These tests throw pathological inputs at the source parser to verify
//! it never panics, never hangs, and always returns either Ok or Err.

use vajra_source::{parse_source, SourceConfig, SourceLanguage};

fn rust_config() -> SourceConfig {
    SourceConfig {
        language: Some(SourceLanguage::Rust),
        ..SourceConfig::default()
    }
}

// ---------------------------------------------------------------------------
// Empty and minimal inputs
// ---------------------------------------------------------------------------

#[test]
fn empty_source() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"", &rust_config())?;
    // tree-sitter produces a root node even for empty input
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn single_byte() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"x", &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn single_newline() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"\n", &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn only_whitespace() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"   \t\n\n  \t  ", &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Malformed code (tree-sitter produces ERROR nodes)
// ---------------------------------------------------------------------------

#[test]
fn unmatched_braces() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"fn main() {{{{{", &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn random_keywords() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"fn fn fn let let let if if if", &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn truncated_function() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"fn main(a: i32, b:", &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn only_punctuation() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"(){}[];,.<>+-*/=!@#$%^&", &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Large / deep / wide structures
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_blocks() {
    // 200 levels of nested blocks — may exceed serde_json recursion limit.
    // The important thing: no panics. Either Ok or Err is acceptable.
    let mut src = String::new();
    src.push_str("fn main() ");
    for _ in 0..200 {
        src.push_str("{ if true ");
    }
    for _ in 0..200 {
        src.push('}');
    }
    src.push('}');
    let result = parse_source(src.as_bytes(), &rust_config());
    // Either succeeds or returns a clean error — never panics
    match result {
        Ok(doc) => {
            assert!(doc.metadata().total_nodes > 0);
            assert!(doc.metadata().max_depth > 10);
        }
        Err(_) => {
            // Recursion limit exceeded — acceptable for extreme nesting
        }
    }
}

#[test]
fn wide_function_parameters() -> Result<(), Box<dyn std::error::Error>> {
    // Function with 500 parameters
    let mut src = String::from("fn wide(");
    for i in 0..500 {
        if i > 0 {
            src.push_str(", ");
        }
        src.push_str(&format!("p{i}: i32"));
    }
    src.push_str(") {}");
    let doc = parse_source(src.as_bytes(), &rust_config())?;
    assert!(doc.metadata().total_nodes > 100);
    Ok(())
}

#[test]
fn many_small_functions() -> Result<(), Box<dyn std::error::Error>> {
    // 1000 tiny functions
    let mut src = String::new();
    for i in 0..1000 {
        src.push_str(&format!("fn f{i}() {{}}\n"));
    }
    let doc = parse_source(src.as_bytes(), &rust_config())?;
    assert!(doc.metadata().total_nodes > 1000);
    Ok(())
}

#[test]
fn long_string_literal() -> Result<(), Box<dyn std::error::Error>> {
    // Function containing a 100KB string literal
    let mut src = String::from("fn main() { let s = \"");
    for _ in 0..100_000 {
        src.push('a');
    }
    src.push_str("\"; }");
    let doc = parse_source(src.as_bytes(), &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Unicode edge cases
// ---------------------------------------------------------------------------

#[test]
fn unicode_identifiers() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source("fn 你好() {}".as_bytes(), &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn emoji_in_string() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(
        r#"fn main() { let s = "🦀🔥💯"; }"#.as_bytes(),
        &rust_config(),
    )?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn null_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let src = b"fn main\x00() {}";
    let doc = parse_source(src, &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn mixed_line_endings() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_source(b"fn a() {}\r\nfn b() {}\rfn c() {}\n", &rust_config())?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Config edge cases
// ---------------------------------------------------------------------------

#[test]
fn all_options_enabled() -> Result<(), Box<dyn std::error::Error>> {
    let config = SourceConfig {
        language: Some(SourceLanguage::Rust),
        include_anonymous: true,
        include_spans: true,
        include_text: true,
        max_file_size: 10 * 1024 * 1024,
    };
    let doc = parse_source(b"fn main() { let x = 42; }", &config)?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn text_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let config = SourceConfig {
        language: Some(SourceLanguage::Rust),
        include_text: false,
        ..SourceConfig::default()
    };
    let doc = parse_source(b"fn main() {}", &config)?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
fn no_language_specified() {
    let config = SourceConfig::default();
    let result = parse_source(b"fn main() {}", &config);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Cross-language chaos (all default languages)
// ---------------------------------------------------------------------------

#[test]
#[cfg(feature = "python")]
fn python_malformed() -> Result<(), Box<dyn std::error::Error>> {
    let config = SourceConfig {
        language: Some(SourceLanguage::Python),
        ..SourceConfig::default()
    };
    let doc = parse_source(b"def def def class class", &config)?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
#[cfg(feature = "go")]
fn go_malformed() -> Result<(), Box<dyn std::error::Error>> {
    let config = SourceConfig {
        language: Some(SourceLanguage::Go),
        ..SourceConfig::default()
    };
    let doc = parse_source(b"package func func func {{{", &config)?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

#[test]
#[cfg(feature = "javascript")]
fn javascript_malformed() -> Result<(), Box<dyn std::error::Error>> {
    let config = SourceConfig {
        language: Some(SourceLanguage::JavaScript),
        ..SourceConfig::default()
    };
    let doc = parse_source(b"function function var var {{{", &config)?;
    assert!(doc.metadata().total_nodes > 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Determinism under chaos
// ---------------------------------------------------------------------------

#[test]
fn malformed_code_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let src = b"fn {{{ let = if while ) ; ...";
    let config = rust_config();
    let mut outputs = Vec::new();
    for _ in 0..10 {
        let doc = parse_source(src, &config)?;
        let json = serde_json::to_string(&doc.metadata())?;
        outputs.push(json);
    }
    for (i, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            &outputs[0], output,
            "malformed code: run 0 and run {} differ",
            i
        );
    }
    Ok(())
}
