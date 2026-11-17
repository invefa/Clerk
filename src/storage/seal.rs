//! Seal: the hash baseline of managed files and page regions.
//!
//! The table records one hash per managed key. A key is either a workspace
//! relative file or `file#region`, where the region is the text between the
//! begin and end markers of that name.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::config::identity::Identity;
use crate::config::workspace::Workspace;
use crate::foundation::csv;
use crate::foundation::error::{Error, Result};

const HEADERS: [&str; 5] = ["key", "sha256", "sealed_at", "by", "by_email"];

/// One recorded baseline.
#[derive(Clone, Debug)]
pub struct Entry {
    pub sha256: String,
    pub sealed_at: String,
    pub by: String,
    pub by_email: String,
}

/// The seal table of a workspace.
#[derive(Clone, Debug, Default)]
pub struct Seals {
    entries: BTreeMap<String, Entry>,
}

impl Seals {
    pub fn load(workspace: &Workspace) -> Result<Self> {
        let path = workspace.resolve(&workspace.layout.seal);
        if !path.exists() {
            return Ok(Seals::default());
        }
        let body = std::fs::read_to_string(&path)
            .map_err(|error| crate::foundation::error::io_error(&path, error))?;
        let rows = csv::parse(&body)?;
        if rows.is_empty() {
            return Ok(Seals::default());
        }
        let headers = &rows[0];
        let at = |name: &str| headers.iter().position(|header| header == name);
        // Older tables name the identifier column `file`; take the first
        // column when `key` is absent, the same rule the registries use.
        let key_at = at("key").unwrap_or(0);
        let hash_at = at("sha256").ok_or_else(|| {
            Error::usage(format!(
                "{}: missing `sha256` column",
                workspace.layout.seal
            ))
        })?;
        let stamp_at = at("sealed_at");
        let by_at = at("by");
        let email_at = at("by_email");
        let mut entries = BTreeMap::new();
        for row in &rows[1..] {
            if row.iter().all(|cell| cell.trim().is_empty()) {
                continue;
            }
            let key = row.get(key_at).cloned().unwrap_or_default();
            if key.trim().is_empty() {
                continue;
            }
            entries.insert(
                key,
                Entry {
                    sha256: row.get(hash_at).cloned().unwrap_or_default(),
                    sealed_at: stamp_at
                        .and_then(|index| row.get(index))
                        .cloned()
                        .unwrap_or_default(),
                    by: by_at
                        .and_then(|index| row.get(index))
                        .cloned()
                        .unwrap_or_default(),
                    by_email: email_at
                        .and_then(|index| row.get(index))
                        .cloned()
                        .unwrap_or_default(),
                },
            );
        }
        Ok(Seals { entries })
    }

    pub fn save(&self, workspace: &Workspace) -> Result<()> {
        let path = workspace.resolve(&workspace.layout.seal);
        if let Some(parent) = path.parent() {
            crate::execution::transaction::create_dir_all(parent)?;
        }
        let mut rows = vec![HEADERS.iter().map(|name| name.to_string()).collect()];
        for (key, entry) in &self.entries {
            rows.push(vec![
                key.clone(),
                entry.sha256.clone(),
                entry.sealed_at.clone(),
                entry.by.clone(),
                entry.by_email.clone(),
            ]);
        }
        crate::execution::transaction::write(&path, csv::serialize(&rows))?;
        Ok(())
    }

    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        &self.entries
    }

    /// Update the table: record the listed keys, drop the removed ones.
    pub fn update(
        workspace: &Workspace,
        record: &[String],
        prune: &[String],
        identity: &Identity,
        stamp: &str,
    ) -> Result<Vec<String>> {
        let mut seals = Seals::load(workspace)?;
        for key in prune {
            seals.entries.remove(key);
        }
        for key in record {
            let hash = hash_key(workspace, key)?;
            seals.entries.insert(
                key.clone(),
                Entry {
                    sha256: hash,
                    sealed_at: stamp.to_string(),
                    by: identity.git_name.clone(),
                    by_email: identity.git_email.clone(),
                },
            );
        }
        seals.save(workspace)?;
        Ok(record.to_vec())
    }

    /// Compare every recorded key with the current content.
    pub fn verify(&self, workspace: &Workspace) -> Vec<Problem> {
        let mut problems = Vec::new();
        for (key, entry) in &self.entries {
            match hash_key(workspace, key) {
                Ok(current) => {
                    if current != entry.sha256 {
                        problems.push(Problem {
                            key: key.clone(),
                            message: "content differs from the seal".to_string(),
                        });
                    }
                }
                Err(error) => problems.push(Problem {
                    key: key.clone(),
                    message: error.to_string(),
                }),
            }
        }
        problems
    }
}

