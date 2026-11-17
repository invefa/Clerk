//! The write pipeline: what happens after data changed on disk.
//!
//! Sealing and committing live here so that every entry point shares one
//! definition of the write guarantee. A verb only mutates its own files and
//! reports them; the pipeline decides what is sealed and what is committed.

use crate::config::workspace::Level;
use crate::execution::context::Context;
use crate::execution::engine::Outcome;
use crate::foundation::error::{Error, Result};
use crate::foundation::time;
use crate::git::commit::{self, Baseline};
use crate::storage::seal::Seals;

/// Record the tree state before a write command.
pub struct Write {
    baseline: Baseline,
    transaction: crate::execution::transaction::Transaction,
    pub require_check: bool,
}

pub fn begin(context: &mut Context) -> Result<Write> {
    begin_with(context, false)
}

pub fn begin_reseal(context: &mut Context) -> Result<Write> {
    begin_with(context, true)
}

fn begin_with(context: &mut Context, reseal: bool) -> Result<Write> {
    if crate::git::operation::current()
        .is_none_or(|report| report.data != crate::git::operation::Data::NotWritten)
    {
        crate::git::operation::begin(&context.workspace);
    }
    let lock = crate::execution::transaction::WriterLock::acquire(&context.workspace)?;
    let transaction = crate::execution::transaction::Transaction::with_lock(lock)?;
    let root = context.workspace.root().to_string_lossy().to_string();
    *context = Context::open(Some(&root))?;
    crate::git::commit_message::require(&context.workspace)?;
    crate::git::operation::change(|report| {
        report.before = crate::git::commit::head_full(&context.workspace).unwrap_or_default()
    });
    crate::git::sync::gate(&context.workspace)?;
    if !reseal {
        let problems = Seals::load(&context.workspace)?.verify(&context.workspace);
        if !problems.is_empty() {
            return Err(Error::conflict(format!(
                "existing seal drift: {}; restore the files or use `clerk seal --note <reason>`",
                problems
                    .iter()
                    .map(|problem| format!("{}: {}", problem.key, problem.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
    }
    crate::git::operation::change(|report| report.data = crate::git::operation::Data::RolledBack);
    Ok(Write {
        baseline: commit::begin(&context.workspace)?,
        transaction,
        require_check: false,
    })
}

/// Seal the affected paths and commit them when the policy allows.
pub fn finish(context: &mut Context, mut outcome: Outcome, baseline: Write) -> Result<Outcome> {
    if outcome.dry_run {
        return Ok(outcome);
    }
    let removed = outcome.removed.clone();
    let notes = finish_paths(
        context,
        &outcome.verb,
        &mut outcome.affected,
        &removed,
        baseline,
    )?;
    outcome.notes.extend(notes);
    Ok(outcome)
}

/// Seal and commit a set of paths, returning the notes to report.
pub fn finish_paths(
    context: &mut Context,
    verb: &str,
    affected: &mut Vec<String>,
    removed: &[String],
    mut baseline: Write,
) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    let identity = context.identity()?.clone();

    let removed: Vec<String> = removed
        .iter()
        .filter(|path| !context.workspace.resolve(path).exists())
        .cloned()
        .collect();
    affected.retain(|path| !removed.contains(path));

    let sealed = sealable(context, affected);
    let pruned = sealable(context, &removed);
    if !sealed.is_empty() || !pruned.is_empty() {
        let stamp = time::stamp(time::now_epoch(), context.workspace.policy.offset_hours);
        Seals::update(&context.workspace, &sealed, &pruned, &identity, &stamp)?;
        if !sealed.is_empty() {
            notes.push(format!("sealed {}", sealed.join(", ")));
        }
        if !pruned.is_empty() {
            notes.push(format!("unsealed {}", pruned.join(", ")));
        }
        affected.push(context.workspace.layout.seal.clone());
    }

    affected.extend(removed.iter().cloned());
    affected.sort();
    affected.dedup();
    if baseline.require_check {
        let problems = crate::execution::checks::problems(context)?;
        if !problems.is_empty() {
            return Err(Error::validation(
                problems
                    .iter()
                    .map(|(file, line, message)| format!("{file}:{line}: {message}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
    }
    crate::git::commit_message::resolve(&context.workspace, affected, verb)?;
    crate::git::operation::prepare(&context.workspace, affected, verb)?;
    let (id, receipt) = crate::git::operation::accepted_receipt()?;
    let accepted = baseline
        .transaction
        .accept_with_receipt(Some((&id, &receipt)));
    if baseline.transaction.is_accepted() {
        crate::git::operation::change(|report| report.data = crate::git::operation::Data::Saved);
    }
    accepted?;
    crate::git::operation::save(&context.workspace)?;
    match commit::finish(&context.workspace, &baseline.baseline, affected, verb) {
        Ok(Some(head)) => notes.push(format!("committed {head}")),
        Ok(None) => {}
        Err(error) if error.failure() == crate::foundation::error::Failure::Conflict => {
            notes.push(format!("warning: {error}"));
        }
        Err(error) => {
            crate::git::operation::change(|report| {
                if report.commit == crate::git::operation::Stage::NotRun {
                    report.commit = crate::git::operation::Stage::Failed;
                }
            });
            crate::git::operation::save(&context.workspace)?;
            return Err(Error::new(
                error.failure(),
                format!(
                    "local data was saved, but Git did not finish: {error}; inspect git status and HEAD before retrying"
                ),
            ));
        }
    }
    crate::git::operation::save(&context.workspace)?;
    if let Some(report) = crate::git::operation::current() {
        notes.push(report.summary());
    }
    Ok(notes)
}

/// Expand affected paths into seal keys according to the declared rules.
fn sealable(context: &Context, affected: &[String]) -> Vec<String> {
    let mut keys = Vec::new();
    for path in affected {
        let Some(rule) = context.workspace.rule_for(path) else {
            continue;
        };
        if rule.level != Level::Sealed {
            continue;
        }
        if rule.regions.is_empty() {
            keys.push(path.clone());
        } else {
            for region in &rule.regions {
                keys.push(format!("{path}#{region}"));
            }
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

/// Validate a dry-run workspace with the same temporary seal changes as a real write.
pub fn check_paths(context: &mut Context, affected: &[String], removed: &[String]) -> Result<()> {
    let identity = context.identity()?.clone();
    let stamp = time::stamp(time::now_epoch(), context.workspace.policy.offset_hours);
    let affected: Vec<_> = affected
        .iter()
        .filter(|path| context.workspace.resolve(path).exists())
        .cloned()
        .collect();
    Seals::update(
        &context.workspace,
        &sealable(context, &affected),
        &sealable(context, removed),
        &identity,
        &stamp,
    )?;
    let problems = crate::execution::checks::problems(context)?;
    if !problems.is_empty() {
        return Err(Error::validation(
            problems
                .iter()
                .map(|(file, line, message)| format!("{file}:{line}: {message}"))
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    Ok(())
}
