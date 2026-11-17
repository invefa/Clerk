//! One pass over the instances of a kind.
//!
//! A record table and a document directory hold the same thing: instances of
//! one kind. The checks that apply to an instance do not depend on where it is
//! stored, so the walk, the key uniqueness rule and the field rules live here
//! and the storage only decides how an instance is read.

use std::collections::BTreeMap;

use crate::config::kind::Kind;
use crate::execution::context::Context;
use crate::foundation::error::Result;
use crate::storage::store::Record;

/// One problem, placed where a reader can find it.
pub struct Problem {
    pub at: String,
    pub line: u32,
    pub message: String,
}

/// One instance of a kind, with the place it came from.
struct Site {
    at: String,
    line: u32,
    record: Record,
    page: Option<crate::storage::document::Page>,
}

fn sites(context: &Context, kind: &Kind) -> Result<Vec<Site>> {
    if kind.document_dir().is_some() {
        let mut out = Vec::new();
        for page in crate::storage::document::pages(context, kind)? {
            out.push(Site {
                at: context.workspace.relative(&page.path),
                line: 0,
                record: crate::storage::document::record_of(kind, &page),
                page: Some(page),
            });
        }
        return Ok(out);
    }
    let table = crate::storage::records::table(context, kind, false)?;
    let at = context
        .workspace
        .relative(&context.workspace.resolve(kind.table_path()));
    Ok((0..table.rows.len())
        .map(|index| Site {
            at: at.clone(),
            line: index as u32 + 2,
            record: table.record(index),
            page: None,
        })
        .collect())
}

/// Check every instance of a kind.
pub fn problems(context: &Context, kind: &Kind) -> Result<Vec<Problem>> {
    let sites = match sites(context, kind) {
        Ok(sites) => sites,
        Err(error) => {
            return Ok(vec![Problem {
                at: kind.source.clone(),
                line: 0,
                message: error.to_string(),
            }]);
        }
    };
    let mut out = Vec::new();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for site in &sites {
        let key = site_key(kind, site);
        let place = place(site);
        if let Some(previous) = seen.insert(key.clone(), place.clone()) {
            out.push(Problem {
                at: site.at.clone(),
                line: site.line,
                message: format!("key `{key}` is also used at {previous}"),
            });
        }
        if let Err(error) = crate::execution::engine::validate_record(context, kind, &site.record) {
            out.push(Problem {
                at: site.at.clone(),
                line: site.line,
                message: error.to_string(),
            });
        }
        if let Some(page) = &site.page {
            for (at, message) in
                crate::storage::document::structure_problems(context, kind, page, &site.at)?
            {
                out.push(Problem {
                    at,
                    line: 0,
                    message,
                });
            }
        }
    }
    if kind.document_dir().is_some() {
        for (at, message) in crate::storage::document::index_problems(context, kind)? {
            out.push(Problem {
                at,
                line: 0,
                message,
            });
        }
    }
    Ok(out)
}

/// Identity of an instance, the values of the declared key fields.
fn site_key(kind: &Kind, site: &Site) -> String {
    if let Some(page) = &site.page {
        return crate::storage::document::key_of(kind, page);
    }
    kind.key
        .iter()
        .map(|name| site.record.get(name).cloned().unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

fn place(site: &Site) -> String {
    if site.line > 0 {
        format!("{}:{}", site.at, site.line)
    } else {
        site.at.clone()
    }
}
