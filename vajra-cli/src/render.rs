//! A minimal document model for command output.
//!
//! Commands historically formatted their own output with `println!` per format,
//! which is why `--format markdown` and `--format compact-ai` silently fell
//! through to text for eleven of twelve commands: each command had to implement
//! every format, so most implemented one.
//!
//! A command instead builds a [`Report`] — headings, key/value pairs, tables,
//! notes — and the renderer emits it per format. Adding a format then costs one
//! implementation here rather than twelve at the call sites.
//!
//! Deliberately small. This is not a layout engine: it covers the shapes vajra
//! commands actually produce, which after surveying them is flat key/value
//! summaries, tables with a caveat attached, and nested detail lists.

use std::fmt::Write as _;

/// One element of a report.
#[derive(Debug, Clone)]
pub enum Block {
    /// A section heading.
    Heading(String),
    /// Aligned label/value pairs — a summary.
    Fields(Vec<(String, String)>),
    /// A table with a header row.
    Table(Table),
    /// Prose qualifying what precedes it. Never omitted: these carry the
    /// caveats that stop a number being over-read.
    Note(String),
    /// A nested list under a lead line, for per-item detail.
    Nested(Vec<(String, Vec<String>)>),
}

/// A table with a header row and body rows.
#[derive(Debug, Clone)]
pub struct Table {
    /// Column headers.
    pub headers: Vec<String>,
    /// Body rows. Short rows are padded, long rows are not truncated.
    pub rows: Vec<Vec<String>>,
    /// Shown when `rows` is empty, instead of an empty table.
    pub empty: String,
}

impl Table {
    /// A table with the given headers and an empty-state message.
    pub fn new(headers: &[&str], empty: &str) -> Self {
        Self {
            headers: headers.iter().map(|h| (*h).to_owned()).collect(),
            rows: Vec::new(),
            empty: empty.to_owned(),
        }
    }

    /// Append a row.
    pub fn push(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    /// Column widths for aligned text output.
    fn widths(&self) -> Vec<usize> {
        let mut w: Vec<usize> = self.headers.iter().map(String::len).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i >= w.len() {
                    w.push(cell.len());
                } else if cell.len() > w[i] {
                    w[i] = cell.len();
                }
            }
        }
        w
    }
}

/// A command's output, independent of format.
#[derive(Debug, Clone, Default)]
pub struct Report {
    blocks: Vec<Block>,
}

impl Report {
    /// An empty report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a heading.
    pub fn heading(&mut self, text: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Heading(text.into()));
        self
    }

    /// Add label/value pairs.
    pub fn fields(&mut self, pairs: Vec<(String, String)>) -> &mut Self {
        self.blocks.push(Block::Fields(pairs));
        self
    }

    /// Add a table.
    pub fn table(&mut self, table: Table) -> &mut Self {
        self.blocks.push(Block::Table(table));
        self
    }

    /// Add a qualifying note.
    pub fn note(&mut self, text: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Note(text.into()));
        self
    }

    /// Add a nested detail list.
    pub fn nested(&mut self, items: Vec<(String, Vec<String>)>) -> &mut Self {
        self.blocks.push(Block::Nested(items));
        self
    }

    /// Render as aligned plain text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let mut prev_was_heading = false;
        for (i, block) in self.blocks.iter().enumerate() {
            // A blank line separates blocks, but not a heading from what it
            // introduces.
            if i > 0 && !prev_was_heading {
                out.push('\n');
            }
            prev_was_heading = matches!(block, Block::Heading(_));
            match block {
                Block::Heading(h) => {
                    let _ = writeln!(out, "=== {h} ===");
                }
                Block::Fields(pairs) => {
                    // A trailing colon on the label, matching the convention the
                    // hand-written output used, so anything grepping text output
                    // keeps working.
                    let w = pairs.iter().map(|(k, _)| k.len() + 1).max().unwrap_or(0);
                    for (k, v) in pairs {
                        let label = format!("{k}:");
                        let _ = writeln!(out, "  {label:<w$}  {v}", w = w);
                    }
                }
                Block::Table(t) => {
                    if t.rows.is_empty() {
                        let _ = writeln!(out, "  ({})", t.empty);
                    } else {
                        let w = t.widths();
                        let header: Vec<String> = t
                            .headers
                            .iter()
                            .enumerate()
                            .map(|(i, h)| format!("{h:<width$}", width = w[i]))
                            .collect();
                        let _ = writeln!(out, "  {}", header.join("  ").trim_end());
                        for row in &t.rows {
                            let cells: Vec<String> = row
                                .iter()
                                .enumerate()
                                .map(|(i, c)| {
                                    format!("{c:<width$}", width = w.get(i).copied().unwrap_or(0))
                                })
                                .collect();
                            let _ = writeln!(out, "  {}", cells.join("  ").trim_end());
                        }
                    }
                }
                Block::Note(n) => {
                    for line in n.lines() {
                        let _ = writeln!(out, "  {line}");
                    }
                }
                Block::Nested(items) => {
                    for (lead, children) in items {
                        let _ = writeln!(out, "  {lead}");
                        for child in children {
                            let _ = writeln!(out, "      {child}");
                        }
                    }
                }
            }
        }
        out
    }

    /// Render as GitHub-flavoured Markdown.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let mut prev_was_heading = false;
        for (i, block) in self.blocks.iter().enumerate() {
            // A blank line separates blocks, but not a heading from what it
            // introduces.
            if i > 0 && !prev_was_heading {
                out.push('\n');
            }
            prev_was_heading = matches!(block, Block::Heading(_));
            match block {
                Block::Heading(h) => {
                    let _ = writeln!(out, "## {h}");
                }
                Block::Fields(pairs) => {
                    let _ = writeln!(out, "| Field | Value |");
                    let _ = writeln!(out, "|---|---|");
                    for (k, v) in pairs {
                        let _ = writeln!(out, "| {} | {} |", escape(k), escape(v));
                    }
                }
                Block::Table(t) => {
                    if t.rows.is_empty() {
                        let _ = writeln!(out, "_{}_", t.empty);
                    } else {
                        let _ = writeln!(
                            out,
                            "| {} |",
                            t.headers
                                .iter()
                                .map(|h| escape(h))
                                .collect::<Vec<_>>()
                                .join(" | ")
                        );
                        let _ = writeln!(out, "|{}", "---|".repeat(t.headers.len().max(1)));
                        for row in &t.rows {
                            let _ = writeln!(
                                out,
                                "| {} |",
                                row.iter()
                                    .map(|c| escape(c))
                                    .collect::<Vec<_>>()
                                    .join(" | ")
                            );
                        }
                    }
                }
                Block::Note(n) => {
                    for line in n.lines() {
                        let _ = writeln!(out, "> {line}");
                    }
                }
                Block::Nested(items) => {
                    for (lead, children) in items {
                        let _ = writeln!(out, "- {}", escape(lead));
                        for child in children {
                            let _ = writeln!(out, "  - {}", escape(child));
                        }
                    }
                }
            }
        }
        out
    }
}

