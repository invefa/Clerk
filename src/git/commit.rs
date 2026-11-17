//! Automatic commit of managed files.
//!
//! A command that writes data seals the affected files and commits exactly
//! those paths. The commit is skipped when the tree already carried managed
//! changes when the command started, so a colleague's pending work is never
//! folded into an unrelated commit.

use std::path::Path;

use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};

/// Managed files that were already dirty when a command started.
#[derive(Clone, Debug, Default)]
pub struct Baseline {
    dirty: Vec<String>,
}

impl Baseline {
    pub fn dirty(&self) -> &[String] {
        &self.dirty
    }
}

/// Record the state before a write command.
pub fn begin(workspace: &Workspace) -> Result<Baseline> {
    Ok(Baseline {
        dirty: if workspace.policy.auto_commit {
            dirty_paths(workspace.root())?
        } else {
            Vec::new()
        },
    })
}

pub(crate) fn dirty_paths(root: &Path) -> Result<Vec<String>> {
    let output = git(
        root,
        &["status", "--porcelain", "-z", "--untracked-files=all"],
    )?;
    let mut paths = Vec::new();
    let mut entries = output.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if let Some(path) = entry.get(3..) {
            paths.push(path.to_string());
        }
        if entry
            .as_bytes()
            .get(..2)
            .is_some_and(|status| status.contains(&b'R') || status.contains(&b'C'))
        {
            entries.next();
        }
    }
    Ok(paths)
}

pub fn require_clean(workspace: &Workspace) -> Result<()> {
    let paths = dirty_paths(workspace.root())?;
    if !paths.is_empty() {
        return Err(Error::conflict(format!(
            "workflow requires a clean working tree; existing changes: {}",
            paths.join(", ")
        )));
    }
    Ok(())
}

/// Commit the given paths when the policy allows it.
pub fn finish(
    workspace: &Workspace,
    baseline: &Baseline,
    paths: &[String],
    _verb: &str,
) -> Result<Option<String>> {
    if !workspace.policy.auto_commit || paths.is_empty() {
        crate::git::operation::change(|report| {
            report.commit = crate::git::operation::Stage::Skipped
        });
        return Ok(None);
    }
    if !baseline.dirty.is_empty() {
        crate::git::operation::change(|report| {
            report.commit = crate::git::operation::Stage::Skipped
        });
        return Err(Error::conflict(format!(
            "auto-commit skipped: managed files were already dirty: {}",
            baseline.dirty.join(", ")
        )));
    }
    // A region key such as `status.md#tasks` seals part of a file; git stages
    // the file itself.
    let mut files: Vec<String> = Vec::new();
    for path in paths {
        let file = path.split('#').next().unwrap_or(path).to_string();
        let tracked = git(
            workspace.root(),
            &["ls-files", "--error-unmatch", "--", &file],
        )
        .is_ok();
        if (workspace.resolve(&file).exists() || tracked) && !files.contains(&file) {
            files.push(file);
        }
    }
    if files.is_empty() {
        return Ok(None);
    }
    let mut add = vec!["add".to_string(), "--".to_string()];
    add.extend(files.clone());
    run_add(workspace, &add)?;
    let changed = git(workspace.root(), &["diff", "--cached", "--name-only", "-z"])?;
    if changed
        .split(char::from(0))
        .filter(|path| !path.is_empty())
        .any(|path| !files.iter().any(|file| file == path))
    {
        return Err(Error::io(
            "Git index changed outside this operation; commit stopped",
        ));
    }
    if changed.trim().is_empty() {
        crate::git::operation::change(|report| {
            report.commit = crate::git::operation::Stage::Skipped
        });
        return Ok(None);
    }

    let message = crate::git::operation::current()
        .and_then(|report| report.message)
        .ok_or_else(|| {
            Error::conflict("operation has no recorded commit message; inspect Git manually")
        })?;
    crate::git::operation::change(|report| report.commit = crate::git::operation::Stage::Pending);
    crate::git::operation::save(workspace)?;
    if let Err(error) = git(
        workspace.root(),
        &["commit", "--cleanup=verbatim", "-m", &message],
    ) {
        crate::git::operation::change(|report| {
            report.commit = crate::git::operation::Stage::Failed
        });
        return Err(error);
    }
    let head = git(workspace.root(), &["rev-parse", "--short", "HEAD"])?;
    crate::git::operation::change(|report| {
        report.commit = crate::git::operation::Stage::Succeeded;
        report.head = head_full(workspace).unwrap_or_default();
    });
    crate::git::operation::save(workspace)?;
    if workspace.policy.auto_push {
        let branch = git(workspace.root(), &["branch", "--show-current"])?;
        if branch.trim() != workspace.sync.branch {
            return Err(Error::io(format!(
                "commit {} was saved on branch `{}`; auto-push requires configured branch `{}`",
                head.trim(),
                branch.trim(),
                workspace.sync.branch
            )));
        }
        push(workspace)?;
    }

    Ok(Some(head.trim().to_string()))
}

