//! Terminal text: display width, padding and table layout.
//!
//! Column widths count wide characters as two cells so that tables with mixed
//! scripts stay aligned.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Width of text in terminal cells, including combining and emoji sequences.
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Keep scanning rows on one physical line and make control sequences inert.
pub fn single_line(text: &str) -> String {
    visible(&text.split_whitespace().collect::<Vec<_>>().join(" "), 0)
}

/// Pad to a width, or keep the string when it is already wider.
pub fn pad(text: &str, width: usize) -> String {
    let current = display_width(text);
    if current >= width {
        return text.to_string();
    }
    let mut out = String::from(text);
    out.push_str(&" ".repeat(width - current));
    out
}

/// Cut a string to a width, marking the cut with three dots.
pub fn truncate(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let limit = width - 3;
    let mut out = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let step = display_width(grapheme);
        if used + step > limit {
            break;
        }
        used += step;
        out.push_str(grapheme);
    }
    out.push_str("...");
    out
}

/// A name and value block, with the names padded to the widest one.
/// Values may have continuation lines; child nodes belong in `group` instead.
pub fn pairs(rows: &[(String, String)], indent: usize) -> String {
    pairs_with(rows, indent, false)
}

/// Indent a rendered block, retaining blank lines without trailing spaces.
pub fn indent(block: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    block
        .split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect()
}

/// A parent label and value followed by children two cells beneath the label.
pub fn group(label: &str, value: &str, children: &str) -> String {
    let mut out = pairs_with(&[(label.into(), value.into())], 0, true);
    out.push_str(&indent(children, 2));
    out
}

/// Mark every line of verbatim content, including blank lines, with a visible gutter.
pub fn literal(block: &str) -> String {
    block
        .split('\n')
        .map(|line| format!("| {}\n", line.trim_end_matches('\r')))
        .collect()
}

/// End of a generated source gutter, with an optional source line number.
pub fn literal_gutter(line: &str) -> Option<usize> {
    let bar = line.find('|')?;
    (line[..bar]
        .bytes()
        .all(|ch| ch.is_ascii_digit() || ch == b' ')
        && (bar + 1 == line.len() || line.as_bytes()[bar + 1] == b' '))
        .then_some(bar + 1)
}

/// Presentation-only wrapping; the source content and machine output stay untouched.
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    width: Option<usize>,
}

/// One visual row maps a byte interval of its original logical line.
#[derive(Clone, Debug)]
pub struct ReadingRow {
    pub line: usize,
    pub range: std::ops::Range<usize>,
    pub prefix: usize,
}

impl Reading {
    pub fn width(self) -> Option<usize> {
        self.width
    }
    pub fn new(width: Option<usize>) -> Self {
        Self {
            width: width.filter(|width| *width > 0),
        }
    }

    /// Source line numbers belong to physical source lines, never visual continuations.
    pub fn literal(self, block: &str, first: usize) -> String {
        if self.width.is_none() {
            return literal(block);
        }
        let lines: Vec<_> = if block.is_empty() {
            vec![""]
        } else {
            block.split_terminator('\n').collect()
        };
        let digits = first
            .saturating_add(lines.len().saturating_sub(1))
            .to_string()
            .len();
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                format!(
                    "{:>digits$} | {}\n",
                    first.saturating_add(index),
                    line.trim_end_matches('\r')
                )
            })
            .collect()
    }

    /// Wrap only after all parent indentation has been applied to the block.
    pub fn render(self, block: &str) -> String {
        if self.width.is_none() {
            return block.into();
        }
        self.mapped(block).0
    }

    pub fn mapped(self, block: &str) -> (String, Vec<String>, Vec<ReadingRow>) {
        let width = self.width.unwrap_or(usize::MAX / 2);
        let mut logical = Vec::new();
        let mut rows = Vec::new();
        let mut out = String::new();
        for line in block.split_terminator('\n') {
            let indent = line.len() - line.trim_start_matches(' ').len();
            let (prefix, continuation, body) = if let Some(end) = literal_gutter(line) {
                let number = &line[..end - 1];
                let body = line[end..].strip_prefix(' ').unwrap_or(&line[end..]);
                (
                    format!("{number}| "),
                    format!("{}| ", " ".repeat(number.len())),
                    body,
                )
            } else if let Some((label, body)) = line.split_once(": ") {
                let padding = body.len() - body.trim_start_matches(' ').len();
                let prefix = format!("{label}: {}", " ".repeat(padding));
                let columns = display_width(&prefix);
                if columns + 2 <= width {
                    (prefix, " ".repeat(columns), &body[padding..])
                } else {
                    // Long keys cannot consume the entire value column.
                    (" ".repeat(indent), " ".repeat(indent), &line[indent..])
                }
            } else {
                (
                    " ".repeat(indent),
                    " ".repeat((indent + 2).min(width.saturating_sub(2))),
                    &line[indent..],
                )
            };
            let body = visible(body, display_width(&prefix));
            logical.push(format!("{prefix}{body}"));
            let mut consumed = 0;
            let mut rest = body.as_str();
            let mut first = true;
            loop {
                let gutter = if first { &prefix } else { &continuation };
                let available = width.saturating_sub(display_width(gutter)).max(1);
                let end = wrap_end(rest, available);
                rows.push(ReadingRow {
                    line: logical.len() - 1,
                    range: if first {
                        0..prefix.len() + end
                    } else {
                        prefix.len() + consumed..prefix.len() + consumed + end
                    },
                    prefix: if first { 0 } else { gutter.len() },
                });
                out.push_str(gutter);
                out.push_str(&rest[..end]);
                out.push('\n');
                consumed += end;
                rest = &rest[end..];
                if rest.is_empty() {
                    break;
                }
                first = false;
            }
        }
        (out, logical, rows)
    }
}

