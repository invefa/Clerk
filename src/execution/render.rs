//! Projections: rendering records into a page region.
//!
//! A view declares a target file and a region name. The renderer replaces only
//! the text between the region markers, so hand written prose around the block
//! survives every render.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::kind::{Kind, View};
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};

/// What one render pass touched.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub changed: Vec<String>,
    pub blocks: usize,
}

/// Render every view that declares a target.
///
/// A target whose managed path is `generated` is written whole; any other
/// target is patched region by region, and several views may share a region,
/// in which case their blocks are concatenated in declaration order.
pub fn render(context: &Context) -> Result<Report> {
    let mut grouped: BTreeMap<String, Vec<(String, View)>> = BTreeMap::new();
    for kind in context.catalog.kinds() {
        for view in &kind.views {
            let Some(target) = &view.target else {
                continue;
            };
            grouped
                .entry(target.clone())
                .or_default()
                .push((kind.name.clone(), view.clone()));
        }
    }

    let mut report = Report::default();
    for (target, views) in grouped {
        let path = context.workspace.resolve(&target);
        let original = if path.exists() {
            std::fs::read_to_string(&path)
                .map_err(|error| crate::foundation::error::io_error(&path, error))?
        } else {
            String::new()
        };
        let generated = context
            .workspace
            .rule_for(&target)
            .map(|rule| rule.level == crate::config::workspace::Level::Generated)
            .unwrap_or(false);

        let mut body = original.clone();
        if generated {
            let mut blocks = String::new();
            for (kind_name, view) in &views {
                let kind = context.catalog.require(kind_name)?;
                blocks.push_str(&block_text(context, kind, view)?);
                blocks.push('\n');
                report.blocks += 1;
            }
            body = blocks;
        } else {
            let mut by_region: BTreeMap<String, Vec<(String, View)>> = BTreeMap::new();
            for (kind_name, view) in views {
                let Some(region) = view.region.clone() else {
                    return Err(Error::usage(format!(
                        "kind `{kind_name}` view `{}` declares target {target} but no region",
                        view.name
                    )));
                };
                by_region.entry(region).or_default().push((kind_name, view));
            }
            for (region, items) in by_region {
                let mut block = String::new();
                for (index, (kind_name, view)) in items.iter().enumerate() {
                    let kind = context.catalog.require(kind_name)?;
                    if index > 0 {
                        block.push('\n');
                    }
                    block.push_str(&block_text(context, kind, view)?);
                    report.blocks += 1;
                }
                body = replace_region(&body, &region, &block).ok_or_else(|| {
                    Error::usage(format!(
                        "{target}: no region `{region}`; expected {} and {}",
                        begin_marker(&region),
                        end_marker(&region)
                    ))
                })?;
            }
        }
        if body != original {
            if let Some(parent) = path.parent() {
                crate::execution::transaction::create_dir_all(parent)?;
            }
            crate::execution::transaction::write(&path, &body)?;
            report.changed.push(target);
        }
    }
    Ok(report)
}

/// Generated targets whose content no longer matches the renderer output.
pub fn generated_drift(context: &Context) -> Result<Vec<(String, String)>> {
    let mut targets: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for kind in context.catalog.kinds() {
        for view in &kind.views {
            if let Some(target) = &view.target
                && context
                    .workspace
                    .rule_for(target)
                    .map(|rule| rule.level == crate::config::workspace::Level::Generated)
                    .unwrap_or(false)
            {
                targets.insert(target.clone());
            }
        }
    }
    let mut problems = Vec::new();
    for target in targets {
        let mut expected = String::new();
        for item in context.catalog.kinds() {
            for view in &item.views {
                if view.target.as_deref() == Some(target.as_str()) {
                    expected.push_str(&block_text(context, item, view)?);
                    expected.push('\n');
                }
            }
        }
        let actual = read_page(&context.workspace.resolve(&target))?;
        if expected.trim_end() != actual.trim_end() {
            problems.push((
                target,
                "generated file differs from the renderer output".to_string(),
            ));
        }
    }
    Ok(problems)
}

