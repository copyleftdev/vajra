//! Markdown → structured JSON → [`Document`] adapter.
//!
//! Converts Markdown input into a hierarchical JSON representation that
//! preserves document structure (headings → sections, code blocks, lists,
//! tables, links, images) and then feeds the JSON through the standard
//! parse pipeline to produce a [`Document`] with path trie and metadata.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_json::{json, Map, Value};
use vajra_types::{Document, VajraError};

use crate::parse::parse_str;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse Markdown content into a structured JSON [`Document`].
///
/// The resulting JSON has the shape:
///
/// ```json
/// {
///   "type": "document",
///   "metadata": { "heading_count": …, "paragraph_count": …, … },
///   "sections": [ { "heading": "…", "level": 1, "content": […], "subsections": […] } ]
/// }
/// ```
///
/// # Errors
///
/// Returns [`VajraError::Parse`] if internal JSON conversion fails (should
/// not happen on well-formed Markdown, but we propagate just in case).
pub fn parse_markdown(input: &str) -> Result<Document, VajraError> {
    let tree = build_section_tree(input);
    let json_value = tree_to_json(&tree);
    let json_text = serde_json::to_string(&json_value).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("Markdown->JSON serialization: {e}"),
        source_path: None,
    })?;
    parse_str(&json_text)
}

// ---------------------------------------------------------------------------
// Internal IR (intermediate representation)
// ---------------------------------------------------------------------------

/// A section of the document, introduced by a heading.
#[derive(Debug)]
struct Section {
    heading: String,
    level: u32,
    content: Vec<ContentItem>,
    subsections: Vec<Self>,
}

