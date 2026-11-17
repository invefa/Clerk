//! Record provenance stays separate from business fields.

use crate::config::kind::Kind;
use crate::execution::{
    context::Context,
    engine::{Options, Outcome},
};
use crate::foundation::error::{Error, Result};
use crate::storage::{document, records, store::Record};
use std::collections::BTreeSet;

pub fn key(kind: &Kind, record: &Record) -> String {
    // Length framing avoids collisions in composite keys.
    kind.key
        .iter()
        .map(|field| {
            let value = record.get(field).map(String::as_str).unwrap_or("");
            format!("{}:{value}", value.len())
        })
        .collect()
}

pub fn load(context: &Context, kind: &Kind, scope: &str) -> Result<Vec<(Record, bool)>> {
    let scope = if scope.is_empty() { "current" } else { scope };
    let mut output = Vec::new();
    for archived in [false, true] {
        if (scope == "current" && archived) || (scope == "archived" && !archived) {
            continue;
        }
        let rows = if let Some(dir) = kind.document_dir() {
            document::records_in(
                context,
                kind,
                &if archived {
                    kind.archive_dir()
                } else {
                    dir.into()
                },
            )?
        } else {
            let table = records::table(context, kind, archived)?;
            (0..table.rows.len()).map(|i| table.record(i)).collect()
        };
        output.extend(rows.into_iter().map(|record| (record, archived)));
    }
    let mut seen = BTreeSet::new();
    for (record, _) in &output {
        if !seen.insert(key(kind, record)) {
            return Err(Error::conflict(format!(
                "duplicate `{}` identity in selected scope; use --data-scope current or --data-scope archived and inspect the conflicting records",
                kind.name
            )));
        }
    }
    Ok(output)
}

pub fn read(
    context: &mut Context,
    kind: &Kind,
    verb: &str,
    keys: &[String],
    options: &Options,
) -> Result<Outcome> {
    let mut result = Outcome::new(kind, verb);
    result.scope = Some(if options.scope.is_empty() {
        "current".into()
    } else {
        options.scope.clone()
    });
    if verb == "show" {
        result.columns = kind.fields.iter().map(|field| field.name.clone()).collect();
    }
    let keep = crate::query::filters::Keep::load(
        context,
        kind,
        options.filter.as_deref(),
        &options.filters,
        options.mine,
        options.claimed,
        options.case.as_deref(),
    )?;
    for (record, archived) in load(context, kind, &options.scope)? {
        if verb == "show" {
            if kind
                .key
                .iter()
                .map(|field| record.get(field).cloned().unwrap_or_default())
                .collect::<Vec<_>>()
                != keys
            {
                continue;
            }
        } else if !keep.keep(context, kind, &record)? {
            continue;
        }
        if archived {
            result.archived_keys.insert(key(kind, &record));
        }
        result.records.push(record);
    }
    if verb == "show" && result.records.is_empty() {
        return Err(Error::validation(format!(
            "unknown `{}` key `{}` in scope {}",
            kind.name,
            keys.join(" "),
            options.scope
        )));
    }
    Ok(result)
}