fn run_add(workspace: &Workspace, arguments: &[String]) -> Result<()> {
    let mut command = std::process::Command::new("git");
    command.arg("-C").arg(workspace.root());
    for argument in arguments {
        command.arg(argument);
    }
    let output = command
        .output()
        .map_err(|error| Error::io(format!("cannot run git: {error}")))?;
    if !output.status.success() {
        return Err(Error::io(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// Short hash of the current commit, empty when the repository has none.
pub fn head(workspace: &Workspace) -> Option<String> {
    let text = git(workspace.root(), &["rev-parse", "--short", "HEAD"]).ok()?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn git(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .map_err(|error| Error::io(format!("cannot run git: {error}")))?;
    if !output.status.success() {
        return Err(Error::io(format!(
            "git {} failed: {}",
            arguments.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Full identity for resumable operation receipts.
pub fn head_full(workspace: &Workspace) -> Option<String> {
    git(workspace.root(), &["rev-parse", "HEAD"])
        .ok()
        .map(|text| text.trim().into())
}

pub fn push(workspace: &Workspace) -> Result<()> {
    crate::git::operation::change(|report| report.push = crate::git::operation::Stage::Pending);
    crate::git::operation::save(workspace)?;
    match git(
        workspace.root(),
        &[
            "push",
            &workspace.sync.remote,
            &format!("HEAD:refs/heads/{}", workspace.sync.branch),
        ],
    ) {
        Ok(_) => {
            crate::git::operation::change(|report| {
                report.push = crate::git::operation::Stage::Succeeded
            });
            Ok(())
        }
        Err(error) => {
            crate::git::operation::change(|report| {
                report.push = crate::git::operation::Stage::Unknown
            });
            // A disconnected client cannot know whether the remote accepted a push.
            Err(error)
        }
    }
}

/// Retry Git finalization only; business verbs are never replayed here.
pub fn retry(workspace: &Workspace, id: Option<&str>) -> Result<crate::git::operation::Report> {
    use crate::git::operation::{self, Stage};
    let _lock = crate::execution::transaction::WriterLock::acquire(workspace)?;
    let mut report = operation::load(workspace, id)?;
    if report.data != operation::Data::Saved || report.commit == Stage::Skipped {
        return Err(Error::conflict(
            "operation was not accepted for automatic Git finalization; inspect it manually",
        ));
    }
    if report.commit != Stage::Succeeded && report.message.is_none() {
        return Err(Error::conflict(
            "legacy operation has no recorded commit message; inspect Git and commit manually",
        ));
    }
    let current = head_full(workspace).unwrap_or_default();
    if report.commit != Stage::Succeeded {
        let message = git(workspace.root(), &["log", "-1", "--format=%B"]).unwrap_or_default();
        if message
            .lines()
            .any(|line| line == format!("Clerk-Operation: {}", report.id))
        {
            report.commit = Stage::Succeeded;
            report.head = current.clone();
        } else if current != report.before {
            return Err(Error::conflict(
                "HEAD changed after this operation; inspect Git history manually",
            ));
        }
    }
    if report.commit == Stage::Succeeded && current != report.head {
        return Err(Error::conflict(
            "HEAD changed since the recorded commit; retry stopped",
        ));
    }
    for (path, expected) in &report.files {
        if path
            .split(['/', char::from(92)])
            .any(|part| part == ".." || part.eq_ignore_ascii_case(".git"))
            || std::path::Path::new(path).is_absolute()
            || path.starts_with(char::from(92))
            || path.contains(':')
        {
            return Err(Error::usage("invalid path in operation receipt"));
        }
        if operation::digest(workspace, path)? != *expected {
            return Err(Error::conflict(format!(
                "operation file `{path}` has changed; retry stopped"
            )));
        }
    }
    if dirty_paths(workspace.root())?
        .iter()
        .any(|path| !report.files.contains_key(path))
    {
        return Err(Error::conflict(
            "unrelated local changes exist; retry stopped",
        ));
    }
    let branch = git(workspace.root(), &["branch", "--show-current"])?;
    if branch.trim() != workspace.sync.branch {
        return Err(Error::conflict("retry requires the configured branch"));
    }
    operation::set(report.clone());
    if report.commit != Stage::Succeeded {
        let mut policy = workspace.clone();
        policy.policy.auto_commit = true;
        let paths = report.files.keys().cloned().collect::<Vec<_>>();
        if let Err(error) = finish(&policy, &Baseline::default(), &paths, &report.verb) {
            operation::save(workspace)?;
            return Err(error);
        }
    } else if workspace.policy.auto_push && report.push != Stage::Succeeded {
        // A previous network error may have happened after the server accepted it.
        let remote = git(
            workspace.root(),
            &[
                "ls-remote",
                "--heads",
                &workspace.sync.remote,
                &format!("refs/heads/{}", workspace.sync.branch),
            ],
        )?;
        if remote.split_whitespace().next() == Some(&report.head) {
            operation::change(|report| report.push = Stage::Succeeded);
        } else if let Err(error) = push(workspace) {
            operation::save(workspace)?;
            return Err(error);
        }
    }
    operation::save(workspace)?;
    Ok(operation::current().expect("operation receipt"))
}
