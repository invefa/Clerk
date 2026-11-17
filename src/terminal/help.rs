//! Width-aware help layout over structured entries, independent of CLI parsing.

use crate::terminal::{reader::Document, text};

#[derive(Clone, Debug)]
pub struct OptionEntry {
    pub name: String,
    pub short: Option<char>,
    pub argument: String,
    pub description: String,
    pub choices: Vec<String>,
    pub default: Option<String>,
    pub group: String,
    pub indent: usize,
}

impl OptionEntry {
    fn label(&self) -> String {
        format!(
            "--{}{}{}",
            self.name,
            if self.argument.is_empty() { "" } else { " " },
            self.argument
        )
    }
}

#[derive(Clone, Debug)]
pub enum Block {
    Text(String),
    Pairs {
        rows: Vec<(String, String)>,
        indent: usize,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Columns {
        rows: Vec<Vec<String>>,
        indent: usize,
    },
    Options(Vec<OptionEntry>),
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub blocks: Vec<Block>,
}

impl Page {
    pub fn text(&mut self, text: impl Into<String>) {
        self.blocks.push(Block::Text(text.into()));
    }
    pub fn pairs(&mut self, rows: Vec<(String, String)>, indent: usize) {
        self.blocks.push(Block::Pairs { rows, indent });
    }
    pub fn table(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        self.blocks.push(Block::Table { headers, rows });
    }
    pub fn columns(&mut self, rows: Vec<Vec<String>>, indent: usize) {
        self.blocks.push(Block::Columns { rows, indent });
    }
    pub fn options(&mut self, rows: Vec<OptionEntry>) {
        self.blocks.push(Block::Options(rows));
    }

