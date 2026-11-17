//! Loading the records of one kind, the same way everywhere.

use crate::config::kind::Kind;
use crate::execution::context::Context;
use crate::foundation::error::Result;
use std::path::Path;

use crate::storage::store::{Record, Store, Table};

/// The table behind a kind, or the archive table when asked.
pub fn table(context: &Context, kind: &Kind, archived: bool) -> Result<Table> {
    if !archived {
        return Ok(Store::open(&context.workspace, kind)?.table);
    }
    let path = context.workspace.resolve(&kind.archive_table());
    if path.exists() {
        Table::load(&path)
    } else {
        Ok(Table::empty())
    }
}

/// Every live record of a kind, whether it is stored in a table or as pages.
pub fn all(context: &Context, kind: &Kind) -> Result<Vec<Record>> {
    if kind.document_dir().is_some() {
        return crate::storage::document::records(context, kind);
    }
    let table = table(context, kind, false)?;
    Ok((0..table.rows.len())
        .map(|index| table.record(index))
        .collect())
}

pub fn including_archived(context: &Context, kind: &Kind) -> Result<Vec<Record>> {
    let mut records = all(context, kind)?;
    if kind.document_dir().is_some() {
        records.extend(crate::storage::document::records_in(
            context,
            kind,
            &kind.archive_dir(),
        )?);
    } else {
        let archive = table(context, kind, true)?;
        records.extend((0..archive.rows.len()).map(|index| archive.record(index)));
    }
    Ok(records)
}

pub fn contains(context: &Context, kind: &Kind, keys: &[String]) -> Result<bool> {
    Ok(keys.len() == kind.key.len()
        && including_archived(context, kind)?.iter().any(|record| {
            kind.key
                .iter()
                .zip(keys)
                .all(|(field, value)| record.get(field) == Some(value))
        }))
}

/// Every markdown file below a directory, in path order.
pub fn markdown_files(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|error| crate::foundation::error::io_error(&dir, error))?
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Identifier characters are ASCII: prose beside an ID does not extend it.
pub fn mentions(text: &str, id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let identifier = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_');
    text.match_indices(id).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(identifier)
            && !text[at + id.len()..].chars().next().is_some_and(identifier)
    })
}

/// The log declaration owns both current and archived log directories.
pub fn mentioned_logs(context: &Context, kind: &Kind, id: &str) -> Result<Vec<(String, String)>> {
    let mut files = markdown_files(
        &context
            .workspace
            .resolve(kind.document_dir().expect("log document kind")),
    )?;
    files.extend(markdown_files(
        &context.workspace.resolve(&kind.archive_dir()),
    )?);
    files.sort();
    files.dedup();
    let mut out = Vec::new();
    for path in files {
        let relative = context.workspace.relative(&path);
        if relative == context.workspace.layout.seal_log {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| crate::foundation::error::io_error(&path, e))?;
        if crate::collaboration::log_entry::parse(&text)?
            .iter()
            .any(|entry| entry.matches(None, id))
        {
            out.push((relative, text));
        }
    }
    Ok(out)
}
