//! Commit intent, validated before mutation and frozen before data acceptance.

use crate::config::workspace::{CommitMode, Workspace};
use crate::foundation::error::{Error, Result};
use crate::git::operation;

pub fn validate(text: &str) -> Result<String> {
    let normalized = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    if normalized
        .lines()
        .next()
        .is_none_or(|line| line.trim().is_empty())
        || normalized
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        || normalized
            .lines()
            .any(|line| line.trim_start().starts_with("Clerk-Operation:"))
    {
        return Err(Error::usage(
            "commit message needs a nonempty first line, no control characters and no Clerk-Operation trailer",
        ));
    }
    Ok(normalized.trim_end().to_string())
}

pub fn require(workspace: &Workspace) -> Result<()> {
    if workspace.policy.auto_commit
        && workspace.policy.commit_mode == CommitMode::Required
        && operation::current().is_none_or(|report| report.message.is_none())
    {
        return Err(Error::usage(
            "policy.commit_mode = required: supply --message or --message-file before writing",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Change {
    kind: String,
    verb: String,
    keys: Vec<String>,
}

pub fn record(kind: &str, verb: &str, keys: &[String]) {
    operation::change(|report| {
        report.actions.push(Change {
            kind: kind.into(),
            verb: verb.into(),
            keys: keys.to_vec(),
        })
    });
}

fn abbreviated(items: impl IntoIterator<Item = String>) -> String {
    let items: std::collections::BTreeSet<_> = items.into_iter().collect();
    let shown = items
        .iter()
        .take(5)
        .map(|item| {
            let clean: String = item
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .take(80)
                .collect();
            if item.chars().count() > 80 {
                clean + "..."
            } else {
                clean
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > 5 {
        format!("{shown}, +{} more", items.len() - 5)
    } else {
        shown
    }
}

/// Freeze a bounded change summary while rollback is still possible.
pub fn resolve(workspace: &Workspace, paths: &[String], verb: &str) -> Result<()> {
    if !workspace.policy.auto_commit {
        return Ok(());
    }
    require(workspace)?;
    let report = operation::current().ok_or_else(|| Error::io("missing operation receipt"))?;
    let mut objects = std::collections::BTreeMap::<(String, String), &str>::new();
    for action in &report.actions {
        let category = match action.verb.as_str() {
            "new" => "Added",
            "delete" => "Deleted",
            "archive" => "Archived",
            "restore" => "Restored",
            _ => "Updated",
        };
        for key in &action.keys {
            let entry = objects
                .entry((action.kind.clone(), key.clone()))
                .or_insert(category);
            if *entry != "Added" || category == "Deleted" {
                *entry = category;
            }
        }
    }
    let mut groups = std::collections::BTreeMap::<(&str, String), Vec<String>>::new();
    for ((kind, key), category) in &objects {
        groups
            .entry((*category, kind.clone()))
            .or_default()
            .push(key.clone());
    }
    let mut lines = groups
        .iter()
        .take(8)
        .map(|((category, kind), keys)| {
            format!(
                "{category} {kind} ({}): {}.",
                keys.len(),
                abbreviated(keys.clone())
            )
        })
        .collect::<Vec<_>>();
    if groups.len() > 8 {
        lines.push(format!("Plus {} other change groups.", groups.len() - 8));
    }
    if lines.is_empty() {
        lines.push(format!("Applied {verb} to managed files."));
    }
    let paths: std::collections::BTreeSet<_> = paths.iter().cloned().collect();
    let details = format!(
        "{}\n\nFiles ({}): {}.\n\nVerified managed-file validation before Git finalization.",
        lines.join("\n"),
        paths.len(),
        abbreviated(paths)
    );
    let message = match report.message {
        Some(message) => message,
        None if workspace.policy.commit_mode == CommitMode::Default => {
            let fixed = &workspace.policy.commit_message;
            if fixed.contains('\n') {
                fixed.clone()
            } else {
                format!("{fixed}\n\n{details}")
            }
        }
        None => {
            let added = objects
                .values()
                .filter(|category| **category == "Added")
                .count();
            let changed = objects.len() - added;
            let summary = match (added, changed) {
                (0, 0) => "update managed files".into(),
                (0, n) => format!("update {n} managed object(s)"),
                (n, 0) => format!("add {n} managed object(s)"),
                (a, c) => format!("add {a} and update {c} managed object(s)"),
            };
            format!("chore(clerk): {summary}\n\n{details}")
        }
    };
    let message = format!(
        "{}\n\nClerk-Operation: {}\n",
        validate(&message)?,
        report.id
    );
    operation::change(|report| report.message = Some(message));
    Ok(())
}
