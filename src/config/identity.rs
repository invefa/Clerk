//! Identity: the git account, the person it maps to, and the operator name.
//!
//! The git account is authoritative for every recorded author field. The person
//! comes from a mapping table so that several accounts collapse into one name.
//! The operator is a local name and never decides ownership.

use std::path::Path;

use crate::config::registry::Registry;
use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};

/// Resolved identity for the current process.
#[derive(Clone, Debug, Default)]
pub struct Identity {
    pub git_name: String,
    pub git_email: String,
    pub person: Option<String>,
    pub operator: Option<String>,
}

impl Identity {
    pub fn resolve(workspace: &Workspace, override_name: Option<&str>) -> Result<Self> {
        let git_name = git_config(workspace.root(), "user.name").unwrap_or_default();
        let git_email = git_config(workspace.root(), "user.email").unwrap_or_default();
        if git_name.is_empty() || git_email.is_empty() {
            return Err(Error::usage(
                "git identity is missing; set `git config user.name` and `git config user.email`",
            ));
        }
        let person = person_of(workspace, &git_email)?;
        let operator = match override_name {
            Some(value) => Some(value.to_string()),
            None => read_operator(&operator_path(workspace)?)?,
        };
        Ok(Identity {
            git_name,
            git_email,
            person,
            operator,
        })
    }

    /// The best human readable name: the mapped person, else the git name.
    pub fn display(&self) -> String {
        self.person.clone().unwrap_or_else(|| self.git_name.clone())
    }
}

/// Read one git setting from the workspace directory.
pub fn git_config(root: &Path, key: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("config")
        .arg("--get")
        .arg(key)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn person_of(workspace: &Workspace, email: &str) -> Result<Option<String>> {
    let path = workspace.registry_dir().join("person.csv");
    if !path.exists() {
        return Ok(None);
    }
    let registry = Registry::read(&path, "person")?;
    for entry in registry.entries() {
        let candidate = entry
            .cells
            .get("email")
            .map(String::as_str)
            .unwrap_or(&entry.key);
        if candidate.eq_ignore_ascii_case(email) {
            if let Some(person) = entry.cells.get("person") {
                return Ok(Some(person.clone()));
            }
            return Ok(Some(entry.key.clone()));
        }
    }
    Ok(None)
}

fn read_operator(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let body = std::fs::read_to_string(path)
        .map_err(|error| crate::foundation::error::io_error(path, error))?;
    let value = body.trim();
    if value.is_empty() {
        return Ok(None);
    }
    Ok(Some(value.to_string()))
}

/// Write the operator name for this clone.
pub fn write_operator(workspace: &Workspace, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::usage("operator name must not be empty"));
    }
    if name.contains(char::is_whitespace) {
        return Err(Error::usage("operator name must not contain spaces"));
    }
    let _lock = crate::execution::transaction::WriterLock::acquire(workspace)?;
    let path = operator_path(workspace)?;
    let _identity_lock =
        crate::execution::transaction::identity_lock(path.parent().expect("identity directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| crate::foundation::error::io_error(parent, error))?;
    }
    crate::execution::transaction::atomic_write(&path, format!("{name}\n").as_bytes())?;
    Ok(())
}

fn operator_path(workspace: &Workspace) -> Result<std::path::PathBuf> {
    Ok(crate::execution::transaction::local_dir(workspace)?.join("identity"))
}
