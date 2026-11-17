//! One place decides how a stored value is shown.
//!
//! A key stays a key when display text is off, but a multi valued cell is
//! always written as a comma separated list, because `|` is the storage
//! separator rather than something a reader should see.

use crate::config::kind::{FieldKind, Kind};
use crate::execution::context::Context;
use crate::foundation::error::Result;

/// Text of one cell.
pub fn cell(
    context: &Context,
    kind: &Kind,
    name: &str,
    raw: &str,
    display: bool,
) -> Result<String> {
    cell_with_separator(context, kind, name, raw, display, ", ")
}

/// Presentation chooses spacing without changing stored values or display lookup.
pub fn cell_with_separator(
    context: &Context,
    kind: &Kind,
    name: &str,
    raw: &str,
    display: bool,
    separator: &str,
) -> Result<String> {
    if !display {
        return Ok(list(raw, separator));
    }
    let Some(field) = kind.field(name) else {
        return Ok(raw.to_string());
    };
    if field.kind != FieldKind::Enum {
        return Ok(if field.multi {
            list(raw, separator)
        } else {
            raw.to_string()
        });
    }
    let Some(registry_name) = &field.registry else {
        return Ok(raw.to_string());
    };
    let registry = context.registry(registry_name)?;
    Ok(if field.multi {
        raw.split('|')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(|item| registry.display_of(item))
            .collect::<Vec<_>>()
            .join(separator)
    } else if raw.trim().is_empty() {
        String::new()
    } else {
        registry.display_of(raw.trim())
    })
}

/// A multi valued cell written as a list.
fn list(raw: &str, separator: &str) -> String {
    if !raw.contains('|') {
        return raw.to_string();
    }
    raw.split('|')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join(separator)
}
