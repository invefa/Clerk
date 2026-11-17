//! Ordered, idempotent mutations shared by commands and workflows.

use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::storage::store::Table;

#[derive(Clone, Copy)]
pub enum Change {
    Set,
    Add,
    Remove,
}

impl Change {
    pub fn parse(verb: &str) -> Result<Self> {
        match verb {
            "set" => Ok(Self::Set),
            "add" => Ok(Self::Add),
            "remove" => Ok(Self::Remove),
            _ => Err(Error::usage("focus expects show, set, add or remove")),
        }
    }
}

pub struct Outcome {
    pub keys: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub unchanged: Vec<String>,
    pub affected: Vec<String>,
}

pub fn keys(context: &Context) -> Result<Vec<String>> {
    let path = context.workspace.registry_dir().join("focus.csv");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let table = Table::load(&path)?;
    if table.headers != ["key"] || table.rows.iter().any(|row| row.len() != 1) {
        return Err(Error::validation("focus.csv must have a single key column"));
    }
    Ok(table.rows.into_iter().map(|row| row[0].clone()).collect())
}

/// The caller holds the write transaction before reading the latest collection.
pub fn apply(context: &Context, change: Change, inputs: &[String]) -> Result<Outcome> {
    let before = keys(context)?;
    let mut requested = Vec::new();
    for input in inputs {
        if input.trim().is_empty() || input != input.trim() || input.contains('|') {
            return Err(Error::validation(
                "focus keys must be nonempty individual keys",
            ));
        }
        if !requested.contains(input) {
            requested.push(input.clone());
        }
    }
    if !matches!(change, Change::Remove) {
        let mut known = std::collections::BTreeSet::new();
        for kind in context
            .catalog
            .kinds()
            .iter()
            .filter(|kind| kind.key.len() == 1)
        {
            for record in crate::storage::records::all(context, kind)? {
                if let Some(key) = record.get(&kind.key[0]) {
                    known.insert(key.clone());
                }
            }
        }
        for key in &requested {
            if !known.contains(key) {
                return Err(Error::validation(format!("unknown live focus key `{key}`")));
            }
        }
    }
    let mut after = match change {
        Change::Set => requested.clone(),
        _ => before.clone(),
    };
    match change {
        Change::Add => {
            for key in &requested {
                if !after.contains(key) {
                    after.push(key.clone());
                }
            }
        }
        Change::Remove => after.retain(|key| !requested.contains(key)),
        Change::Set => (),
    }
    let mut outcome = Outcome {
        added: after
            .iter()
            .filter(|key| !before.contains(key))
            .cloned()
            .collect(),
        removed: before
            .iter()
            .filter(|key| !after.contains(key))
            .cloned()
            .collect(),
        unchanged: requested
            .iter()
            .filter(|key| before.contains(key) == after.contains(key))
            .cloned()
            .collect(),
        keys: after.clone(),
        affected: Vec::new(),
    };
    if after != before {
        let path = context.workspace.registry_dir().join("focus.csv");
        Table {
            headers: vec!["key".into()],
            rows: after.into_iter().map(|key| vec![key]).collect(),
        }
        .save_to(&path)?;
        outcome.affected.push(context.workspace.relative(&path));
        crate::git::commit_message::record("focus", "add", &outcome.added);
        crate::git::commit_message::record("focus", "remove", &outcome.removed);
    }
    Ok(outcome)
}