/// A single content item within a section.
#[derive(Debug)]
enum ContentItem {
    Paragraph(String),
    CodeBlock {
        language: String,
        code: String,
    },
    List {
        ordered: bool,
        items: Vec<String>,
    },
    Link {
        text: String,
        url: String,
    },
    Image {
        alt: String,
        url: String,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

/// Metadata counters gathered during parsing.
#[derive(Debug, Default)]
#[allow(clippy::struct_field_names)]
struct MdMetadata {
    heading_count: u32,
    paragraph_count: u32,
    code_block_count: u32,
    link_count: u32,
    list_count: u32,
    table_count: u32,
    image_count: u32,
    word_count: u64,
}

/// Top-level document tree.
#[derive(Debug)]
struct DocTree {
    sections: Vec<Section>,
    meta: MdMetadata,
}

// ---------------------------------------------------------------------------
// Tree builder – iterate pulldown-cmark events
// ---------------------------------------------------------------------------

/// State machine context for the event-driven builder.
#[allow(clippy::struct_excessive_bools)]
struct Builder {
    /// Stack of (`heading_level`, section).  Level 0 = synthetic root.
    stack: Vec<(u32, Section)>,
    meta: MdMetadata,

    // transient accumulators
    in_heading: bool,
    heading_level: u32,
    heading_text: String,

    in_paragraph: bool,
    para_text: String,

    in_code_block: bool,
    code_lang: String,
    code_text: String,

    in_list: bool,
    list_ordered: bool,
    list_items: Vec<String>,
    in_item: bool,
    item_text: String,

    in_link: bool,
    link_url: String,
    link_text: String,

    in_image: bool,
    image_url: String,
    image_alt: String,

    in_table: bool,
    in_table_head: bool,
    in_table_row: bool,
    in_table_cell: bool,
    table_headers: Vec<String>,
    table_rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    cell_text: String,
}

impl Builder {
    fn new() -> Self {
        // Start with a synthetic root section at level 0.
        let root = Section {
            heading: String::new(),
            level: 0,
            content: Vec::new(),
            subsections: Vec::new(),
        };
        Self {
            stack: vec![(0, root)],
            meta: MdMetadata::default(),

            in_heading: false,
            heading_level: 0,
            heading_text: String::new(),

            in_paragraph: false,
            para_text: String::new(),

            in_code_block: false,
            code_lang: String::new(),
            code_text: String::new(),

            in_list: false,
            list_ordered: false,
            list_items: Vec::new(),
            in_item: false,
            item_text: String::new(),

            in_link: false,
            link_url: String::new(),
            link_text: String::new(),

            in_image: false,
            image_url: String::new(),
            image_alt: String::new(),

            in_table: false,
            in_table_head: false,
            in_table_row: false,
            in_table_cell: false,
            table_headers: Vec::new(),
            table_rows: Vec::new(),
            current_row: Vec::new(),
            cell_text: String::new(),
        }
    }

    /// Push accumulated text to the right target.
    fn accumulate_text(&mut self, text: &str) {
        if self.in_heading {
            self.heading_text.push_str(text);
        } else if self.in_code_block {
            self.code_text.push_str(text);
        } else if self.in_table_cell {
            self.cell_text.push_str(text);
        } else if self.in_image {
            self.image_alt.push_str(text);
        } else if self.in_link {
            self.link_text.push_str(text);
        } else if self.in_item {
            self.item_text.push_str(text);
        } else if self.in_paragraph {
            self.para_text.push_str(text);
        }
    }

    /// Return the *current* section (top of the stack) mutably.
    fn current_section_mut(&mut self) -> &mut Section {
        // The stack is initialized with a root section and never popped
        // below one element, so `last_mut()` always succeeds.
        match self.stack.last_mut() {
            Some((_, section)) => section,
            None => unreachable!("builder stack is never empty"),
        }
    }

    /// Open a new heading section: pop sections from the stack until we find
    /// one with a *strictly lower* level, then push the new section.
    fn open_heading_section(&mut self, heading: String, level: u32) {
        // Pop sections whose level >= this heading's level, attaching each
        // as a subsection of the one below it.
        while self.stack.len() > 1 {
            let top_level = self.stack.last().map_or(0, |(l, _)| *l);
            if top_level < level {
                break;
            }
            // Pop and attach as subsection.
            if let Some((_, child)) = self.stack.pop() {
                if let Some((_, parent)) = self.stack.last_mut() {
                    parent.subsections.push(child);
                }
            }
        }

        let section = Section {
            heading,
            level,
            content: Vec::new(),
            subsections: Vec::new(),
        };
        self.stack.push((level, section));
    }

    /// Collapse the entire stack into a single root and return it.
    fn finish(mut self) -> DocTree {
        // Pop everything back to root.
        while self.stack.len() > 1 {
            if let Some((_, child)) = self.stack.pop() {
                if let Some((_, parent)) = self.stack.last_mut() {
                    parent.subsections.push(child);
                }
            }
        }

        let root = self.stack.pop().map_or_else(
            || Section {
                heading: String::new(),
                level: 0,
                content: Vec::new(),
                subsections: Vec::new(),
            },
            |(_, s)| s,
        );

        // Root's content goes into a root section; subsections become
        // top-level sections.
        let mut sections = Vec::new();
        if !root.content.is_empty() {
            // Content before the first heading → anonymous root section.
            sections.push(Section {
                heading: String::new(),
                level: 0,
                content: root.content,
                subsections: Vec::new(),
            });
        }
        sections.extend(root.subsections);

        DocTree {
            sections,
            meta: self.meta,
        }
    }

    fn push_content(&mut self, item: ContentItem) {
        self.current_section_mut().content.push(item);
    }
}

const fn heading_level_to_u32(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn count_words(text: &str) -> u64 {
    text.split_whitespace().count() as u64
}

#[allow(clippy::too_many_lines)]
fn build_section_tree(input: &str) -> DocTree {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(input, opts);
    let mut b = Builder::new();

    for event in parser {
        match event {
            // ---- Headings ----
            Event::Start(Tag::Heading { level, .. }) => {
                b.in_heading = true;
                b.heading_level = heading_level_to_u32(level);
                b.heading_text.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                b.in_heading = false;
                b.meta.heading_count += 1;
                let heading = std::mem::take(&mut b.heading_text);
                let level = b.heading_level;
                b.open_heading_section(heading, level);
            }

            // ---- Paragraphs ----
            Event::Start(Tag::Paragraph) => {
                b.in_paragraph = true;
                b.para_text.clear();
            }
            Event::End(TagEnd::Paragraph) => {
                b.in_paragraph = false;
                let text = std::mem::take(&mut b.para_text);
                if !text.is_empty() {
                    b.meta.paragraph_count += 1;
                    b.meta.word_count += count_words(&text);
                    b.push_content(ContentItem::Paragraph(text));
                }
            }

            // ---- Code blocks ----
            Event::Start(Tag::CodeBlock(kind)) => {
                b.in_code_block = true;
                b.code_text.clear();
                b.code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                b.in_code_block = false;
                b.meta.code_block_count += 1;
                let language = std::mem::take(&mut b.code_lang);
                let code = std::mem::take(&mut b.code_text);
                b.meta.word_count += count_words(&code);
                b.push_content(ContentItem::CodeBlock { language, code });
            }

            // ---- Lists ----
            Event::Start(Tag::List(first)) => {
                b.in_list = true;
                b.list_ordered = first.is_some();
                b.list_items.clear();
            }
            Event::End(TagEnd::List(_)) => {
                b.in_list = false;
                b.meta.list_count += 1;
                let ordered = b.list_ordered;
                let items = std::mem::take(&mut b.list_items);
                for item in &items {
                    b.meta.word_count += count_words(item);
                }
                b.push_content(ContentItem::List { ordered, items });
            }
            Event::Start(Tag::Item) => {
                b.in_item = true;
                b.item_text.clear();
            }
            Event::End(TagEnd::Item) => {
                b.in_item = false;
                let text = std::mem::take(&mut b.item_text);
                b.list_items.push(text);
            }

            // ---- Links ----
            Event::Start(Tag::Link { dest_url, .. }) => {
                b.in_link = true;
                b.link_url = dest_url.to_string();
                b.link_text.clear();
            }
            Event::End(TagEnd::Link) => {
                b.in_link = false;
                b.meta.link_count += 1;
                let text = std::mem::take(&mut b.link_text);
                let url = std::mem::take(&mut b.link_url);
                // Also accumulate link text into the containing paragraph.
                if b.in_paragraph {
                    b.para_text.push_str(&text);
                } else if b.in_item {
                    b.item_text.push_str(&text);
                }
                b.push_content(ContentItem::Link { text, url });
            }

            // ---- Images ----
            Event::Start(Tag::Image { dest_url, .. }) => {
                b.in_image = true;
                b.image_url = dest_url.to_string();
                b.image_alt.clear();
            }
            Event::End(TagEnd::Image) => {
                b.in_image = false;
                b.meta.image_count += 1;
                let alt = std::mem::take(&mut b.image_alt);
                let url = std::mem::take(&mut b.image_url);
                b.push_content(ContentItem::Image { alt, url });
            }

            // ---- Tables ----
            Event::Start(Tag::Table(_)) => {
                b.in_table = true;
                b.table_headers.clear();
                b.table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                b.in_table = false;
                b.meta.table_count += 1;
                let headers = std::mem::take(&mut b.table_headers);
                let rows = std::mem::take(&mut b.table_rows);
                for h in &headers {
                    b.meta.word_count += count_words(h);
                }
                for row in &rows {
                    for cell in row {
                        b.meta.word_count += count_words(cell);
                    }
                }
                b.push_content(ContentItem::Table { headers, rows });
            }
            Event::Start(Tag::TableHead) => {
                b.in_table_head = true;
                b.current_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                b.in_table_head = false;
                // TableHead directly contains TableCells (no TableRow wrapper).
                let row = std::mem::take(&mut b.current_row);
                b.table_headers = row;
            }
            Event::Start(Tag::TableRow) => {
                b.in_table_row = true;
                b.current_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                b.in_table_row = false;
                let row = std::mem::take(&mut b.current_row);
                b.table_rows.push(row);
            }
            Event::Start(Tag::TableCell) => {
                b.in_table_cell = true;
                b.cell_text.clear();
            }
            Event::End(TagEnd::TableCell) => {
                b.in_table_cell = false;
                let cell = std::mem::take(&mut b.cell_text);
                b.current_row.push(cell);
            }

            // ---- Text events ----
            Event::Text(text) => {
                b.accumulate_text(&text);
            }
            Event::Code(code) => {
                // Inline code – treat as text.
                b.accumulate_text(&code);
            }
            Event::SoftBreak | Event::HardBreak => {
                b.accumulate_text(" ");
            }

            // Everything else we ignore (HTML blocks, footnotes, etc.).
            _ => {}
        }
    }

    b.finish()
}

// ---------------------------------------------------------------------------
// IR → serde_json::Value
// ---------------------------------------------------------------------------

fn content_item_to_json(item: &ContentItem) -> Value {
    match item {
        ContentItem::Paragraph(text) => json!({
            "type": "paragraph",
            "text": text,
        }),
        ContentItem::CodeBlock { language, code } => json!({
            "type": "code_block",
            "language": language,
            "code": code,
        }),
        ContentItem::List { ordered, items } => json!({
            "type": "list",
            "ordered": ordered,
            "items": items,
        }),
        ContentItem::Link { text, url } => json!({
            "type": "link",
            "text": text,
            "url": url,
        }),
        ContentItem::Image { alt, url } => json!({
            "type": "image",
            "alt": alt,
            "url": url,
        }),
        ContentItem::Table { headers, rows } => json!({
            "type": "table",
            "headers": headers,
            "rows": rows,
        }),
    }
}

fn section_to_json(section: &Section) -> Value {
    let content: Vec<Value> = section.content.iter().map(content_item_to_json).collect();
    let subsections: Vec<Value> = section.subsections.iter().map(section_to_json).collect();

    let mut obj = Map::new();
    obj.insert("heading".into(), Value::String(section.heading.clone()));
    obj.insert("level".into(), json!(section.level));
    obj.insert("content".into(), Value::Array(content));
    if !subsections.is_empty() {
        obj.insert("subsections".into(), Value::Array(subsections));
    }
    Value::Object(obj)
}

fn tree_to_json(tree: &DocTree) -> Value {
    let sections: Vec<Value> = tree.sections.iter().map(section_to_json).collect();

    json!({
        "type": "document",
        "metadata": {
            "heading_count": tree.meta.heading_count,
            "paragraph_count": tree.meta.paragraph_count,
            "code_block_count": tree.meta.code_block_count,
            "link_count": tree.meta.link_count,
            "list_count": tree.meta.list_count,
            "table_count": tree.meta.table_count,
            "image_count": tree.meta.image_count,
            "word_count": tree.meta.word_count,
        },
        "sections": sections,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: parse markdown and return the raw JSON value.
    fn md_json(input: &str) -> Value {
        let doc = parse_markdown(input).unwrap_or_else(|e| panic!("parse_markdown failed: {e}"));
        doc.value().clone()
    }

    // Helper: get metadata sub-object.
    fn md_meta(v: &Value) -> &Value {
        &v["metadata"]
    }

    // ---- 1. Simple heading + paragraph ----
    #[test]
    fn simple_heading_paragraph() {
        let v = md_json("# Hello\n\nWorld.\n");
        assert_eq!(v["type"], "document");
        let sections = v["sections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sections"));
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0]["heading"], "Hello");
        assert_eq!(sections[0]["level"], 1);
        let content = sections[0]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content"));
        assert_eq!(content[0]["type"], "paragraph");
        assert_eq!(content[0]["text"], "World.");
    }

    // ---- 2. Nested headings h1 > h2 > h3 ----
    #[test]
    fn nested_headings_hierarchy() {
        let input = "# Top\n\n## Mid\n\n### Deep\n\nText here.\n";
        let v = md_json(input);
        let sections = v["sections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sections"));
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0]["heading"], "Top");
        let sub1 = sections[0]["subsections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sub"));
        assert_eq!(sub1.len(), 1);
        assert_eq!(sub1[0]["heading"], "Mid");
        let sub2 = sub1[0]["subsections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sub2"));
        assert_eq!(sub2.len(), 1);
        assert_eq!(sub2[0]["heading"], "Deep");
    }

    // ---- 3. Code block with language ----
    #[test]
    fn code_block_with_language() {
        let input = "# Code\n\n```rust\nfn main() {}\n```\n";
        let v = md_json(input);
        let content = v["sections"][0]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content"));
        assert_eq!(content[0]["type"], "code_block");
        assert_eq!(content[0]["language"], "rust");
        assert_eq!(content[0]["code"], "fn main() {}\n");
    }

    // ---- 4. Unordered list ----
    #[test]
    fn unordered_list() {
        let input = "- apple\n- banana\n- cherry\n";
        let v = md_json(input);
        // Content before first heading → root section.
        let sections = v["sections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sections"));
        let content = sections[0]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content"));
        let list = &content[0];
        assert_eq!(list["type"], "list");
        assert_eq!(list["ordered"], false);
        let items = list["items"]
            .as_array()
            .unwrap_or_else(|| panic!("no items"));
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], "apple");
    }

    // ---- 5. Ordered list ----
    #[test]
    fn ordered_list() {
        let input = "1. first\n2. second\n3. third\n";
        let v = md_json(input);
        let content = v["sections"][0]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content"));
        let list = &content[0];
        assert_eq!(list["type"], "list");
        assert_eq!(list["ordered"], true);
        let items = list["items"]
            .as_array()
            .unwrap_or_else(|| panic!("no items"));
        assert_eq!(items.len(), 3);
    }

    // ---- 6. Link ----
    #[test]
    fn link_captured() {
        let input = "# Links\n\n[click here](https://example.com)\n";
        let v = md_json(input);
        let content = v["sections"][0]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content"));
        // The link is inside a paragraph, but also emitted as its own content item.
        let has_link = content.iter().any(|c| c["type"] == "link");
        assert!(has_link, "expected a link content item");
        let link = content
            .iter()
            .find(|c| c["type"] == "link")
            .unwrap_or_else(|| panic!("no link"));
        assert_eq!(link["text"], "click here");
        assert_eq!(link["url"], "https://example.com");
    }

    // ---- 7. Table ----
    #[test]
    fn table_parsed() {
        let input = "| Col A | Col B |\n|-------|-------|\n| 1 | 2 |\n| 3 | 4 |\n";
        let v = md_json(input);
        let content = v["sections"][0]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content"));
        let table = content
            .iter()
            .find(|c| c["type"] == "table")
            .unwrap_or_else(|| panic!("no table"));
        let headers = table["headers"]
            .as_array()
            .unwrap_or_else(|| panic!("no headers"));
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], "Col A");
        assert_eq!(headers[1], "Col B");
        let rows = table["rows"]
            .as_array()
            .unwrap_or_else(|| panic!("no rows"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], "1");
        assert_eq!(rows[1][1], "4");
    }

    // ---- 8. Mixed content ----
    #[test]
    fn mixed_content() {
        let input = r#"# Intro

Some text here.

```python
print("hi")
```

- item a
- item b
"#;
        let v = md_json(input);
        let content = v["sections"][0]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content"));
        let types: Vec<&str> = content.iter().filter_map(|c| c["type"].as_str()).collect();
        assert!(types.contains(&"paragraph"), "missing paragraph");
        assert!(types.contains(&"code_block"), "missing code_block");
        assert!(types.contains(&"list"), "missing list");
    }

    // ---- 9. Empty markdown ----
    #[test]
    fn empty_markdown() {
        let v = md_json("");
        assert_eq!(v["type"], "document");
        let sections = v["sections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sections"));
        assert!(sections.is_empty());
    }

    // ---- 10. No headings ----
    #[test]
    fn no_headings_root_section() {
        let v = md_json("Just some text.\n\nMore text.\n");
        let sections = v["sections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sections"));
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0]["heading"], "");
        assert_eq!(sections[0]["level"], 0);
    }

    // ---- 11. Multiple h1 sections ----
    #[test]
    fn multiple_h1_sections() {
        let input = "# First\n\nText 1.\n\n# Second\n\nText 2.\n\n# Third\n\nText 3.\n";
        let v = md_json(input);
        let sections = v["sections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sections"));
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0]["heading"], "First");
        assert_eq!(sections[1]["heading"], "Second");
        assert_eq!(sections[2]["heading"], "Third");
    }

    // ---- 12. Image reference ----
    #[test]
    fn image_captured() {
        let input = "![alt text](https://example.com/img.png)\n";
        let v = md_json(input);
        let content = v["sections"][0]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content"));
        let img = content
            .iter()
            .find(|c| c["type"] == "image")
            .unwrap_or_else(|| panic!("no image"));
        assert_eq!(img["alt"], "alt text");
        assert_eq!(img["url"], "https://example.com/img.png");
    }

    // ---- 13. Metadata counts ----
    #[test]
    fn metadata_counts_correct() {
        let input = r#"# H1

Paragraph one.

Paragraph two.

## H2

```rust
code
```

- a
- b

[link](http://x.com)

| A | B |
|---|---|
| 1 | 2 |

![img](http://i.png)
"#;
        let v = md_json(input);
        let m = md_meta(&v);
        assert_eq!(m["heading_count"], 2);
        // "Paragraph one.", "Paragraph two.", and the paragraph wrapping the link.
        assert_eq!(m["paragraph_count"], 3);
        assert_eq!(m["code_block_count"], 1);
        // link_count: the inline link produces 1
        assert!(m["link_count"].as_u64().unwrap_or(0) >= 1);
        assert_eq!(m["list_count"], 1);
        assert_eq!(m["table_count"], 1);
        assert_eq!(m["image_count"], 1);
    }

    // ---- 14. Full analysis pipeline works on markdown-derived JSON ----
    #[test]
    fn pipeline_produces_valid_document() {
        let input = "# Test\n\nHello world.\n";
        let doc = parse_markdown(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        // Should have a valid trie and metadata.
        assert!(doc.metadata().total_nodes > 0);
        assert!(doc.metadata().distinct_paths > 0);
        assert!(doc.trie().path_count() > 0);
    }

    // ---- 15. Word count is reasonable ----
    #[test]
    fn word_count_reasonable() {
        let input = "# Title\n\nOne two three four five.\n";
        let v = md_json(input);
        let wc = v["metadata"]["word_count"].as_u64().unwrap_or(0);
        assert!(wc >= 5, "expected at least 5 words, got {wc}");
    }

    // ---- 16. Determinism: same markdown → identical Document ----
    #[test]
    fn deterministic_output() {
        let input = "# Hello\n\nWorld.\n\n## Sub\n\n- a\n- b\n";
        let reference = parse_markdown(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        for _ in 0..10 {
            let doc = parse_markdown(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
            assert_eq!(doc.value(), reference.value());
        }
    }

    // ---- 17. Same markdown → identical fingerprint ----
    #[test]
    fn deterministic_fingerprint() {
        use vajra_fingerprint::FingerprintAnalyzer;
        use vajra_types::Analyzer;
        let input = "# Hello\n\nSome content here.\n\n## Details\n\n- one\n- two\n";
        let doc1 = parse_markdown(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let doc2 = parse_markdown(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let fp1 = FingerprintAnalyzer
            .analyze(&doc1)
            .unwrap_or_else(|e| panic!("fp1 failed: {e}"));
        let fp2 = FingerprintAnalyzer
            .analyze(&doc2)
            .unwrap_or_else(|e| panic!("fp2 failed: {e}"));
        assert_eq!(fp1.path_set, fp2.path_set);
        assert_eq!(fp1.typed_path, fp2.typed_path);
        assert_eq!(fp1.shape, fp2.shape);
    }

    // ---- 18. Very large markdown ----
    #[test]
    fn large_markdown_10k_lines() {
        let mut buf = String::from("# Big Document\n\n");
        for i in 0..10_000 {
            buf.push_str(&format!("Line {i} of the document with some words.\n\n"));
        }
        let doc = parse_markdown(&buf).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert!(doc.metadata().total_nodes > 100);
    }

    // ---- 19. Deeply nested headings h1–h6 repeated ----
    #[test]
    fn deeply_nested_headings() {
        let mut buf = String::new();
        for cycle in 0..10 {
            for level in 1..=6 {
                let hashes = "#".repeat(level);
                buf.push_str(&format!(
                    "{hashes} Heading L{level} C{cycle}\n\nSome text.\n\n"
                ));
            }
        }
        let doc = parse_markdown(&buf).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert!(doc.metadata().total_nodes > 0);
    }

    // ---- 20. Unicode content ----
    #[test]
    fn unicode_content() {
        let input = "# 日本語タイトル\n\nEmoji: 🦀🚀\n\nRTL: مرحبا\n\nCJK: 你好世界\n";
        let v = md_json(input);
        let sections = v["sections"]
            .as_array()
            .unwrap_or_else(|| panic!("no sections"));
        assert_eq!(sections[0]["heading"], "日本語タイトル");
    }

    // ---- 21. Only headings, no content ----
    #[test]
    fn headings_only() {
        let input = "# A\n\n## B\n\n### C\n";
        let doc = parse_markdown(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert!(doc.metadata().total_nodes > 0);
        let v = doc.value();
        assert_eq!(v["metadata"]["heading_count"], 3);
    }

    // ---- 22. Only code blocks ----
    #[test]
    fn only_code_blocks() {
        let input = "```\nline 1\nline 2\n```\n\n```python\nprint(42)\n```\n";
        let v = md_json(input);
        assert_eq!(v["metadata"]["code_block_count"], 2);
    }

    // ---- 23. Stats pipeline on markdown doc ----
    #[test]
    fn stats_pipeline_on_markdown() {
        use vajra_stats::StatsAnalyzer;
        use vajra_types::Analyzer;
        let input = "# Stats Test\n\nHello world one two three.\n\n## Numbers\n\nFoo bar baz.\n";
        let doc = parse_markdown(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stats = StatsAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("stats failed: {e}"));
        // Should produce some path stats (at least for root).
        assert!(!stats.paths.is_empty());
    }

    // ---- 24. Anomaly detection on markdown doc ----
    #[test]
    fn anomaly_pipeline_on_markdown() {
        use vajra_anomaly::AnomalyAnalyzer;
        use vajra_types::Analyzer;
        let input = "# Anomaly Test\n\nSome content.\n\n## Sub\n\nMore content.\n";
        let doc = parse_markdown(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        // Should not panic; anomalies may or may not be found.
        let _anomalies = AnomalyAnalyzer::default().analyze(&doc);
    }
}
