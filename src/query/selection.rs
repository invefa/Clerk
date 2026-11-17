//! Approximate identifiers belong only to explicit read entry points.

use crate::config::kind::Kind;
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::interface::cli::Args;
use crate::query::matching::Matcher;

pub fn resolve(
    context: &Context,
    kind: &Kind,
    keys: &[String],
    args: &Args,
    archived: bool,
) -> Result<Vec<String>> {
    if keys.len() != kind.key.len() {
        return Err(Error::usage(
            "read selection requires all declared key components",
        ));
    }
    let mode = args.value("match").unwrap_or("exact");
    let scope = args
        .value("data-scope")
        .unwrap_or(if archived { "all" } else { "current" });
    let records = if archived && args.value("data-scope").is_none() {
        crate::storage::records::including_archived(context, kind)?
    } else {
        crate::query::scope::load(context, kind, scope)?
            .into_iter()
            .map(|(record, _)| record)
            .collect::<Vec<_>>()
    };
    let key_of = |record: &crate::storage::store::Record| {
        kind.key
            .iter()
            .map(|field| record.get(field).cloned().unwrap_or_default())
            .collect::<Vec<_>>()
    };
    if mode != "regex" && records.iter().any(|record| key_of(record) == keys) {
        return Ok(keys.to_vec());
    }
    let matchers = keys
        .iter()
        .map(|key| Matcher::new(key, mode, args.value("case") != Some("sensitive")))
        .collect::<Result<Vec<_>>>()?;
    let mut candidates = records
        .iter()
        .filter(|record| {
            key_of(record)
                .iter()
                .zip(&matchers)
                .all(|(value, matcher)| matcher.is_match(value))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|record| key_of(record));
    candidates.dedup_by_key(|record| key_of(record));
    match candidates.as_slice() {
        [record] => Ok(key_of(record)),
        [] => Err(Error::validation(format!(
            "no `{}` key matches `{}`",
            kind.name,
            keys.join(" ")
        ))),
        _ => {
            let rows = candidates
                .iter()
                .map(|record| {
                    vec![
                        key_of(record).join(" "),
                        record.get("title").cloned().unwrap_or_default(),
                    ]
                })
                .collect::<Vec<_>>();
            Err(Error::conflict(format!(
                "ambiguous `{}` selection; use a complete key:\n{}",
                kind.name,
                crate::terminal::text::table(
                    Some(&["key".into(), "title".into()]),
                    &rows
                        .iter()
                        .map(|row| row
                            .iter()
                            .map(|cell| crate::terminal::text::single_line(cell))
                            .collect())
                        .collect::<Vec<_>>(),
                    0,
                    false
                )
            )))
        }
    }
}