/// Escape the characters that would break a Markdown table cell.
fn escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Report {
        let mut r = Report::new();
        r.heading("Summary");
        r.fields(vec![
            ("Records".to_owned(), "288".to_owned()),
            ("Entropy".to_owned(), "0.8113".to_owned()),
        ]);
        let mut t = Table::new(&["PATH", "ENTROPY"], "no paths");
        t.push(vec!["$.a".to_owned(), "1.0000".to_owned()]);
        t.push(vec!["$.b".to_owned(), "0.5000".to_owned()]);
        r.table(t);
        r.note("Ranked by entropy.");
        r
    }

    #[test]
    fn text_render_is_aligned_and_complete() {
        let out = sample().to_text();
        assert!(out.contains("=== Summary ==="));
        assert!(out.contains("Records:"), "labels keep their colon:\n{out}");
        assert!(out.contains("$.a"));
        assert!(out.contains("Ranked by entropy."));
    }

    #[test]
    fn markdown_render_produces_real_tables() {
        let out = sample().to_markdown();
        assert!(out.contains("## Summary"), "heading:\n{out}");
        assert!(out.contains("| Field | Value |"), "fields table:\n{out}");
        assert!(out.contains("| PATH | ENTROPY |"), "data table:\n{out}");
        assert!(out.contains("|---|---|"), "separator row:\n{out}");
        assert!(out.contains("> Ranked by entropy."), "blockquote:\n{out}");
    }

    /// The whole point: markdown must not be the text output.
    #[test]
    fn markdown_differs_from_text() {
        let r = sample();
        assert_ne!(r.to_markdown(), r.to_text());
    }

    /// A pipe inside a cell would otherwise break the table.
    #[test]
    fn pipes_are_escaped_in_markdown() {
        let mut r = Report::new();
        let mut t = Table::new(&["RULE"], "none");
        t.push(vec!["a | b".to_owned()]);
        r.table(t);
        let out = r.to_markdown();
        assert!(out.contains("a \\| b"), "pipe must be escaped:\n{out}");
    }

    /// Newlines inside a cell would break row structure.
    #[test]
    fn newlines_are_flattened_in_markdown_cells() {
        let mut r = Report::new();
        let mut t = Table::new(&["X"], "none");
        t.push(vec!["one\ntwo".to_owned()]);
        r.table(t);
        let out = r.to_markdown();
        assert!(out.contains("one two"), "newline flattened:\n{out}");
        // The row must remain a single line.
        let rows: Vec<&str> = out.lines().filter(|l| l.contains("one")).collect();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn empty_table_shows_its_empty_state() {
        let mut r = Report::new();
        r.table(Table::new(&["A", "B"], "nothing found"));
        assert!(r.to_text().contains("(nothing found)"));
        assert!(r.to_markdown().contains("_nothing found_"));
    }

    /// Notes carry the caveats that stop a number being over-read, so they must
    /// survive every format.
    #[test]
    fn notes_survive_both_formats() {
        let mut r = Report::new();
        r.note("Not a verdict.");
        assert!(r.to_text().contains("Not a verdict."));
        assert!(r.to_markdown().contains("Not a verdict."));
    }

    #[test]
    fn nested_lists_render_in_both_formats() {
        let mut r = Report::new();
        r.nested(vec![(
            "cluster of 3".to_owned(),
            vec!["pkg-a".to_owned(), "pkg-b".to_owned()],
        )]);
        let text = r.to_text();
        assert!(text.contains("cluster of 3") && text.contains("pkg-a"));
        let md = r.to_markdown();
        assert!(md.contains("- cluster of 3") && md.contains("  - pkg-a"));
    }

    #[test]
    fn ragged_rows_do_not_panic() {
        let mut r = Report::new();
        let mut t = Table::new(&["A", "B", "C"], "none");
        t.push(vec!["1".to_owned()]);
        t.push(vec![
            "1".to_owned(),
            "2".to_owned(),
            "3".to_owned(),
            "4".to_owned(),
        ]);
        r.table(t);
        let _ = r.to_text();
        let _ = r.to_markdown();
    }

    #[test]
    fn render_is_deterministic() {
        let r = sample();
        assert_eq!(r.to_text(), r.to_text());
        assert_eq!(r.to_markdown(), r.to_markdown());
    }
}