/// Whether every declared region exists in its target.
pub fn missing_regions(context: &Context) -> Vec<(String, String)> {
    let mut problems = Vec::new();
    for kind in context.catalog.kinds() {
        for view in &kind.views {
            let (Some(target), Some(region)) = (&view.target, &view.region) else {
                continue;
            };
            let path = context.workspace.resolve(target);
            let ok = std::fs::read_to_string(&path)
                .map(|body| {
                    body.contains(&begin_marker(region)) && body.contains(&end_marker(region))
                })
                .unwrap_or(false);
            if !ok {
                problems.push((
                    target.clone(),
                    format!("region `{region}` for kind `{}` is missing", kind.name),
                ));
            }
        }
    }
    problems
}

fn block_text(context: &Context, kind: &Kind, view: &View) -> Result<String> {
    let renderer = view.renderer.clone().unwrap_or_else(|| "table".to_string());
    match renderer.as_str() {
        "table" => table_block(
            context,
            kind,
            view,
            &view.columns,
            view.display,
            &view.headers,
        ),
        "lines" => lines_block(context, kind, view),
        "composite" => composite_block(context, kind, view),
        "registry" => registry_block(context, view),
        "focus" => focus_block(context, kind, view),
        "filetable" => file_table_block(context, view),
        "details" => details_block(context, kind, view),
        "vocabularies" => vocabularies_block(context, view),
        other => Err(Error::usage(format!(
            "kind `{}` view `{}`: renderer `{other}` is not supported yet",
            kind.name, view.name
        ))),
    }
}