/// Expand tabs at four-cell stops and make terminal control characters inert.
pub(crate) fn visible(text: &str, column: usize) -> String {
    let mut out = String::new();
    let mut used = column;
    for grapheme in text.graphemes(true) {
        if grapheme == "\t" {
            let spaces = 4 - used % 4;
            out.push_str(&" ".repeat(spaces));
            used += spaces;
        } else if grapheme.chars().any(char::is_control) {
            let escaped = grapheme.escape_default().to_string();
            used += display_width(&escaped);
            out.push_str(&escaped);
        } else {
            used += display_width(grapheme);
            out.push_str(grapheme);
        }
    }
    out
}

/// Prefer a nearby word or punctuation boundary, preserving every source character.
pub(crate) fn wrap_end(text: &str, width: usize) -> usize {
    let mut used = 0;
    let mut end = 0;
    let mut preferred = 0;
    for (index, grapheme) in text.grapheme_indices(true) {
        let step = display_width(grapheme);
        if used + step > width && end > 0 {
            break;
        }
        used += step;
        end = index + grapheme.len();
        if used >= width / 2
            && grapheme.chars().any(|c| {
                c.is_whitespace()
                    || matches!(
                        c,
                        ',' | '.'
                            | ';'
                            | ':'
                            | '/'
                            | '\u{ff0c}'
                            | '\u{3002}'
                            | '\u{ff1b}'
                            | '\u{3001}'
                    )
            })
        {
            preferred = end;
        }
        if used > width {
            break;
        }
    }
    if end == text.len() || preferred == 0 {
        end
    } else {
        preferred
    }
}

/// Render a name and value block with optional compact spacing.
pub fn pairs_with(rows: &[(String, String)], indent: usize, compact: bool) -> String {
    let width = rows
        .iter()
        .map(|(name, _)| display_width(name))
        .max()
        .unwrap_or(0);
    let prefix = " ".repeat(indent);
    let mut out = String::new();
    for (name, value) in rows {
        let label = if compact {
            name.clone()
        } else {
            pad(name, width)
        };
        let gap = " ";
        for (index, line) in value.split('\n').enumerate() {
            out.push_str(&prefix);
            if index == 0 {
                out.push_str(if value.is_empty() { name } else { &label });
            } else if !compact {
                out.push_str(&" ".repeat(width));
            }
            if !line.is_empty() {
                out.push_str(gap);
                out.push_str(line.trim_end_matches('\r'));
            }
            out.push('\n');
        }
    }
    out
}
/// Column widths of a table: the widest cell of each column.
pub fn widths(headers: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths = vec![0usize; headers.len()];
    for row in std::iter::once(headers).chain(rows.iter().map(Vec::as_slice)) {
        for (index, cell) in row.iter().enumerate() {
            if index >= widths.len() {
                break;
            }
            let width = cell.lines().map(display_width).max().unwrap_or(0);
            if width > widths[index] {
                widths[index] = width;
            }
        }
    }
    widths
}
/// Render a grid with a separator row under the heading.
///
/// `max_width` bounds the width of one column; zero means no bound.
pub fn grid(headers: &[String], rows: &[Vec<String>], max_width: usize) -> String {
    table(Some(headers), rows, max_width, false)
}
/// Render the same grid without the heading row.
pub fn body(rows: &[Vec<String>], max_width: usize) -> String {
    table(None, rows, max_width, false)
}

