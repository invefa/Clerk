//! Ordering rows for display.
//!
//! The order belongs to a view, not to the data file, so a list and a page can
//! group the same rows differently. A key is either a column name or
//! `field:part`, where the part is a token of the numbering scheme the
//! identifier field uses, such as `id:prefix` or `id:serial`.

use std::collections::BTreeMap;

use crate::config::kind::{FieldKind, Kind};
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::storage::store::Record;

/// One resolved sort key.
enum Key {
    Column(String),
    Part { field: String, part: String },
}

fn keys_of(context: &Context, kind: &Kind, keys: &[String]) -> Result<Vec<Key>> {
    let mut out = Vec::new();
    for key in keys {
        let Some((field, part)) = key.split_once(':') else {
            if kind.field(key).is_none() {
                let names: Vec<String> = kind.fields.iter().map(|item| item.name.clone()).collect();
                return Err(Error::usage(format!(
                    "no column `{key}`; columns: {}",
                    names.join(", ")
                )));
            }
            out.push(Key::Column(key.clone()));
            continue;
        };
        let scheme_name = kind
            .numbering
            .as_deref()
            .ok_or_else(|| Error::usage("sort key parts require a declared numbering scheme"))?;
        let scheme = context.numbering(scheme_name)?;
        let known = part == scheme.counter || scheme.tokens.contains_key(part);
        if !known {
            let mut names = vec![scheme.counter.clone()];
            names.extend(scheme.tokens.keys().cloned());
            return Err(Error::usage(format!(
                "kind `{}`: sort key `{key}` asks for an unknown part; parts: {}",
                kind.name,
                names.join(", ")
            )));
        }
        out.push(Key::Part {
            field: field.to_string(),
            part: part.to_string(),
        });
    }
    Ok(out)
}

/// The order a numbered kind falls back to: its key parts, as declared.
pub fn default_keys(context: &Context, kind: &Kind) -> Result<Vec<String>> {
    let (Some(scheme_name), Some(field)) = (kind.numbering.as_deref(), kind.key.first()) else {
        return Ok(Vec::new());
    };
    let scheme = context.numbering(scheme_name)?;
    Ok(scheme
        .part_names()
        .into_iter()
        .map(|part| format!("{field}:{part}"))
        .collect())
}
/// Sort value of a column.
///
/// An enum column whose registry carries an `order` column sorts by that
/// number, so a domain can decide the business order without changing the
/// keys. A multi valued cell takes the smallest order among its values, and an
/// unknown value sorts after every known one.
fn column_value(context: &Context, kind: &Kind, name: &str, record: &Record) -> Result<String> {
    let raw = record.get(name).cloned().unwrap_or_default();
    let Some(field) = kind.field(name) else {
        return Ok(raw);
    };
    if field.kind != FieldKind::Enum {
        return Ok(raw);
    }
    let Some(registry_name) = &field.registry else {
        return Ok(raw);
    };
    let registry = context.registry(registry_name)?;
    // Without an order column the keys themselves decide the order.
    if !registry
        .entries()
        .iter()
        .any(|entry| entry.cells.contains_key("order"))
    {
        return Ok(if raw.contains('|') {
            raw.split('|')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
                .join(",")
        } else {
            raw
        });
    }
    let mut orders: Vec<u64> = Vec::new();
    for value in raw.split('|') {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let key = registry.key_of(value).unwrap_or_else(|| value.to_string());
        if let Some(entry) = registry.find(&key)
            && let Some(order) = entry.cells.get("order")
            && let Ok(number) = order.trim().parse::<u64>()
        {
            orders.push(number);
        }
    }
    match orders.iter().min() {
        Some(order) => Ok(format!("{order:06}")),
        None if raw.trim().is_empty() => Ok(String::new()),
        None => Ok("999999".to_string()),
    }
}
/// Return the records in the declared order.
pub fn apply(
    context: &Context,
    kind: &Kind,
    keys: &[String],
    records: &[Record],
) -> Result<Vec<Record>> {
    if keys.is_empty() {
        return Ok(records.to_vec());
    }
    // Ties fall back to the identifier, so a sort by a column still walks the
    // keys in their declared order inside each group.
    let mut all = keys.to_vec();
    for key in default_keys(context, kind)? {
        if !all.contains(&key) {
            all.push(key);
        }
    }
    let keys = keys_of(context, kind, &all)?;
    let registries = context.numbering_registries()?;
    let scheme = kind
        .numbering
        .as_deref()
        .map(|name| context.numbering(name))
        .transpose()?;
    let mut rows: Vec<(Vec<String>, Record)> = Vec::new();
    for record in records {
        let mut values = Vec::new();
        for key in &keys {
            let value = match key {
                Key::Column(name) => column_value(context, kind, name, record)?,
                Key::Part { field, part } => {
                    let raw = record.get(field).cloned().unwrap_or_default();
                    let tokens: BTreeMap<String, String> = scheme
                        .expect("part key has a numbering scheme")
                        .tokens_of_key(&raw, &registries)?
                        .unwrap_or_default();
                    tokens.get(part).cloned().unwrap_or_default()
                }
            };
            values.push(value);
        }
        rows.push((values, record.clone()));
    }
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(rows.into_iter().map(|(_, record)| record).collect())
}