/// A seal problem.
#[derive(Clone, Debug)]
pub struct Problem {
    pub key: String,
    pub message: String,
}

/// Hash the content addressed by a key.
pub fn hash_key(workspace: &Workspace, key: &str) -> Result<String> {
    let content = content_of(workspace, key)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

fn content_of(workspace: &Workspace, key: &str) -> Result<Vec<u8>> {
    let (relative, region) = match key.split_once('#') {
        Some((path, region)) => (path, Some(region)),
        None => (key, None),
    };
    let path = workspace.resolve(relative);
    if !path.exists() {
        return Err(Error::usage(format!("{relative} is missing")));
    }
    let bytes =
        std::fs::read(&path).map_err(|error| crate::foundation::error::io_error(&path, error))?;
    let Some(region) = region else {
        return Ok(bytes);
    };
    let text = String::from_utf8_lossy(&bytes).to_string();
    let inner = region_text(&text, region)
        .ok_or_else(|| Error::usage(format!("{relative}: no region `{region}`")))?;
    Ok(inner.into_bytes())
}

/// Extract the text between the markers of one region.
pub fn region_text(body: &str, region: &str) -> Option<String> {
    let begin = format!("<!-- clerk:begin {region} -->");
    let end = format!("<!-- clerk:end {region} -->");
    let start = body.find(&begin)? + begin.len();
    let stop = body[start..].find(&end)? + start;
    Some(body[start..stop].to_string())
}

/// Every key that the declared sealed paths cover.
pub fn targets(workspace: &Workspace) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    for rule in workspace.rules_at(crate::config::workspace::Level::Sealed) {
        if rule.regions.is_empty() {
            let path = workspace.resolve(&rule.path);
            if path.is_file() {
                keys.push(rule.path.clone());
            } else if path.is_dir() {
                for file in files_below(&path)? {
                    keys.push(workspace.relative(&file));
                }
            }
        } else {
            for region in &rule.regions {
                keys.push(format!("{}#{region}", rule.path));
            }
        }
    }
    keys.sort();
    keys.dedup();
    Ok(keys)
}

fn files_below(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)
            .map_err(|error| crate::foundation::error::io_error(&current, error))?
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Append one line to the seal log.
pub fn note(workspace: &Workspace, identity: &Identity, stamp: &str, reason: &str) -> Result<()> {
    let path = workspace.resolve(&workspace.layout.seal_log);
    if let Some(parent) = path.parent() {
        crate::execution::transaction::create_dir_all(parent)?;
    }
    let line = format!(
        "- {stamp} {} <{}>: {reason}\n",
        identity.git_name, identity.git_email
    );
    crate::execution::transaction::append(&path, &line)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_region_between_markers() {
        let body = "head\n<!-- clerk:begin tasks -->\nrow\n<!-- clerk:end tasks -->\ntail\n";
        assert_eq!(
            region_text(body, "tasks").map(|text| text.trim().to_string()),
            Some("row".to_string())
        );
        assert!(region_text(body, "missing").is_none());
    }
}
