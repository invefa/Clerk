//! Output rendering: one value model, several formats.
//!
//! People read tables with display text; scripts read keys. Both come from the same
//! records so that a change in the data shows up in every format at once.

use crate::config::kind::{FieldKind, Kind};
use crate::execution::engine::Outcome;
use crate::foundation::error::Result;
use crate::storage::store::Record;
use crate::terminal::text;

/// Requested rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
    Tsv,
    Csv,
    Markdown,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Table => "table",
            Format::Json => "json",
            Format::Tsv => "tsv",
            Format::Csv => "csv",
            Format::Markdown => "markdown",
        }
    }
}

/// How one outcome is printed.
#[derive(Clone, Debug)]
pub struct Print {
    pub format: Format,
    pub columns: Option<Vec<String>>,
    pub max_width: usize,
    pub display: bool,
    pub long: bool,
    pub compact: bool,
    /// Order declared by the view being printed.
    pub view_sort: Vec<String>,
    /// Sort keys from the command line, which win over the view.
    pub sort: Vec<String>,
    /// Reverse the order when the command line asked for a sort.
    pub desc: bool,
    /// Print the heading row of a table.
    pub headers: bool,
    pub width: Option<usize>,
    pub plain: bool,
    pub details: bool,
}

impl Default for Print {
    fn default() -> Self {
        Print {
            format: Format::Table,
            columns: None,
            max_width: 0,
            display: false,
            long: false,
            compact: false,
            view_sort: Vec::new(),
            sort: Vec::new(),
            desc: false,
            headers: true,
            width: None,
            plain: false,
            details: false,
        }
    }
}
/// Render one outcome.
pub fn render(
    context: &crate::execution::context::Context,
    kind: &Kind,
    outcome: &Outcome,
    print: &Print,
) -> Result<String> {
    Ok(document(context, kind, outcome, print)?.text)
}

/// Render boundaries alongside text for shared screen handling.
pub fn document(
    context: &crate::execution::context::Context,
    kind: &Kind,
    outcome: &Outcome,
    print: &Print,
) -> Result<crate::terminal::reader::Document> {
    let view = kind
        .views
        .iter()
        .find(|view| {
            view.target.is_none()
                && view.renderer.as_deref().unwrap_or("table") == "table"
                && view.name
                    == if print.long && print.format == Format::Table {
                        "long"
                    } else {
                        "list"
                    }
        })
        .or_else(|| {
            kind.views.iter().find(|view| {
                view.name == "list"
                    && view.target.is_none()
                    && view.renderer.as_deref().unwrap_or("table") == "table"
            })
        });
    let mut columns = print.columns.clone().unwrap_or_else(|| {
        if print.format == Format::Table && outcome.verb == "list" {
            if print.details {
                kind.fields.iter().map(|field| field.name.clone()).collect()
            } else if print.long {
                view.map(|view| view.columns.clone())
                    .unwrap_or_else(|| outcome.columns.clone())
            } else {
                outcome.columns.clone()
            }
        } else {
            outcome.columns.clone()
        }
    });
    let max_width = print.max_width;
    let display = print.display;
    let keys = sort_keys(context, kind, print)?;
    let mut records = if keys.is_empty() {
        outcome.records.clone()
    } else {
        crate::query::order::apply(context, kind, &keys, &outcome.records)?
    };
    if !print.sort.is_empty() && print.desc {
        records.reverse();
    }
    if outcome
        .scope
        .as_deref()
        .is_some_and(|scope| scope != "current")
        && print.format != Format::Json
    {
        if !columns.iter().any(|name| name == "@archive") {
            columns.insert(0, "@archive".into());
        }
        for record in &mut records {
            let archived = outcome
                .archived_keys
                .contains(&crate::query::scope::key(kind, record));
            record.insert("@archive".into(), archived.to_string());
        }
    }
    match print.format {
        Format::Table => {
            if print.details && outcome.verb == "list" {
                let mut blocks = Vec::new();
                for record in &records {
                    let mut block =
                        detail(context, kind, record, &columns, display, print.compact)?;
                    block.push('\n');
                    blocks.push(block);
                }
                let mut doc = crate::terminal::reader::Document::blocks(blocks);
                doc.detail_styles(context, kind, &records, &columns, display)?;
                doc.identities = records
                    .iter()
                    .map(|record| crate::query::scope::key(kind, record))
                    .collect();
                return Ok(
                    if let Some(width) =
                        crate::terminal::size::reading(print.plain, print.width).width()
                    {
                        doc.reading_width(width)
                    } else {
                        doc
                    },
                );
            }
            if outcome.verb == "show" && records.len() == 1 {
                let mut doc: crate::terminal::reader::Document =
                    detail(context, kind, &records[0], &columns, display, print.compact)?.into();
                doc.detail_styles(context, kind, &records, &columns, display)?;
                doc.identities = records
                    .iter()
                    .map(|record| crate::query::scope::key(kind, record))
                    .collect();
                return Ok(
                    if let Some(width) =
                        crate::terminal::size::reading(print.plain, print.width).width()
                    {
                        doc.reading_width(width)
                    } else {
                        doc
                    },
                );
            }
            if outcome.verb == "list" {
                return list_table(context, kind, view, &records, &columns, print);
            }
            let rows = project(context, kind, &records, &columns, display)?
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|cell| text::single_line(&cell))
                        .collect()
                })
                .collect::<Vec<Vec<String>>>();
            let headers = columns
                .iter()
                .map(|name| header_of(kind, name, display))
                .collect::<Vec<_>>();
            let mut out = text::table(
                print.headers.then_some(headers.as_slice()),
                &rows,
                max_width,
                print.compact,
            );
            for note in &outcome.notes {
                out.push_str(note);
                out.push('\n');
            }
            Ok(out.into())
        }
        Format::Tsv => {
            let mut out = String::new();
            out.push_str(&columns.join("\t"));
            out.push('\n');
            for row in raw_rows(&records, &columns) {
                out.push_str(&row.join("\t"));
                out.push('\n');
            }
            Ok(out.into())
        }
        Format::Csv => {
            let mut rows = raw_rows(&records, &columns);
            rows.insert(0, columns.clone());
            Ok(crate::foundation::csv::serialize(&rows).into())
        }
        Format::Markdown => {
            let rows = raw_rows(&records, &columns);
            Ok(markdown(kind, &columns, &rows, display).into())
        }
        Format::Json => Ok(json(
            kind,
            outcome,
            &raw_rows(&records, &columns),
            &columns,
            &records,
        )
        .into()),
    }
}