fn table_block(
    context: &Context,
    kind: &Kind,
    view: &View,
    columns: &[String],
    display: bool,
    headers: &[String],
) -> Result<String> {
    let columns: Vec<String> = if columns.is_empty() {
        kind.list_columns()
    } else {
        columns.to_vec()
    };
    let keys = if view.sort.is_empty() {
        crate::query::order::default_keys(context, kind)?
    } else {
        view.sort.clone()
    };
    let records = crate::query::order::apply(context, kind, &keys, &rows_of(context, kind)?)?;
    let mut out = String::new();
    out.push_str(&format!(
        "| {} |\n",
        columns
            .iter()
            .enumerate()
            .map(|(index, name)| match headers.get(index) {
                Some(title) if headers.len() == columns.len() => title.clone(),
                _ => header_of(kind, name),
            })
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    out.push_str(&format!(
        "|{}|\n",
        columns.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    ));
    for record in &records {
        let cells: Vec<String> = columns
            .iter()
            .map(|name| {
                let raw = record.get(name).cloned().unwrap_or_default();
                if kind
                    .field(name)
                    .is_some_and(|field| field.kind == crate::config::kind::FieldKind::Path)
                {
                    let target = context
                        .workspace
                        .resolve(view.target.as_deref().unwrap_or("status.md"));
                    return Ok(crate::collaboration::references::path_cell(
                        context, &target, &raw,
                    ));
                }
                crate::terminal::value::cell(context, kind, name, &raw, display)
            })
            .collect::<Result<Vec<_>>>()?;
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    Ok(out)
}

/// A template is literal text, or a workspace relative `.md` file to read.
pub fn template_text(context: &Context, text: &str) -> Result<String> {
    let trimmed = text.trim();
    if !trimmed.ends_with(".md") {
        return Ok(text.to_string());
    }
    let path = context.workspace.resolve(trimmed);
    if !path.exists() {
        return Err(Error::usage(format!("template `{trimmed}` is missing")));
    }
    std::fs::read_to_string(&path).map_err(|error| crate::foundation::error::io_error(&path, error))
}
/// Render records through a line template, one record per line.
///
/// The template takes `{field}` placeholders and is expanded with raw stored
/// values, so `- {id} [{state}] {text}` reproduces a bullet list without
/// translating keys.
fn lines_block(context: &Context, kind: &Kind, view: &View) -> Result<String> {
    let template = view.template.clone().ok_or_else(|| {
        Error::usage(format!(
            "kind `{}` view `{}`: the lines renderer needs a template",
            kind.name, view.name
        ))
    })?;
    let template = template_text(context, &template)?;
    let keys = if view.sort.is_empty() {
        crate::query::order::default_keys(context, kind)?
    } else {
        view.sort.clone()
    };
    let records = crate::query::order::apply(context, kind, &keys, &rows_of(context, kind)?)?;
    let mut out = String::new();
    for record in &records {
        let mut line = template.clone();
        for field in &kind.fields {
            let value = record.get(&field.name).cloned().unwrap_or_default();
            line = line.replace(&format!("{{{}}}", field.name), &value);
        }
        out.push_str(line.trim_end());
        out.push('\n');
        if let Some(field) = &view.detail_field
            && let Some(value) = record.get(field)
            && !value.trim().is_empty()
        {
            let label = kind
                .field(field)
                .map(|item| item.display.clone())
                .unwrap_or_else(|| field.clone());
            out.push_str(&format!("  {label}: {}\n", value.trim()));
        }
    }
    Ok(out)
}
/// Render a composite view: its fragments in order, each under a heading.
fn composite_block(context: &Context, kind: &Kind, view: &View) -> Result<String> {
    if view.fragments.is_empty() {
        return Err(Error::usage(format!(
            "kind `{}` view `{}`: a composite view needs fragments",
            kind.name, view.name
        )));
    }
    let mut out = String::new();
    for name in &view.fragments {
        let fragment = kind
            .views
            .iter()
            .find(|item| &item.name == name)
            .ok_or_else(|| {
                Error::usage(format!(
                    "kind `{}` view `{}`: no fragment `{name}`",
                    kind.name, view.name
                ))
            })?;
        if let Some(title) = &fragment.title {
            out.push_str(&format!("### {title}\n\n"));
        }
        out.push_str(&block_text(context, kind, fragment)?);
        out.push('\n');
    }
    Ok(out)
}

/// Render one registry as a table.
fn registry_block(context: &Context, view: &View) -> Result<String> {
    let source = view
        .source
        .clone()
        .ok_or_else(|| Error::usage("a registry view needs a source registry name".to_string()))?;
    let registry = context.registry(&source)?;
    let columns = if view.columns.is_empty() {
        vec!["key".to_string(), "display".to_string()]
    } else {
        view.columns.clone()
    };
    let mut rows = Vec::new();
    for entry in registry.entries() {
        let cells: Vec<String> = columns
            .iter()
            .map(|column| match column.as_str() {
                "key" => entry.key.clone(),
                "display" => entry.display.clone(),
                other => entry.cells.get(other).cloned().unwrap_or_default(),
            })
            .collect();
        rows.push(cells);
    }
    Ok(grid_of(&columns, &view.headers, &rows))
}

/// Render the current focus: a file of identifiers plus their task titles.
fn focus_block(context: &Context, kind: &Kind, view: &View) -> Result<String> {
    let source = view.source.clone().ok_or_else(|| {
        Error::usage(format!(
            "kind `{}` view `{}`: a focus view needs a source file",
            kind.name, view.name
        ))
    })?;
    let path = context.workspace.resolve(&source);
    let ids: Vec<String> = if path.exists() {
        let body = std::fs::read_to_string(&path)
            .map_err(|error| crate::foundation::error::io_error(&path, error))?;
        crate::foundation::csv::parse(&body)?
            .into_iter()
            .map(|row| row.first().cloned().unwrap_or_default().trim().to_string())
            .filter(|cell| !cell.is_empty() && cell != "id")
            .collect()
    } else {
        Vec::new()
    };
    let records = rows_of(context, kind)?;
    let columns = if view.columns.is_empty() {
        vec!["id".to_string(), "title".to_string()]
    } else {
        view.columns.clone()
    };
    let mut rows = Vec::new();
    for id in ids {
        let Some(record) = records.iter().find(|record| {
            kind.key
                .first()
                .and_then(|name| record.get(name))
                .map(|value| value.trim() == id)
                .unwrap_or(false)
        }) else {
            continue;
        };
        rows.push(
            columns
                .iter()
                .map(|column| record.get(column).cloned().unwrap_or_default())
                .collect(),
        );
    }
    Ok(grid_of(&columns, &view.headers, &rows))
}

/// Build a markdown table from already rendered cells.
fn grid_of(columns: &[String], headers: &[String], rows: &[Vec<String>]) -> String {
    let titles: Vec<String> = if headers.len() == columns.len() {
        headers.to_vec()
    } else {
        columns.to_vec()
    };
    let mut out = String::new();
    out.push_str(&format!("| {} |\n", titles.join(" | ")));
    out.push_str(&format!(
        "|{}|\n",
        titles.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    ));
    for row in rows {
        out.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    out
}

/// Render one table from a CSV file that is not a kind's own table.
fn file_table_block(context: &Context, view: &View) -> Result<String> {
    let source = view
        .source
        .clone()
        .ok_or_else(|| Error::usage("a file table view needs a source file".to_string()))?;
    let path = context.workspace.resolve(&source);
    if !path.exists() {
        return Ok(String::new());
    }
    let body = std::fs::read_to_string(&path)
        .map_err(|error| crate::foundation::error::io_error(&path, error))?;
    let rows = crate::foundation::csv::parse(&body)?;
    let Some(headers) = rows.first() else {
        return Ok(String::new());
    };
    let columns: Vec<String> = if view.columns.is_empty() {
        headers.clone()
    } else {
        view.columns.clone()
    };
    let indexes: Vec<Option<usize>> = columns
        .iter()
        .map(|column| headers.iter().position(|header| header == column))
        .collect();
    let mut cells = Vec::new();
    for row in rows.iter().skip(1) {
        if row.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        cells.push(
            indexes
                .iter()
                .map(|index| {
                    index
                        .and_then(|position| row.get(position))
                        .cloned()
                        .unwrap_or_default()
                })
                .collect(),
        );
    }
    Ok(grid_of(&columns, &view.headers, &cells))
}

/// Render per-record detail blocks, linking path fields.
fn details_block(context: &Context, kind: &Kind, view: &View) -> Result<String> {
    let records = rows_of(context, kind)?;
    let columns: Vec<String> = if view.columns.is_empty() {
        kind.fields.iter().map(|field| field.name.clone()).collect()
    } else {
        view.columns.clone()
    };
    let mut out = String::new();
    for record in &records {
        let id = kind
            .key
            .first()
            .and_then(|name| record.get(name))
            .cloned()
            .unwrap_or_default();
        let lines: Vec<String> = columns
            .iter()
            .filter_map(|column| {
                let field = kind.field(column)?;
                let value = record.get(column).cloned().unwrap_or_default();
                if value.trim().is_empty() {
                    return None;
                }
                let shown = if field.kind == crate::config::kind::FieldKind::Path {
                    let name = value.rsplit('/').next().unwrap_or(&value).to_string();
                    format!("[{name}]({value})")
                } else {
                    value
                };
                Some(format!("{}: {shown}", field.display))
            })
            .collect();
        let mut lines = lines;
        if view.logs {
            for path in logs_mentioning(context, &id)? {
                let prefix = view
                    .logs_prefix
                    .clone()
                    .unwrap_or_else(|| "logs".to_string());
                lines.push(format!("{prefix}: {path}"));
            }
        }
        if lines.is_empty() {
            continue;
        }
        out.push_str(&format!("### {id}\n\n"));
        for line in lines {
            out.push_str(&line);
            out.push_str("\n\n");
        }
    }
    Ok(out)
}

/// Render several vocabularies as one table.
fn vocabularies_block(context: &Context, view: &View) -> Result<String> {
    if view.entries.is_empty() {
        return Err(Error::usage(
            "the vocabularies renderer needs at least one entry".to_string(),
        ));
    }
    let mut rows = Vec::new();
    for entry in &view.entries {
        let values: Vec<String> = if let Some(name) = &entry.registry {
            context.registry(name)?.keys()
        } else if let Some(name) = &entry.kind {
            let kind = context.catalog.require(name)?;
            let store = crate::storage::store::Store::open(&context.workspace, kind)?;
            match kind.key.first() {
                Some(first) => store.table.values(first),
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let mut seen: Vec<String> = Vec::new();
        for value in values {
            if !seen.contains(&value) {
                seen.push(value);
            }
        }
        rows.push(vec![
            entry.display.clone(),
            seen.join(&view.separator.clone().unwrap_or_else(|| ", ".to_string())),
            if entry.multi {
                view.multi_display
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "yes".to_string())
            } else {
                view.multi_display
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| "no".to_string())
            },
        ]);
    }
    let columns = vec![
        "display".to_string(),
        "values".to_string(),
        "multi".to_string(),
    ];
    Ok(grid_of(&columns, &view.headers, &rows))
}

/// Files under the log directory that mention an identifier.
fn logs_mentioning(context: &Context, id: &str) -> Result<Vec<String>> {
    let Some(kind) = context
        .catalog
        .find("logs")
        .filter(|kind| kind.document_dir().is_some())
    else {
        return Ok(Vec::new());
    };
    Ok(crate::storage::records::mentioned_logs(context, kind, id)?
        .into_iter()
        .map(|(path, _)| path)
        .collect())
}

/// Every record of a kind, whatever its storage.
fn rows_of(context: &Context, kind: &Kind) -> Result<Vec<crate::storage::store::Record>> {
    crate::storage::records::all(context, kind)
}

fn header_of(kind: &Kind, name: &str) -> String {
    kind.field(name)
        .map(|field| field.display.clone())
        .unwrap_or_else(|| name.to_string())
}

/// Replace the text inside one region.
pub fn replace_region(body: &str, region: &str, text: &str) -> Option<String> {
    let begin = begin_marker(region);
    let end = end_marker(region);
    let start = body.find(&begin)? + begin.len();
    let stop = body[start..].find(&end)? + start;
    let mut replacement = String::from("\n\n");
    replacement.push_str(text.trim_end());
    replacement.push_str("\n\n");
    let mut out = String::with_capacity(body.len() + text.len());
    out.push_str(&body[..start]);
    out.push_str(&replacement);
    out.push_str(&body[stop..]);
    Some(out)
}

pub fn begin_marker(region: &str) -> String {
    format!("<!-- clerk:begin {region} -->")
}

pub fn end_marker(region: &str) -> String {
    format!("<!-- clerk:end {region} -->")
}

/// Expand `{name}` placeholders from a record.
pub fn expand(template: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}').map(|offset| open + offset) else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &rest[open + 1..close];
        if let Some(value) = values.get(name) {
            out.push_str(value);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Resolve a target path inside the workspace.
pub fn target_path(context: &Context, target: &str) -> PathBuf {
    context.workspace.resolve(target)
}

/// Read a page, treating a missing file as empty.
pub fn read_page(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(path).map_err(|error| crate::foundation::error::io_error(path, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_only_the_region() {
        let body = "head\n<!-- clerk:begin tasks -->\nold\n<!-- clerk:end tasks -->\ntail\n";
        let updated = replace_region(body, "tasks", "| a |\n").expect("replaced");
        assert!(updated.contains("| a |"));
        assert!(!updated.contains("old"));
        assert!(updated.starts_with("head"));
        assert!(updated.ends_with("tail\n"));
    }

    #[test]
    fn expands_placeholders() {
        let mut values = BTreeMap::new();
        values.insert("id".to_string(), "APP-001".to_string());
        assert_eq!(expand("{id}.md", &values), "APP-001.md");
        assert_eq!(expand("fixed.md", &values), "fixed.md");
    }
}
