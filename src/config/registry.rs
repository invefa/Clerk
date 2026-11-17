//! Controlled value tables.
//!
//! A registry is a CSV file with a `key` column and optional `display` and extra
//! columns. Keys are compared by machine; display text is shown to people. Adding a
//! value means editing the table, never the engine.

use std::collections::BTreeMap;
use std::path::Path;

use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};

/// One row of a registry.
#[derive(Clone, Debug)]
pub struct Entry {
    pub key: String,
    pub display: String,
    pub cells: BTreeMap<String, String>,
}

/// A loaded value table.
#[derive(Clone, Debug)]
pub struct Registry {
    pub name: String,
    entries: Vec<Entry>,
    /// Whether the file carries a `display` column, which makes it a domain.
    domain: bool,
}

impl Entry {
    pub fn new(key: impl Into<String>, display: impl Into<String>) -> Self {
        let key = key.into();
        let display = display.into();
        let mut cells = BTreeMap::new();
        cells.insert("key".to_string(), key.clone());
        cells.insert("display".to_string(), display.clone());
        Entry {
            key,
            display,
            cells,
        }
    }
}

impl Registry {
    pub fn from_entries(name: impl Into<String>, entries: Vec<Entry>) -> Self {
        Registry {
            name: name.into(),
            entries,
            domain: false,
        }
    }

    pub fn load(workspace: &Workspace, name: &str) -> Result<Self> {
        let path = workspace.registry_dir().join(format!("{name}.csv"));
        if !path.exists() {
            return Err(Error::usage(format!(
                "registry `{name}` not found at {}",
                workspace.relative(&path)
            )));
        }
        Self::read(&path, name)
    }

    pub fn read(path: &Path, name: &str) -> Result<Self> {
        let body = std::fs::read_to_string(path)
            .map_err(|error| crate::foundation::error::io_error(path, error))?;
        Self::parse(&body, name)
    }

    /// Build a registry from CSV text.
    pub fn parse(body: &str, name: &str) -> Result<Self> {
        let table = crate::storage::store::Table::from_text(body)?;
        Self::from_table(table, name)
    }

    /// Build a registry from an already parsed table.
    pub fn from_table(table: crate::storage::store::Table, name: &str) -> Result<Self> {
        let headers = &table.headers;
        if headers.is_empty() {
            return Ok(Registry {
                name: name.to_string(),
                entries: Vec::new(),
                domain: false,
            });
        }
        // The first column is the identity: `key` for a domain, `alias` for a
        // mapping, `term` or `person` for a ledger. A domain adds `display`.
        let key_at = 0;
        let display_at = headers.iter().position(|header| header == "display");
        let mut entries = Vec::new();
        for row in &table.rows {
            if row.iter().all(|cell| cell.trim().is_empty()) {
                continue;
            }
            let key = row.get(key_at).map(|value| value.trim()).unwrap_or("");
            if key.is_empty() {
                return Err(Error::usage(format!(
                    "registry `{name}`: a row has an empty key"
                )));
            }
            let display = display_at
                .and_then(|index| row.get(index))
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| key.to_string());
            let mut cells = BTreeMap::new();
            for (index, header) in headers.iter().enumerate() {
                if let Some(value) = row.get(index) {
                    cells.insert(header.clone(), value.trim().to_string());
                }
            }
            entries.push(Entry {
                key: key.to_string(),
                display,
                cells,
            });
        }
        Ok(Registry {
            name: name.to_string(),
            entries,
            domain: display_at.is_some(),
        })
    }

    /// Whether this registry is a domain, that is, whether it carries display text.
    pub fn is_domain(&self) -> bool {
        self.domain
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn keys(&self) -> Vec<String> {
        self.entries.iter().map(|entry| entry.key.clone()).collect()
    }

    pub fn displays(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.display.clone())
            .collect()
    }

    pub fn has(&self, key: &str) -> bool {
        self.entries.iter().any(|entry| entry.key == key)
    }

    pub fn find(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.key == key)
    }

    /// The key for exactly this key. Display text and aliases are resolved
    /// person can type either form; storage always keeps the key.
    pub fn key_of(&self, value: &str) -> Option<String> {
        let trimmed = value.trim();
        if let Some(entry) = self.entries.iter().find(|entry| entry.key == trimmed) {
            return Some(entry.key.clone());
        }
        None
    }

    pub fn display_of(&self, key: &str) -> String {
        self.find(key)
            .map(|entry| entry.display.clone())
            .unwrap_or_else(|| key.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_keys_and_display_text() {
        let registry =
            Registry::parse("key,display\ndone,完成\nactive,进行中\n", "states").expect("loaded");
        assert_eq!(registry.keys(), vec!["done", "active"]);
        assert_eq!(registry.key_of("done"), Some("done".to_string()));
        // Display text is not identity and does not resolve to a key.
        assert_eq!(registry.key_of("完成"), None);
        assert_eq!(registry.display_of("active"), "进行中");
    }

    #[test]
    fn keeps_extra_columns() {
        let registry = Registry::parse(
            "key,display,remotes\ncode,服务端,github.com/example/library-app\n",
            "repos",
        )
        .expect("loaded");
        let entry = registry.find("code").expect("entry");
        assert_eq!(
            entry.cells.get("remotes").map(String::as_str),
            Some("github.com/example/library-app")
        );
    }
}