    /// Recompute from the page on every viewport change, retaining all content.
    pub fn render(&self, width: Option<usize>, compact: bool) -> Document {
        let width = width.unwrap_or(usize::MAX / 4).max(1);
        let option_width = self
            .blocks
            .iter()
            .filter_map(|block| {
                if let Block::Options(rows) = block {
                    Some(rows)
                } else {
                    None
                }
            })
            .flatten()
            .map(|row| text::display_width(&row.label()))
            .max()
            .unwrap_or(0);
        let mut out = Output::default();
        for block in &self.blocks {
            match block {
                Block::Text(value) => out.text(value, width),
                Block::Pairs { rows, indent } => {
                    let labels = rows
                        .iter()
                        .map(|(key, _)| text::display_width(key))
                        .max()
                        .unwrap_or(0);
                    for (key, value) in rows {
                        let start = format!("{}{key}", " ".repeat(*indent));
                        let column = indent + labels + 1;
                        if value.is_empty() {
                            wrapped(
                                &mut out,
                                &" ".repeat(*indent),
                                &" ".repeat(*indent),
                                key,
                                width,
                            );
                        } else if !compact && column + 24 <= width {
                            wrapped(
                                &mut out,
                                &format!(
                                    "{start}{} ",
                                    " ".repeat(labels - text::display_width(key))
                                ),
                                &" ".repeat(column),
                                value,
                                width,
                            );
                        } else if compact {
                            wrapped(
                                &mut out,
                                &format!("{start} "),
                                &" ".repeat(indent + 2),
                                value,
                                width,
                            );
                        } else {
                            wrapped(
                                &mut out,
                                &" ".repeat(*indent),
                                &" ".repeat(*indent),
                                key,
                                width,
                            );
                            wrapped(
                                &mut out,
                                &" ".repeat(indent + 2),
                                &" ".repeat(indent + 2),
                                value,
                                width,
                            );
                        }
                    }
                }
                Block::Columns { rows, indent } => {
                    let count = rows.iter().map(Vec::len).max().unwrap_or(0);
                    let widths = (0..count.saturating_sub(1))
                        .map(|column| {
                            rows.iter()
                                .filter_map(|row| row.get(column))
                                .map(|cell| text::display_width(cell))
                                .max()
                                .unwrap_or(0)
                        })
                        .collect::<Vec<_>>();
                    for row in rows {
                        let Some((value, labels)) = row.split_last() else {
                            continue;
                        };
                        let key = labels
                            .iter()
                            .enumerate()
                            .map(|(column, label)| {
                                if compact {
                                    format!("{label} ")
                                } else {
                                    format!(
                                        "{label}{} ",
                                        " ".repeat(widths[column] - text::display_width(label))
                                    )
                                }
                            })
                            .collect::<String>();
                        if value.is_empty() {
                            let prefix = " ".repeat(*indent);
                            wrapped(&mut out, &prefix, &prefix, key.trim_end(), width);
                            continue;
                        }
                        let column = indent + text::display_width(&key);
                        if column + 24 <= width || (compact && column < width) {
                            wrapped(
                                &mut out,
                                &format!("{}{key}", " ".repeat(*indent)),
                                &" ".repeat(if compact { indent + 2 } else { column }),
                                value,
                                width,
                            );
                        } else {
                            let prefix = " ".repeat(*indent);
                            wrapped(&mut out, &prefix, &prefix, &labels.join(" "), width);
                            let prefix = " ".repeat(indent + 2);
                            wrapped(&mut out, &prefix, &prefix, value, width);
                        }
                    }
                }
                Block::Table { headers, rows } => {
                    let natural = text::widths(headers, rows).iter().sum::<usize>()
                        + headers.len().saturating_sub(1)
                        + 2;
                    if natural <= width {
                        out.text(
                            &text::indent(&text::table(Some(headers), rows, 0, compact), 2),
                            width,
                        );
                    } else {
                        for (index, row) in rows.iter().enumerate() {
                            if index > 0 {
                                out.text("\n", width);
                            }
                            for (header, value) in headers.iter().zip(row) {
                                if !value.is_empty() {
                                    wrapped(
                                        &mut out,
                                        "  ",
                                        "    ",
                                        &format!("{header}: {value}"),
                                        width,
                                    );
                                }
                            }
                        }
                    }
                }
                Block::Options(rows) => {
                    let mut groups = Vec::new();
                    for row in rows {
                        if !groups.contains(&row.group.as_str()) {
                            groups.push(row.group.as_str());
                        }
                    }
                    for group in groups {
                        let selected = rows
                            .iter()
                            .filter(|row| row.group == group)
                            .collect::<Vec<_>>();
                        if selected.is_empty() {
                            continue;
                        }
                        if !group.is_empty() {
                            wrapped(&mut out, "", "", &format!("\n{}:", group), width);
                        }
                        for row in selected {
                            let short = row
                                .short
                                .map(|short| format!("-{short}"))
                                .unwrap_or_default();
                            let label = row.label();
                            let start = out.logical.len();
                            let base = " ".repeat(row.indent);
                            let column = row.indent + 3 + option_width + 1;
                            let wide = !compact && column + 24 <= width;
                            let description_indent;
                            if compact {
                                let key = format!(
                                    "{}{}",
                                    if short.is_empty() {
                                        String::new()
                                    } else {
                                        format!("{short} ")
                                    },
                                    label
                                );
                                wrapped(
                                    &mut out,
                                    &base,
                                    &" ".repeat(row.indent + 2),
                                    &format!("{key} {}", row.description),
                                    width,
                                );
                                out.mark(start, 0..base.len() + key.len(), "info");
                                description_indent = row.indent + 2;
                            } else if wide {
                                let key = format!(
                                    "{base}{short:2} {label}{} ",
                                    " ".repeat(option_width - text::display_width(&label))
                                );
                                wrapped(
                                    &mut out,
                                    &key,
                                    &" ".repeat(column),
                                    &row.description,
                                    width,
                                );
                                out.mark(start, 0..key.trim_end().len(), "info");
                                description_indent = column;
                            } else {
                                wrapped(
                                    &mut out,
                                    &format!("{base}{short:2} "),
                                    &" ".repeat(row.indent + 3),
                                    &label,
                                    width,
                                );
                                out.mark_lines(start, "info");
                                let prefix = " ".repeat(row.indent + 2);
                                wrapped(&mut out, &prefix, &prefix, &row.description, width);
                                description_indent = row.indent + 2;
                            }
                            let mut notes = Vec::new();
                            if !row.choices.is_empty() {
                                notes.push(format!("Values: {}.", row.choices.join(", ")));
                            }
                            if let Some(value) = &row.default {
                                notes.push(format!("Default: {value}."));
                            }
                            if !notes.is_empty() {
                                let prefix = " ".repeat(description_indent);
                                let start = out.logical.len();
                                wrapped(&mut out, &prefix, &prefix, &notes.join(" "), width);
                                out.mark_lines(start, "muted");
                            }
                        }
                    }
                }
            }
        }
        Document::projected(out.text, out.logical, out.layout, out.styles)
    }
}

#[derive(Default)]
struct Output {
    text: String,
    logical: Vec<String>,
    layout: Vec<text::ReadingRow>,
    styles: Vec<Vec<crate::terminal::style::Span>>,
}

impl Output {
    fn text(&mut self, text: &str, width: usize) {
        let (visual, logical, mut rows) = text::Reading::new(Some(width)).mapped(text);
        let offset = self.logical.len();
        for row in &mut rows {
            row.line += offset;
        }
        self.text.push_str(&visual);
        self.styles.extend(
            logical
                .iter()
                .map(|line| crate::terminal::style::plain(line)),
        );
        self.logical.extend(logical);
        self.layout.extend(rows);
    }