/// One initial-order policy shared by continuous output and interactive readers.
pub fn sort_keys(
    context: &crate::execution::context::Context,
    kind: &Kind,
    print: &Print,
) -> Result<Vec<String>> {
    if !print.sort.is_empty() {
        return Ok(print.sort.clone());
    }
    if print.long
        && print.format == Format::Table
        && let Some(view) = kind.views.iter().find(|view| {
            view.name == "long"
                && view.target.is_none()
                && view.renderer.as_deref().unwrap_or("table") == "table"
        })
        && !view.sort.is_empty()
    {
        return Ok(view.sort.clone());
    }
    if !print.view_sort.is_empty() {
        return Ok(print.view_sort.clone());
    }
    crate::query::order::default_keys(context, kind)
}

/// The same geometry applies to default, extended and explicitly selected columns.
fn list_table(
    context: &crate::execution::context::Context,
    kind: &Kind,
    view: Option<&crate::config::kind::View>,
    records: &[Record],
    columns: &[String],
    print: &Print,
) -> Result<crate::terminal::reader::Document> {
    let reading = crate::terminal::size::reading(print.plain, print.width);
    let headers = columns
        .iter()
        .map(|name| header_of(kind, name, print.display))
        .collect::<Vec<_>>();
    let mut rows = project(context, kind, records, columns, print.display)?;
    if print.columns.is_none()
        && let Some(view) = view.filter(|view| view.summary)
    {
        for row in &mut rows {
            for (name, value) in columns.iter().zip(row) {
                if value.is_empty() {
                    *value = view.empty.get(name).cloned().unwrap_or_else(|| "-".into());
                }
            }
        }
    }
    let priority = view
        .map(|view| {
            view.priority
                .iter()
                .filter_map(|name| columns.iter().position(|column| column == name))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let flex = view
        .and_then(|view| view.flex.as_ref())
        .and_then(|name| columns.iter().position(|column| column == name));
    let fitted = text::fitted_cells(
        &headers,
        &rows,
        &text::TableLayout {
            width: reading.width(),
            max_width: print.max_width,
            compact: print.compact,
            headers: print.headers,
            priority: &priority,
            flex,
            explicit: print.columns.is_some(),
        },
    );
    let mut out = fitted.text;
    let hidden = fitted.hidden;
    if !hidden.is_empty() && !records.is_empty() {
        out.push_str(&reading.render(&format!(
                "Hidden columns ({}) - use --details or --columns",
                hidden
                    .iter()
                    .map(|index| columns[*index].as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )));
    }
    let mut document = crate::terminal::reader::Document::table(
        out,
        if print.headers {
            if print.compact { 1 } else { 2 }
        } else {
            0
        },
        records.len(),
    );
    document.identities = records
        .iter()
        .map(|record| crate::query::scope::key(kind, record))
        .collect();
    for (row, column, range) in fitted.cells {
        let field = kind.field(&columns[column]);
        let record_index = row.saturating_sub(document.header);
        let raw = records
            .get(record_index)
            .and_then(|record| record.get(&columns[column]))
            .map(String::as_str)
            .unwrap_or("");
        let role = field
            .and_then(|field| field.terminal_style.as_ref())
            .and_then(|spec| spec.role(raw))
            .or_else(|| kind.key.contains(&columns[column]).then_some("identifier"));
        if let Some(role) = role
            && let Some(spans) = document.styles.get_mut(row)
        {
            spans.push(crate::terminal::style::Span {
                range,
                role: role.into(),
            });
        }
    }
    Ok(document)
}

/// Print a plain table, honouring `--headers`.
pub fn print_table(
    args: &crate::interface::cli::Args,
    headers: &[String],
    rows: &[Vec<String>],
    max_width: usize,
) -> Result<()> {
    let max_width = if args.value("max-width").is_some() {
        args.max_width()
    } else {
        max_width
    };
    let output = text::table(
        crate::interface::cli::headers_shown(args)?.then_some(headers),
        rows,
        max_width,
        args.switch("compact"),
    );
    let doc = crate::terminal::reader::Document::table(
        output,
        if crate::interface::cli::headers_shown(args)? {
            if args.switch("compact") { 1 } else { 2 }
        } else {
            0
        },
        rows.len(),
    );
    print!(
        "{}",
        doc.colored(crate::terminal::style::enabled(args, false))
    );
    Ok(())
}
/// Human rows: display text, with the sort and display rules applied.
fn project(
    context: &crate::execution::context::Context,
    kind: &Kind,
    records: &[Record],
    columns: &[String],
    display: bool,
) -> Result<Vec<Vec<String>>> {
    records
        .iter()
        .map(|record| human_row(context, kind, record, columns, display))
        .collect()
}

/// Raw rows: stored values, which is what the machine formats carry.
fn raw_rows(records: &[Record], columns: &[String]) -> Vec<Vec<String>> {
    records
        .iter()
        .map(|record| {
            columns
                .iter()
                .map(|name| record.get(name).cloned().unwrap_or_default())
                .collect()
        })
        .collect()
}
/// A markdown table, ready to paste into a page.
fn markdown(kind: &Kind, columns: &[String], rows: &[Vec<String>], display: bool) -> String {
    let headings: Vec<String> = columns
        .iter()
        .map(|name| header_of(kind, name, display))
        .collect();
    let mut out = format!("| {} |\n", headings.join(" | "));
    out.push_str(&format!(
        "|{}|\n",
        columns.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    ));
    for row in rows {
        out.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    out
}
fn header_of(kind: &Kind, name: &str, display: bool) -> String {
    if !display {
        return name.to_string();
    }
    kind.field(name)
        .map(|field| field.display.clone())
        .unwrap_or_else(|| name.to_string())
}

fn human_row(
    context: &crate::execution::context::Context,
    kind: &Kind,
    record: &Record,
    columns: &[String],
    display: bool,
) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(columns.len());
    for name in columns {
        let raw = record.get(name).cloned().unwrap_or_default();
        out.push(crate::terminal::value::cell_with_separator(
            context, kind, name, &raw, display, ",",
        )?);
    }
    Ok(out)
}

/// One line of a diagnostic report.
pub struct ReportItem {
    pub name: String,
    pub ok: bool,
    pub line: u32,
    pub detail: String,
}

/// A diagnostic report as JSON, the same shape for check and doctor.
pub fn report_json(ok: bool, items: &[ReportItem]) -> String {
    let data: Vec<String> = items
        .iter()
        .map(|item| {
            format!(
                "{{\"name\":\"{}\",\"ok\":{},\"line\":{},\"detail\":\"{}\"}}",
                escape(&item.name),
                item.ok,
                item.line,
                escape(&item.detail)
            )
        })
        .collect();
    format!(
        "{{\"ok\":{},\"count\":{},\"items\":[{}]}}",
        ok,
        items.len(),
        data.join(",")
    )
}
/// A plain table as JSON, in the same envelope the record output uses.
pub fn rows_json(columns: &[String], rows: &[Vec<String>]) -> String {
    let data: Vec<String> = rows
        .iter()
        .map(|row| {
            let pairs: Vec<String> = columns
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    format!(
                        "\"{}\":\"{}\"",
                        escape(name),
                        escape(row.get(index).map(String::as_str).unwrap_or(""))
                    )
                })
                .collect();
            format!("{{{}}}", pairs.join(","))
        })
        .collect();
    format!(
        "{{\"count\":{},\"columns\":[{}],\"data\":[{}]}}",
        rows.len(),
        columns
            .iter()
            .map(|name| format!("\"{}\"", escape(name)))
            .collect::<Vec<_>>()
            .join(","),
        data.join(",")
    )
}
/// One cell with registry display text applied.
fn human_value(
    context: &crate::execution::context::Context,
    kind: &Kind,
    record: &Record,
    name: &str,
    display: bool,
) -> Result<String> {
    let raw = record.get(name).cloned().unwrap_or_default();
    crate::terminal::value::cell(context, kind, name, &raw, display)
}

/// One record as aligned `display  value` lines.
fn detail(
    context: &crate::execution::context::Context,
    kind: &Kind,
    record: &Record,
    columns: &[String],
    display: bool,
    compact: bool,
) -> Result<String> {
    let mut rows: Vec<(String, String)> = Vec::new();
    for name in columns {
        rows.push((
            header_of(kind, name, display),
            human_value(context, kind, record, name, display)?,
        ));
    }
    Ok(text::pairs_with(&rows, 0, compact))
}

fn json(
    kind: &Kind,
    outcome: &Outcome,
    rows: &[Vec<String>],
    columns: &[String],
    records: &[Record],
) -> String {
    let mut data = Vec::new();
    for row in rows {
        let mut pairs = Vec::new();
        for (index, name) in columns.iter().enumerate() {
            let raw = row.get(index).cloned().unwrap_or_default();
            let value = json_value(kind, name, &raw);
            pairs.push(format!("\"{}\":{}", escape(name), value));
        }
        data.push(format!("{{{}}}", pairs.join(",")));
    }
    let affected: Vec<String> = outcome
        .affected
        .iter()
        .map(|path| format!("\"{}\"", escape(path)))
        .collect();
    let provenance = records
        .iter()
        .map(|record| {
            crate::config::schema::object([
                (
                    "key",
                    crate::config::schema::object(kind.key.iter().map(|name| {
                        (
                            name.as_str(),
                            crate::config::schema::quote(
                                record.get(name).map(String::as_str).unwrap_or(""),
                            ),
                        )
                    })),
                ),
                (
                    "archived",
                    outcome
                        .archived_keys
                        .contains(&crate::query::scope::key(kind, record))
                        .to_string(),
                ),
            ])
        })
        .collect::<Vec<_>>()
        .join(",");
    let metadata = outcome
        .scope
        .as_ref()
        .map(|scope| {
            format!(
                ",\"scope\":{},\"provenance\":[{provenance}]",
                crate::config::schema::quote(scope)
            )
        })
        .unwrap_or_default();
    format!(
        "{{\"kind\":\"{}\",\"verb\":\"{}\",\"count\":{},\"dry_run\":{},\"data\":[{}],\"affected\":[{}],\"execution\":{}{}}}\n",
        escape(&outcome.kind),
        escape(&outcome.verb),
        rows.len(),
        outcome.dry_run,
        data.join(","),
        affected.join(","),
        crate::git::operation::current()
            .map(|report| report.json())
            .unwrap_or_else(|| "null".into()),
        metadata
    )
}

fn json_value(kind: &Kind, name: &str, raw: &str) -> String {
    let Some(field) = kind.field(name) else {
        return format!("\"{}\"", escape(raw));
    };
    if field.kind == FieldKind::Enum && field.multi {
        let items: Vec<String> = raw
            .split('|')
            .filter(|item| !item.trim().is_empty())
            .map(|item| format!("\"{}\"", escape(item.trim())))
            .collect();
        return format!("[{}]", items.join(","));
    }
    format!("\"{}\"", escape(raw))
}

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", other as u32));
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json_text() {
        assert_eq!(escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(escape("line\nbreak"), "line\\nbreak");
    }
}
