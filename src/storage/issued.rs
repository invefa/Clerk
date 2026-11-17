//! Permanent reservations for generated identifiers, including removed records.

use crate::config::kind::Kind;
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::storage::store::Table;

fn path(context: &Context) -> std::path::PathBuf {
    context
        .workspace
        .resolve(&context.workspace.layout.seal)
        .with_file_name("issued.csv")
}

fn load(context: &Context) -> Result<Table> {
    let path = path(context);
    if path.exists() {
        Table::load(&path)
    } else {
        Ok(Table {
            headers: vec!["kind".into(), "key".into()],
            rows: Vec::new(),
        })
    }
}

/// Historical keys supplement the current storage when allocating a new key.
pub fn keys(context: &Context, kind: &Kind) -> Result<Vec<String>> {
    let ledger = load(context)?;
    let mut keys = Vec::new();
    for row in &ledger.rows {
        if row.first() == Some(&kind.name)
            && let Some(key) = row.get(1)
        {
            keys.push(key.clone());
        }
    }
    if kind.document_dir().is_some() {
        keys.extend(
            crate::storage::document::records_in(context, kind, &kind.archive_dir())?
                .iter()
                .filter_map(|row| kind.key.first().and_then(|key| row.get(key)).cloned()),
        );
    } else {
        let archived = crate::storage::records::table(context, kind, true)?;
        if let Some(first) = kind.key.first() {
            keys.extend(archived.values(first));
        }
    }
    Ok(keys)
}

/// Remember every live and archived identifier before destructive operations.
pub fn remember(context: &Context, kind: &Kind, additional: &[String]) -> Result<Option<String>> {
    if kind.numbering.is_none() {
        return Ok(None);
    }
    let mut keys = keys(context, kind)?;
    keys.extend(
        crate::storage::records::all(context, kind)?
            .iter()
            .filter_map(|record| kind.key.first().and_then(|key| record.get(key)).cloned()),
    );
    keys.extend_from_slice(additional);
    keys.sort();
    keys.dedup();
    let mut ledger = load(context)?;
    if ledger.headers != ["kind", "key"] {
        return Err(Error::validation("issued.csv must have kind,key columns"));
    }
    let before = ledger.rows.len();
    for key in keys {
        let row = vec![kind.name.clone(), key];
        if !ledger.rows.contains(&row) {
            ledger.rows.push(row);
        }
    }
    if ledger.rows.len() == before {
        return Ok(None);
    }
    ledger.rows.sort();
    let path = path(context);
    ledger.save_to(&path)?;
    Ok(Some(context.workspace.relative(&path)))
}
