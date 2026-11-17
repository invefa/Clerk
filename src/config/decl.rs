//! Strict reads of declaration files.
//!
//! Declarations reject unknown keys so that a misspelled key fails loudly
//! instead of being ignored.

pub use ::toml::Value;
pub use ::toml::value::Table;

use crate::foundation::error::{Error, Result};

/// Fail when the table contains a key outside the allowed set.
pub fn only_keys(table: &Table, allowed: &[&str], context: &str) -> Result<()> {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(Error::usage(format!(
                "{context}: unknown key `{key}`; allowed: {}",
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

pub fn text(table: &Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

pub fn required_text(table: &Table, key: &str, context: &str) -> Result<String> {
    text(table, key).ok_or_else(|| {
        Error::usage(format!(
            "{context}: `{key}` is required and must be a string"
        ))
    })
}

pub fn flag(table: &Table, key: &str) -> Option<bool> {
    table.get(key).and_then(|value| value.as_bool())
}

pub fn integer(table: &Table, key: &str) -> Option<i64> {
    table.get(key).and_then(|value| value.as_integer())
}

pub fn strings(table: &Table, key: &str) -> Vec<String> {
    table
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn array_of_tables(table: &Table, key: &str) -> Vec<Table> {
    table
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_table())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub fn sub_table(table: &Table, key: &str) -> Option<Table> {
    table.get(key).and_then(|value| value.as_table()).cloned()
}

/// Parse a document and return its top level table.
pub fn parse(body: &str, context: &str) -> Result<Table> {
    let value: ::toml::Value = body
        .parse()
        .map_err(|error| Error::usage(format!("{context}: {error}")))?;
    value
        .as_table()
        .cloned()
        .ok_or_else(|| Error::usage(format!("{context}: top level must be a table")))
}