/// Geometry for a terminal table, independent of kind and field names.
pub struct TableLayout<'a> {
    pub width: Option<usize>,
    pub max_width: usize,
    pub compact: bool,
    pub headers: bool,
    pub priority: &'a [usize],
    pub flex: Option<usize>,
    pub explicit: bool,
}

/// Select and shorten cells before using the shared table renderer.
/// Hidden indices refer to original columns, in their original display order.
#[derive(Default)]
pub struct FittedTable {
    pub text: String,
    pub hidden: Vec<usize>,
    pub cells: Vec<(usize, usize, std::ops::Range<usize>)>,
}

pub fn fitted_table(
    headers: &[String],
    rows: &[Vec<String>],
    layout: &TableLayout<'_>,
) -> (String, Vec<usize>) {
    let table = fitted_cells(headers, rows, layout);
    (table.text, table.hidden)
}

pub fn fitted_cells(
    headers: &[String],
    rows: &[Vec<String>],
    layout: &TableLayout<'_>,
) -> FittedTable {
    if headers.is_empty() {
        return FittedTable::default();
    }
    let headers = headers
        .iter()
        .map(|cell| single_line(cell))
        .collect::<Vec<_>>();
    let rows = rows
        .iter()
        .map(|row| row.iter().map(|cell| single_line(cell)).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let blank = vec![String::new(); headers.len()];
    let natural = widths(if layout.headers { &headers } else { &blank }, &rows)
        .into_iter()
        .map(|width| {
            if layout.max_width > 0 {
                width.min(layout.max_width)
            } else {
                width
            }
        })
        .map(|width| width.max(1))
        .collect::<Vec<_>>();
    let mut widths = natural.clone();
    let mut selected = (0..headers.len()).collect::<Vec<_>>();
    let rank = |index: usize| {
        layout
            .priority
            .iter()
            .position(|value| *value == index)
            .unwrap_or(layout.priority.len() + index)
    };
    if let Some(limit) = layout.width {
        for (index, width) in widths.iter_mut().enumerate() {
            *width = (*width).min(if layout.flex == Some(index) {
                (limit * 2 / 5).max(12)
            } else {
                16
            });
        }
        let used = |selected: &[usize], widths: &[usize]| {
            selected.iter().map(|index| widths[*index]).sum::<usize>()
                + selected.len().saturating_sub(1)
        };
        if layout.explicit {
            while selected.len() > 1 && selected.len() * 2 - 1 > limit {
                selected.pop();
            }
        } else {
            while selected.len() > headers.len().min(2) && used(&selected, &widths) > limit {
                let remove = selected
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, index)| rank(**index))
                    .map(|(position, _)| position)
                    .expect("selected columns");
                selected.remove(remove);
            }
        }
        while used(&selected, &widths) > limit {
            let Some(index) = selected
                .iter()
                .copied()
                .filter(|index| widths[*index] > 1)
                .max_by_key(|index| widths[*index])
            else {
                break;
            };
            widths[index] -= 1;
        }
        let mut order = selected.clone();
        order.sort_by_key(|index| (Some(*index) != layout.flex, rank(*index)));
        let mut remaining = limit.saturating_sub(used(&selected, &widths));
        for index in order {
            let added = natural[index].saturating_sub(widths[index]).min(remaining);
            widths[index] += added;
            remaining -= added;
        }
    }
    let shortened = |row: &[String]| {
        selected
            .iter()
            .map(|index| {
                truncate(
                    row.get(*index).map(String::as_str).unwrap_or(""),
                    widths[*index],
                )
            })
            .collect::<Vec<_>>()
    };
    let head = shortened(&headers);
    let rows = rows.iter().map(|row| shortened(row)).collect::<Vec<_>>();
    let hidden = (0..headers.len())
        .filter(|index| !selected.contains(index))
        .collect();
    let text = table(
        layout.headers.then_some(head.as_slice()),
        &rows,
        0,
        layout.compact,
    );
    let blank = vec![String::new(); head.len()];
    let columns = self::widths(if layout.headers { &head } else { &blank }, &rows);
    let header = if layout.headers {
        if layout.compact { 1 } else { 2 }
    } else {
        0
    };
    let mut cells = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        let mut offset = 0;
        for (column, value) in row.iter().enumerate() {
            cells.push((
                header + row_index,
                selected[column],
                offset..offset + value.len(),
            ));
            offset += value.len()
                + if !layout.compact && column + 1 < row.len() {
                    columns[column].saturating_sub(display_width(value))
                } else {
                    0
                };
            offset += 1;
        }
    }
    FittedTable {
        text,
        hidden,
        cells,
    }
}

