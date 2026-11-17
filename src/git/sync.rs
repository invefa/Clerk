//! Sync: comparing the local branch with its remote counterpart.
//!
//! The gate runs before a write command. When the remote branch holds commits
//! the local branch does not have, the write is refused so that a quick push
//! cannot diverge silently.

use std::path::Path;
use std::process::Command;

use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Failure, Result};

/// What the comparison found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub reference: String,
    /// The remote branch is not known locally.
    pub unknown: bool,
    pub behind: u64,
    pub ahead: u64,
}

/// Compare the local branch with the configured remote branch.
pub fn check(workspace: &Workspace) -> Result<Report> {
    let remote = &workspace.sync.remote;
    let branch = &workspace.sync.branch;
    let reference = format!("{remote}/{branch}");
    let mut report = Report {
        reference: reference.clone(),
        ..Report::default()
    };
    if !reference_exists(workspace.root(), &reference) {
        report.unknown = true;
        return Ok(report);
    }
    report.behind = count(workspace.root(), &format!("HEAD..{reference}"))?;
    report.ahead = count(workspace.root(), &format!("{reference}..HEAD"))?;
    Ok(report)
}

/// Refuse a write when the remote is ahead.
pub fn gate(workspace: &Workspace) -> Result<()> {
    if !workspace.sync.enabled {
        return Ok(());
    }
    // Offline or a missing remote: with fail_closed off the comparison still
    // runs on the last fetched reference.
    if workspace.sync.write_fetch
        && let Err(error) = fetch_cached(workspace)
        && workspace.sync.fail_closed
    {
        return Err(error);
    }
    let report = check(workspace)?;
    if report.unknown {
        if workspace.sync.fail_closed {
            return Err(Error::new(
                Failure::Stale,
                format!(
                    "remote branch {} is not known locally; run `clerk sync fetch`",
                    report.reference
                ),
            ));
        }
        return Ok(());
    }
    if report.behind > 0 {
        return Err(Error::new(
            Failure::Stale,
            format!(
                "{} commit(s) on {} are not in the local branch; business data was not written; run `clerk sync update` for a clean fast-forward or resolve the Git state manually",
                report.behind, report.reference
            ),
        ));
    }
    Ok(())
}

/// Fetch unless a recent fetch is still valid.
pub fn fetch_cached(workspace: &Workspace) -> Result<()> {
    let seconds = workspace.sync.cache_seconds;
    if seconds > 0 {
        let stamp = crate::execution::transaction::local_dir(workspace)?.join("sync.stamp");
        if let Ok(metadata) = std::fs::metadata(&stamp)
            && let Ok(modified) = metadata.modified()
            && let Ok(age) = modified.elapsed()
            && age.as_secs() < seconds
        {
            return Ok(());
        }
        return fetch(workspace);
    }
    fetch(workspace)
}
/// Fetch the configured remote so the comparison uses fresh data.
pub fn fetch(workspace: &Workspace) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace.root())
        .args(["fetch", &workspace.sync.remote])
        .output()
        .map_err(|error| Error::io(format!("cannot run git: {error}")))?;
    if !output.status.success() {
        return Err(Error::io(format!(
            "git fetch {} failed: {}",
            workspace.sync.remote,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    if workspace.sync.cache_seconds > 0 {
        let path = crate::execution::transaction::local_dir(workspace)?.join("sync.stamp");
        std::fs::write(&path, "")
            .map_err(|error| crate::foundation::error::io_error(&path, error))?;
    }
    Ok(())
}

fn reference_exists(root: &Path, reference: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "--quiet", reference])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn count(root: &Path, range: &str) -> Result<u64> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-list", "--count", range])
        .output()
        .map_err(|error| Error::io(format!("cannot run git: {error}")))?;
    if !output.status.success() {
        return Err(Error::io(format!(
            "cannot compare Git history: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse::<u64>()
        .map_err(|_| Error::io("git returned an invalid revision count"))
}

/// Safe synchronization never stashes, merges divergent history or touches submodules.
pub fn update(workspace: &Workspace) -> Result<()> {
    let _lock = crate::execution::transaction::WriterLock::acquire(workspace)?;
    update_locked(workspace)
}

pub(crate) fn update_locked(workspace: &Workspace) -> Result<()> {
    let root = workspace.root();
    let refuse = |reason: String| {
        Error::conflict(format!(
            "safe sync stopped: {reason}; business data was not written"
        ))
    };
    let branch = git(root, &["branch", "--show-current"])?;
    if branch.trim() != workspace.sync.branch {
        return Err(refuse(format!(
            "expected branch `{}`, found `{}`",
            workspace.sync.branch,
            branch.trim()
        )));
    }
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = git(root, &["rev-parse", "--git-path", marker])?;
        if root.join(path.trim()).exists() {
            return Err(refuse(format!("Git operation `{marker}` is in progress")));
        }
    }
    let before = git(root, &["rev-parse", "HEAD"])?;
    let dirty = git(
        root,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    if !dirty.is_empty() {
        return Err(refuse(format!("working tree is dirty:\n{dirty}")));
    }
    fetch(workspace)
        .map_err(|error| Error::io(format!("{error}; business data was not written")))?;
    let report = check(workspace)?;
    if report.unknown {
        return Err(refuse(format!("{} is unknown", report.reference)));
    }
    if report.behind > 0 && report.ahead > 0 {
        return Err(refuse(
            "local and remote history diverged; merge manually".into(),
        ));
    }
    if report.behind > 0 {
        if before != git(root, &["rev-parse", "HEAD"])?
            || !git(
                root,
                &[
                    "status",
                    "--porcelain",
                    "--untracked-files=all",
                    "--ignore-submodules=none",
                ],
            )?
            .is_empty()
        {
            return Err(refuse("working tree or HEAD changed during fetch".into()));
        }
        git(
            root,
            &[
                "-c",
                "submodule.recurse=false",
                "merge",
                "--ff-only",
                &report.reference,
            ],
        )?;
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| Error::io(error.to_string()))?;
    if !out.status.success() {
        return Err(Error::io(format!(
            "git failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Normalize a remote URL to a case-insensitive repository identity.
pub fn normalise(remote: &str) -> String {
    let trimmed = remote.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let parts: Vec<&str> = trimmed
        .split(['/', ':'])
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() >= 2 {
        format!(
            "{}/{}",
            parts[parts.len() - 2].to_ascii_lowercase(),
            parts[parts.len() - 1].to_ascii_lowercase()
        )
    } else {
        trimmed.to_ascii_lowercase()
    }
}

#[cfg(test)]
mod remote_tests {
    #[test]
    fn normalises_remotes() {
        assert_eq!(
            super::normalise("git@github.com:Example/Library.git"),
            "example/library"
        );
        assert_eq!(
            super::normalise("https://github.com/example/library/"),
            "example/library"
        );
    }
}