    fn mark(&mut self, line: usize, range: std::ops::Range<usize>, role: &str) {
        if let Some(spans) = self.styles.get_mut(line) {
            spans.push(crate::terminal::style::Span {
                range,
                role: role.into(),
            });
        }
    }

    fn mark_lines(&mut self, first: usize, role: &str) {
        for line in first..self.logical.len() {
            self.mark(line, 0..self.logical[line].len(), role);
        }
    }
}

/// Keep unwrapped logical text so search and colors survive generated line breaks.
fn wrapped(out: &mut Output, first: &str, continuation: &str, value: &str, width: usize) {
    for (line_index, line) in value.split('\n').enumerate() {
        if line.is_empty() {
            out.text("\n", width);
            continue;
        }
        let source_prefix = if line_index == 0 { first } else { continuation };
        let source = format!("{source_prefix}{line}");
        let logical = out.logical.len();
        out.styles.push(crate::terminal::style::plain(&source));
        out.logical.push(source);
        let mut rest = line;
        let mut consumed = 0;
        let mut first_row = true;
        loop {
            let prefix = if first_row {
                source_prefix
            } else {
                continuation
            };
            let available = width.saturating_sub(text::display_width(prefix)).max(1);
            let end = text::wrap_end(rest, available);
            let displayed = rest[..end].trim_end();
            out.text.push_str(prefix);
            out.text.push_str(displayed);
            out.text.push('\n');
            out.layout.push(text::ReadingRow {
                line: logical,
                range: if first_row {
                    0..source_prefix.len() + displayed.len()
                } else {
                    source_prefix.len() + consumed..source_prefix.len() + consumed + displayed.len()
                },
                prefix: if first_row { 0 } else { prefix.len() },
            });
            let remaining = &rest[end..];
            let trimmed = remaining.trim_start();
            consumed += end + remaining.len() - trimmed.len();
            rest = trimmed;
            if rest.is_empty() {
                break;
            }
            first_row = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Page {
        let mut page = Page::default();
        page.text("clerk sample - a reusable help page\n");
        page.options(vec![
            OptionEntry {
                name: "scope".into(),
                short: Some('S'),
                argument: "<scope>".into(),
                description:
                    "DESCRIPTION_FIRST select current records and retain all required context"
                        .into(),
                choices: vec!["current".into(), "archived".into(), "all".into()],
                default: Some("current".into()),
                group: "Data scope".into(),
                indent: 2,
            },
            OptionEntry {
                name: "longer-option".into(),
                short: None,
                argument: "<value>".into(),
                description:
                    "DESCRIPTION_SECOND preserve Chinese text 中文内容 and e\u{301} characters"
                        .into(),
                choices: Vec::new(),
                default: None,
                group: "Custom group".into(),
                indent: 2,
            },
        ]);
        page
    }

    #[test]
    fn unheaded_columns_align_display_cells_and_stack_descriptions_on_narrow_screens() {
        let mut page = Page::default();
        page.columns(
            vec![
                vec![
                    "assets".into(),
                    "资源".into(),
                    "DESCRIPTION_FIRST complete content".into(),
                ],
                vec![
                    "milestones".into(),
                    "里程碑".into(),
                    "DESCRIPTION_SECOND complete content".into(),
                ],
                vec!["tasks".into(), "任务".into(), String::new()],
            ],
            2,
        );
        let document = page.render(Some(100), false);
        assert!(document.text.contains("tasks"));
        let lines = document
            .text
            .lines()
            .filter(|line| line.contains("DESCRIPTION_"))
            .collect::<Vec<_>>();
        let offsets = lines
            .iter()
            .map(|line| text::display_width(&line[..line.find("DESCRIPTION_").unwrap()]))
            .collect::<Vec<_>>();
        assert_eq!(offsets[0], offsets[1]);
        assert!(!document.text.contains(" - "));
        for width in [20, 32, 48] {
            for compact in [false, true] {
                let narrow = page.render(Some(width), compact);
                assert!(narrow.text.contains("tasks"));
                assert!(
                    narrow
                        .text
                        .lines()
                        .all(|line| text::display_width(line) <= width)
                );
                assert!(
                    narrow
                        .text
                        .split_whitespace()
                        .collect::<String>()
                        .contains("DESCRIPTION_SECOND")
                );
            }
        }
        let narrow = page.render(Some(32), false);
        assert!(narrow.text.contains("  assets 资源\n    DESCRIPTION_FIRST"));
        assert!(
            page.render(Some(100), true)
                .text
                .contains("  assets 资源 DESCRIPTION_FIRST")
        );
    }

    #[test]
    fn columns_span_groups_and_missing_short_names_keep_the_same_slot() {
        let document = page().render(Some(120), false);
        let lines = document
            .text
            .lines()
            .filter(|line| line.contains("DESCRIPTION_"))
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].find("DESCRIPTION_"), lines[1].find("DESCRIPTION_"));
        assert_eq!(lines[0].find("--scope"), lines[1].find("--longer-option"));
        assert!(
            document
                .text
                .contains("Values: current, archived, all. Default: current.")
        );
    }

    #[test]
    fn narrow_compact_and_resize_layouts_retain_all_content_without_overflow() {
        let page = page();
        let original = page.render(Some(120), false).text;
        for compact in [false, true] {
            for width in [20, 32, 48, 64, 80, 120] {
                let document = page.render(Some(width), compact);
                assert!(
                    document
                        .text
                        .lines()
                        .all(|line| text::display_width(line) <= width),
                    "{width}: {}",
                    document.text
                );
                let joined = document.text.split_whitespace().collect::<String>();
                for expected in [
                    "DESCRIPTION_FIRST",
                    "DESCRIPTION_SECOND",
                    "--longer-option",
                    "中文内容",
                    "e\u{301}",
                    "archived",
                    "Default:current.",
                ] {
                    assert!(joined.contains(expected), "{width}: missing {expected}");
                }
                assert!(!document.colored(false).contains('\x1b'));
                assert!(document.colored(true).contains("\x1b["));
            }
        }
        assert_eq!(original, page.render(Some(120), false).text);
        assert!(
            page.render(Some(48), false)
                .text
                .contains("\n    DESCRIPTION_FIRST")
        );
    }

    #[test]
    fn search_and_styles_keep_logical_ranges_across_wrapped_help_rows() {
        let document = page().render(Some(20), false).reading_width(20);
        let mut search = crate::terminal::search::Search::new(&Default::default());
        search.input.set("records and retain".into());
        search.rebuild(&document);
        assert!(search.status(&document, 0, 100).contains("match 1/1"));
        assert!(search.row(&document).is_some());
        assert!(
            (0..document.layout.len())
                .filter(|row| !search.spans(&document, *row).is_empty())
                .count()
                >= 2
        );
        let visual = document.text.lines().collect::<Vec<_>>();
        for (index, row) in document.layout.iter().enumerate() {
            assert_eq!(
                &document.logical[row.line][row.range.clone()],
                &visual[index][row.prefix..]
            );
            assert!(
                document
                    .visual_spans(index)
                    .iter()
                    .all(|span| span.range.end <= visual[index].len())
            );
        }
    }

    #[test]
    fn tables_stack_complete_cells_instead_of_hiding_columns_or_truncating() {
        let mut page = Page::default();
        page.table(
            vec!["field".into(), "notes".into()],
            vec![vec![
                "long_field_identifier".into(),
                "complete notes with 中文内容 and LAST-MARKER".into(),
            ]],
        );
        let document = page.render(Some(20), false);
        assert!(
            document
                .text
                .lines()
                .all(|line| text::display_width(line) <= 20)
        );
        let joined = document.text.split_whitespace().collect::<String>();
        assert!(joined.contains("long_field_identifier"));
        assert!(joined.contains("LAST-MARKER"));
        assert!(joined.contains("中文内容"));
    }
}