/// Render terminal columns; compact mode omits padding and the heading rule.
pub fn table(
    header: Option<&[String]>,
    rows: &[Vec<String>],
    max_width: usize,
    compact: bool,
) -> String {
    let columns = header
        .map(|head| head.len())
        .unwrap_or_else(|| rows.iter().map(Vec::len).max().unwrap_or(0));
    if columns == 0 {
        return String::new();
    }
    let empty: Vec<String> = vec![String::new(); columns];
    let head = header.unwrap_or(&empty);
    let mut cells_rows: Vec<Vec<String>> = Vec::new();
    if header.is_some() {
        cells_rows.push(head.to_vec());
    }
    for row in rows {
        let mut cells = Vec::new();
        for index in 0..columns {
            let value = row.get(index).map(String::as_str).unwrap_or("");
            cells.push(value.to_string());
        }
        cells_rows.push(cells);
    }

    let mut widths = widths(head, rows);
    if max_width > 0 {
        for width in &mut widths {
            if *width > max_width {
                *width = max_width;
            }
        }
    }

    let gap = " ";
    let mut lines = Vec::new();
    for (row_index, row) in cells_rows.iter().enumerate() {
        let cells: Vec<Vec<&str>> = row.iter().map(|cell| cell.split('\n').collect()).collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for line_index in 0..height {
            let mut line = String::new();
            for (index, cell) in cells.iter().enumerate() {
                if index > 0 {
                    line.push_str(gap);
                }
                let value = cell
                    .get(line_index)
                    .copied()
                    .unwrap_or("")
                    .trim_end_matches('\r');
                let value = if max_width > 0 {
                    truncate(value, widths[index])
                } else {
                    value.to_string()
                };
                if compact || index + 1 == columns {
                    line.push_str(&value);
                } else {
                    line.push_str(&pad(&value, widths[index]));
                }
            }
            lines.push(line);
        }
        if row_index == 0 && header.is_some() && !compact {
            lines.push(
                widths
                    .iter()
                    .map(|width| "-".repeat(*width))
                    .collect::<Vec<_>>()
                    .join(gap),
            );
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_tables_handle_explicit_overflow_and_nontrailing_priorities() {
        let headers = (0..30).map(|index| format!("c{index}")).collect::<Vec<_>>();
        let rows = vec![vec!["long value".into(); 30]];
        let layout = TableLayout {
            width: Some(20),
            max_width: 0,
            compact: false,
            headers: true,
            priority: &[],
            flex: None,
            explicit: true,
        };
        let (output, hidden) = fitted_table(&headers, &rows, &layout);
        assert_eq!(hidden.len(), 20);
        assert_eq!(output.lines().count(), 3);
        assert!(
            output.lines().all(|line| display_width(line) <= 20),
            "{output}"
        );
        let headers = ["key", "summary", "phase", "team"].map(str::to_string);
        let rows = vec![vec![
            "A".into(),
            "Long caption needing space".into(),
            "active".into(),
            "ops".into(),
        ]];
        let layout = TableLayout {
            width: Some(22),
            priority: &[0, 1, 3, 2],
            flex: Some(1),
            explicit: false,
            ..layout
        };
        let (output, hidden) = fitted_table(&headers, &rows, &layout);
        assert_eq!(hidden, vec![2]);
        assert!(
            output.contains("team") && !output.contains("phase"),
            "{output}"
        );
        assert!(
            output.lines().all(|line| display_width(line) <= 22),
            "{output}"
        );
    }

    #[test]
    fn wraps_source_lines_without_losing_graphemes_or_source_numbers() {
        let reading = Reading::new(Some(24));
        let source = "# Header\n\nMixed \u{4efb}\u{52a1} e\u{301} \u{1f469}\u{200d}\u{1f4bb} and a-long-token-without-spaces\n";
        let output = reading.render(&group("Content:", "", &reading.literal(source, 61)));
        assert!(output.contains("  61 | # Header\n  62 | \n  63 | Mixed "));
        assert!(
            output.lines().all(|line| display_width(line) <= 24),
            "{output}"
        );
        let mut rebuilt = Vec::<String>::new();
        for line in output.lines().skip(1) {
            let (number, body) = line.split_once("| ").unwrap();
            if !number.trim().is_empty() {
                rebuilt.push(String::new());
            }
            rebuilt.last_mut().unwrap().push_str(body);
        }
        assert_eq!(rebuilt.join("\n"), source.trim_end_matches('\n'));
        assert!(output.contains("e\u{301}") && output.contains("\u{1f469}\u{200d}\u{1f4bb}"));
    }

    #[test]
    fn keeps_word_boundaries_tabs_controls_and_value_continuations_readable() {
        let reading = Reading::new(Some(28));
        let output = reading.render(
            "  note: alpha beta gamma delta epsilon\n  path: /a/very/long/path/to/file.rs\n",
        );
        assert!(output.contains("\n        delta epsilon\n"), "{output}");
        assert!(output.lines().all(|line| display_width(line) <= 28));
        let original = "\tfirst\tsecond\u{1b}[31m";
        let shown = reading.render(&reading.literal(original, 1));
        assert!(!shown.contains('\t') && !shown.contains('\u{1b}'));
        assert!(shown.contains("\\u{1b}"));
    }

    #[test]
    fn uses_real_last_line_and_handles_digit_growth_and_extreme_widths() {
        let source = "first\n\nthird\n";
        let reading = Reading::new(Some(20));
        let output = reading.render(&reading.literal(source, 99));
        assert_eq!(output, " 99 | first\n100 | \n101 | third\n");
        assert!(!output.contains("102"));
        assert_eq!(Reading::new(None).literal(source, 99), literal(source));
        assert_eq!(Reading::new(None).render(source), source);
        let narrow = Reading::new(Some(1)).render("\u{4efb}e\u{301}\n");
        assert_eq!(narrow, "\u{4efb}\ne\u{301}\n");
        assert_eq!(display_width("e\u{301}"), 1);
        assert_eq!(display_width("\u{1f469}\u{200d}\u{1f4bb}"), 2);
        assert_eq!(
            truncate("\u{1f469}\u{200d}\u{1f4bb}abcde", 5),
            "\u{1f469}\u{200d}\u{1f4bb}..."
        );
    }

    #[test]
    fn counts_wide_characters() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("任务"), 4);
        assert_eq!(display_width("任务 a"), 6);
    }

    #[test]
    fn truncates_with_marker() {
        assert_eq!(truncate("abcdef", 4), "a...");
        assert_eq!(truncate("ab", 4), "ab");
    }

    #[test]
    fn aligns_mixed_width_columns() {
        let headers = vec!["id".to_string(), "标题".to_string()];
        let rows = vec![
            vec!["APP-001".to_string(), "任务".to_string()],
            vec!["APP-002".to_string(), "x".to_string()],
        ];
        let text = grid(&headers, &rows, 0);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(display_width(lines[2]), display_width(lines[1]));
    }

    #[test]
    fn aligns_multiline_values_by_terminal_width() {
        let rows = vec![
            vec!["任务\nnext".into(), "first\nsecond".into()],
            vec!["x".into(), "last".into()],
        ];
        assert_eq!(body(&rows, 0), "任务 first\nnext second\nx    last\n");
        assert_eq!(table(None, &rows, 3, false), "... ...\n... ...\nx   ...\n");
        let pairs = vec![
            ("任务".into(), "first\nsecond".into()),
            ("x".into(), "last".into()),
        ];
        assert_eq!(
            pairs_with(&pairs, 2, false),
            "  任务 first\n       second\n  x    last\n"
        );
        assert_eq!(
            pairs_with(&pairs, 2, true),
            "  任务 first\n   second\n  x last\n"
        );
    }

    #[test]
    fn compact_tables_keep_empty_cells_and_hide_only_the_rule() {
        let headers = vec!["key".into(), "title".into(), "state".into()];
        let rows = vec![vec!["x".into(), "".into(), "todo".into()]];
        assert_eq!(
            table(Some(&headers), &rows, 0, true),
            "key title state\nx  todo\n"
        );
        assert_eq!(table(None, &rows, 0, true), "x  todo\n");
    }

    #[test]
    fn groups_nest_labels_and_keep_document_boundaries_visible() {
        let content = "# Title\n\n## brief\n| a | b |\n```text\nSource: original\n```";
        let excerpt = group("brief:", "", &group("Content:", "", &literal(content)));
        let output = group("Excerpts:", "", &excerpt);
        assert!(
            output.starts_with("Excerpts:\n  brief:\n    Content:\n      | # Title\n      | \n")
        );
        assert!(output.contains("      | ## brief\n      | | a | b |\n"));
        assert!(output.contains("      | ```text\n      | Source: original\n      | ```\n"));
        assert_eq!(
            group("Workspace:", "/checkout", "branch: main\n"),
            "Workspace: /checkout\n  branch: main\n"
        );
    }
}
