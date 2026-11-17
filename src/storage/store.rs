//! Table storage: one row per instance.
//!
//! The table keeps the file column order, so rewriting a table preserves the
//! shape of the file and keeps diffs small.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::kind::Kind;
use crate::config::workspace::Workspace;
use crate::foundation::csv;
use crate::foundation::error::{Error, Result};

/// A record is a field name to cell value map.
pub type Record = BTreeMap<String, String>;

/// One CSV table.
#[derive(Clone, Debug)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn load(path: &Path) -> Result<Self> {
        let body = std::fs::read_to_string(path)
            .map_err(|error| crate::foundation::error::io_error(path, error))?;
        Table::from_text(&body)
    }

    /// Build a table from CSV text.
    pub fn from_text(body: &str) -> Result<Self> {
        let mut rows = csv::parse(body)?;
        if rows.is_empty() {
            return Ok(Table::empty());
        }
        let headers = rows.remove(0);
        rows.retain(|row| row.iter().any(|cell| !cell.trim().is_empty()));
        Ok(Table { headers, rows })
    }

    /// Write the table to a CSV file.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            crate::execution::transaction::create_dir_all(parent)?;
        }
        let mut rows = Vec::with_capacity(self.rows.len() + 1);
        if !self.headers.is_empty() {
            rows.push(self.headers.clone());
        }
        rows.extend(self.rows.iter().cloned());
        crate::execution::transaction::write(path, csv::serialize(&rows))?;
        Ok(())
    }

    pub fn empty() -> Self {
        Table {
            headers: Vec::new(),
            rows: Vec::new(),
        }
    }

    pub fn column(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|header| header == name)
    }

    /// Add missing columns at the end, keeping the existing order.
    pub fn ensure_columns(&mut self, names: &[String]) {
        for name in names {
            if self.column(name).is_none() {
                self.headers.push(name.clone());
                for row in &mut self.rows {
                    row.push(String::new());
                }
            }
        }
    }

    pub fn record(&self, index: usize) -> Record {
        let mut record = Record::new();
        if let Some(row) = self.rows.get(index) {
            for (position, header) in self.headers.iter().enumerate() {
                record.insert(
                    header.clone(),
                    row.get(position).cloned().unwrap_or_default(),
                );
            }
        }
        record
    }

    pub fn apply(&mut self, index: usize, record: &Record) {
        let headers = self.headers.clone();
        if let Some(row) = self.rows.get_mut(index) {
            row.resize(headers.len(), String::new());
            for (position, header) in headers.iter().enumerate() {
                row[position] = record.get(header).cloned().unwrap_or_default();
            }
        }
    }

    pub fn push(&mut self, record: &Record) {
        let mut row = Vec::with_capacity(self.headers.len());
        for header in &self.headers {
            row.push(record.get(header).cloned().unwrap_or_default());
        }
        self.rows.push(row);
    }

    pub fn remove(&mut self, index: usize) -> Option<Vec<String>> {
        if index < self.rows.len() {
            Some(self.rows.remove(index))
        } else {
            None
        }
    }

    /// Find a row whose key columns equal the given values.
    pub fn find(&self, key: &[String], values: &[String]) -> Option<usize> {
        if key.len() != values.len() {
            return None;
        }
        let indexes: Vec<usize> = key.iter().filter_map(|name| self.column(name)).collect();
        if indexes.len() != key.len() {
            return None;
        }
        self.rows.iter().position(|row| {
            indexes.iter().enumerate().all(|(position, column)| {
                row.get(*column).map(|cell| cell.trim()) == Some(values[position].trim())
            })
        })
    }

    /// Values of one column, used for numbering and duplicate checks.
    pub fn values(&self, column: &str) -> Vec<String> {
        match self.column(column) {
            Some(index) => self
                .rows
                .iter()
                .filter_map(|row| row.get(index).cloned())
                .filter(|cell| !cell.trim().is_empty())
                .collect(),
            None => Vec::new(),
        }
    }
}

/// A table bound to its kind and path.
pub struct Store<'a> {
    workspace: &'a Workspace,
    kind: &'a Kind,
    path: PathBuf,
    pub table: Table,
}

impl<'a> Store<'a> {
    pub fn open(workspace: &'a Workspace, kind: &'a Kind) -> Result<Self> {
        let path = workspace.resolve(kind.table_path());
        let mut table = if path.exists() {
            Table::load(&path)?
        } else {
            Table::empty()
        };
        Self::prepare(kind, &mut table)?;
        Ok(Store {
            workspace,
            kind,
            path,
            table,
        })
    }

    fn prepare(kind: &Kind, table: &mut Table) -> Result<()> {
        if table.headers.is_empty() {
            table.headers = kind.fields.iter().map(|field| field.name.clone()).collect();
            return Ok(());
        }
        let missing: Vec<String> = kind
            .fields
            .iter()
            .map(|field| field.name.clone())
            .filter(|name| table.column(name).is_none())
            .collect();
        table.ensure_columns(&missing);
        let declared: Vec<&str> = kind
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        let unknown: Vec<String> = table
            .headers
            .iter()
            .filter(|header| !declared.contains(&header.as_str()))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            return Err(Error::usage(format!(
                "{}: table column(s) {} are not declared by kind `{}`",
                kind.source,
                unknown.join(", "),
                kind.name
            )));
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn relative(&self) -> String {
        self.workspace.relative(&self.path)
    }

    pub fn save(&self) -> Result<()> {
        if self.path.exists() {
            let before = Table::load(&self.path)?;
            for index in 0..self.table.rows.len() {
                let after = self.table.record(index);
                let keys = self
                    .kind
                    .key
                    .iter()
                    .map(|key| after.get(key).cloned().unwrap_or_default())
                    .collect::<Vec<_>>();
                if let Some(old) = before.find(&self.kind.key, &keys) {
                    crate::config::kind::validate_change(self.kind, &before.record(old), &after)?;
                }
            }
        }
        if let Some(parent) = self.path.parent() {
            crate::execution::transaction::create_dir_all(parent)?;
        }
        let mut rows = Vec::with_capacity(self.table.rows.len() + 1);
        rows.push(self.table.headers.clone());
        rows.extend(self.table.rows.iter().cloned());
        let body = csv::serialize(&rows);
        crate::execution::transaction::write(&self.path, body)?;
        Ok(())
    }

    pub fn keys(&self) -> Vec<String> {
        let Some(first) = self.kind.key.first() else {
            return Vec::new();
        };
        self.table.values(first)
    }

    pub fn find(&self, values: &[String]) -> Option<usize> {
        self.table.find(&self.kind.key, values)
    }
}
