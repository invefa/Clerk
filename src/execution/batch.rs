//! Local plan collection, with the shared workflow executor as its only writer.

use crate::collaboration::workflow::Step;
use crate::execution::context::Context;
use crate::execution::engine::Values;
use crate::foundation::error::{Error, Result, io_error};
use crate::interface::cli::Args;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use toml::{Table, Value};

fn hash(text: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(text.as_ref()))
}

fn path(context: &Context, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(Error::usage(
            "batch name needs 1 to 64 ASCII letters, digits, hyphens or underscores",
        ));
    }
    let local = crate::execution::transaction::local_dir(&context.workspace)?;
    if local
        == context
            .workspace
            .root()
            .canonicalize()
            .map_err(|e| io_error(context.workspace.root(), e))?
            .join(".work")
            .join("clerk")
    {
        return Err(Error::usage("local batch drafts require a Git checkout"));
    }
    Ok(local.join("batches").join(format!("{name}.toml")))
}

fn load(context: &Context, name: &str) -> Result<Table> {
    let path = path(context, name)?;
    let text = std::fs::read_to_string(&path).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound {
        Error::usage(format!("batch `{name}` not found; successful drafts are removed automatically; inspect operation status for executed work"))
    } else { io_error(&path,e) })?;
    let table = crate::config::decl::parse(&text, "batch draft")?;
    crate::config::decl::only_keys(
        &table,
        &["version", "id", "name", "owner", "baseline", "plan"],
        "batch draft",
    )?;
    if table.get("version").and_then(Value::as_integer) != Some(1) {
        return Err(Error::usage("batch draft requires version = 1"));
    }
    for key in ["id", "name", "owner"] {
        string(&table, key)?;
    }
    plan(&table)?;
    table
        .get("baseline")
        .and_then(Value::as_table)
        .ok_or_else(|| Error::usage("draft baseline is missing"))?;
    Ok(table)
}

fn string<'a>(table: &'a Table, key: &str) -> Result<&'a str> {
    table
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::usage(format!("batch draft requires `{key}`")))
}

fn plan(table: &Table) -> Result<&Table> {
    table
        .get("plan")
        .and_then(Value::as_table)
        .ok_or_else(|| Error::usage("draft plan is missing"))
}

pub(crate) fn table(steps: &[Step]) -> Table {
    let mut plan = Table::new();
    plan.insert("version".into(), Value::Integer(1));
    let entries = steps
        .iter()
        .map(|step| {
            let mut table = Table::new();
            table.insert("verb".into(), Value::String(step.verb.clone()));
            if let Some(kind) = &step.kind {
                table.insert("kind".into(), Value::String(kind.clone()));
            }
            if let Some(alias) = &step.alias {
                table.insert("as".into(), Value::String(alias.clone()));
            }
            if !step.keys.is_empty() {
                table.insert(
                    "keys".into(),
                    Value::Array(step.keys.iter().cloned().map(Value::String).collect()),
                );
            }
            for (name, values) in [("values", &step.values), ("tokens", &step.tokens)] {
                if !values.is_empty() {
                    table.insert(
                        name.into(),
                        Value::Table(
                            values
                                .iter()
                                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                                .collect(),
                        ),
                    );
                }
            }
            if let Some(body) = &step.body {
                table.insert("body".into(), Value::String(body.clone()));
            }
            if step.force {
                table.insert("force".into(), Value::Boolean(true));
            }
            Value::Table(table)
        })
        .collect();
    plan.insert("step".into(), Value::Array(entries));
    plan
}

pub(crate) fn print_results(entries: &[Value]) {
    for entry in entries {
        let number = entry.get("number").and_then(Value::as_integer).unwrap_or(0);
        let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("page");
        let verb = entry.get("verb").and_then(Value::as_str).unwrap_or("");
        let keys = entry
            .get("keys")
            .and_then(Value::as_array)
            .map(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        println!("{number}. {kind} {verb} {keys}");
    }
}

pub(crate) fn preview(context: &Context, steps: &[Step], json: bool) -> Result<i32> {
    crate::collaboration::workflow::validate_plan(context, steps)?;
    let plan = table(steps);
    if json {
        println!("{}", serde_json::to_string(&plan).expect("plan JSON"));
    } else {
        println!("Preview: {} step(s); nothing executed\n{plan}", steps.len());
    }
    Ok(0)
}

fn tree(root: &Path, base: &Path, files: &mut Table) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    if root.is_file() {
        let bytes = std::fs::read(root).map_err(|e| io_error(root, e))?;
        files.insert(
            root.strip_prefix(base)
                .unwrap_or(root)
                .to_string_lossy()
                .replace('\\', "/"),
            Value::String(hash(bytes)),
        );
    } else {
        for entry in std::fs::read_dir(root).map_err(|e| io_error(root, e))? {
            let entry = entry.map_err(|e| io_error(root, e))?;
            if entry
                .file_type()
                .map_err(|e| io_error(&entry.path(), e))?
                .is_symlink()
            {
                return Err(Error::usage("batch snapshots do not follow symbolic links"));
            }
            tree(&entry.path(), base, files)?;
        }
    }
    Ok(())
}

/// Snapshot declarations and only the existing targets, independent of HEAD.
fn baseline(context: &Context, steps: &[Step]) -> Result<Table> {
    let workspace = &context.workspace;
    let mut config = Table::new();
    tree(
        &workspace.resolve(crate::config::workspace::MARKER),
        workspace.root(),
        &mut config,
    )?;
    tree(&workspace.kinds_dir(), workspace.root(), &mut config)?;
    tree(&workspace.registry_dir(), workspace.root(), &mut config)?;
    // Focus has independent optimistic checks; unrelated membership is not a declaration change.
    config.remove(&workspace.relative(&workspace.registry_dir().join("focus.csv")));
    let mut out = Table::new();
    out.insert("config".into(), Value::String(hash(config.to_string())));
    for (number, step) in steps.iter().enumerate() {
        let mut target = Table::new();
        if step.kind.as_deref() == Some("focus") && step.verb == "set" {
            target.insert(
                "keys".into(),
                Value::Array(
                    crate::collaboration::focus::keys(context)?
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            );
        } else if let Some(name) = step.kind.as_ref().filter(|name| name.as_str() != "focus") {
            let kind = context.catalog.require(name)?;
            if kind.document_dir().is_some() {
                // Append merges under the writer lock; other document writes protect the kind.
                if step.verb != "new"
                    && kind
                        .action(&step.verb)
                        .is_none_or(|action| action.handler != "append")
                {
                    let dir = if step.verb == "restore" {
                        kind.archive_dir()
                    } else {
                        kind.document_dir().unwrap().into()
                    };
                    tree(&workspace.resolve(&dir), workspace.root(), &mut target)?;
                }
            } else if step.verb != "new"
                && !step
                    .keys
                    .iter()
                    .any(|key| key.starts_with('@') || key.contains("${"))
            {
                let records =
                    crate::storage::records::table(context, kind, step.verb == "restore")?;
                let keys: Vec<_> = step.keys.iter().take(kind.key.len()).cloned().collect();
                if let Some(index) = records.find(&kind.key, &keys) {
                    let record = records.record(index);
                    target.insert(
                        "record".into(),
                        Value::Table(
                            record
                                .iter()
                                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                                .collect(),
                        ),
                    );
                    for field in kind
                        .actions
                        .iter()
                        .filter(|action| action.handler == "attach")
                        .filter_map(|action| action.field.as_ref())
                    {
                        if let Some(file) = record.get(field).filter(|file| !file.is_empty()) {
                            tree(&workspace.resolve(file), workspace.root(), &mut target)?;
                        }
                    }
                }
            }
        }
        out.insert(
            format!("step{}", number + 1),
            Value::String(hash(target.to_string())),
        );
    }
    Ok(out)
}

fn owner(context: &mut Context) -> Result<String> {
    let identity = context.identity()?;
    Ok(format!(
        "{}\n{}\n{}",
        identity.operator.as_deref().unwrap_or(""),
        identity.git_name,
        identity.git_email
    ))
}

fn applied(context: &Context, id: &str) -> Result<Option<String>> {
    let dir = crate::git::operation::directory(&context.workspace)?;
    if !dir.exists() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(&dir).map_err(|e| io_error(&dir, e))? {
        let path = entry.map_err(|e| io_error(&dir, e))?.path();
        if path.extension().is_none_or(|ext| ext != "toml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| io_error(&path, e))?;
        let receipt = crate::config::decl::parse(&text, "operation receipt")?;
        if receipt.get("data").and_then(Value::as_str) == Some("saved")
            && receipt
                .get("batch")
                .and_then(Value::as_table)
                .and_then(|batch| batch.get("draft"))
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                == Some(id)
        {
            return Ok(Some(string(&receipt, "id")?.into()));
        }
    }
    Ok(None)
}

/// Reject known replays before a new local operation can hide Git recovery state.
pub(crate) fn preflight(context: &Context, args: &Args) -> Result<()> {
    if args.positional.get(1).is_some_and(|sub| sub == "apply") && args.positional.len() == 3 {
        let _lock = if args.switch("preview") {
            crate::execution::transaction::WriterLock::snapshot(&context.workspace)?
        } else {
            crate::execution::transaction::WriterLock::acquire(&context.workspace)?
        };
        let draft = load(context, &args.positional[2])?;
        if let Some(operation) = applied(context, string(&draft, "id")?)? {
            return Err(Error::conflict(format!(
                "batch was already applied in {operation}; use clerk operation retry {operation} for Git finalization"
            )));
        }
    }
    Ok(())
}

fn validate(context: &mut Context, draft: &Table) -> Result<Vec<Step>> {
    if let Some(operation) = applied(context, string(draft, "id")?)? {
        return Err(Error::conflict(format!(
            "batch was already applied in {operation}; use operation status or operation retry for Git finalization"
        )));
    }
    if string(draft, "owner")? != owner(context)? {
        return Err(Error::conflict(
            "batch belongs to another operator or Git identity",
        ));
    }
    let steps =
        crate::collaboration::workflow::parse_steps(plan(draft)?, context.workspace.root(), true)?;
    crate::collaboration::workflow::validate_plan(context, &steps)?;
    if draft.get("baseline") != Some(&Value::Table(baseline(context, &steps)?)) {
        return Err(Error::conflict(
            "batch target or declarations changed since capture; inspect the draft and collect a fresh batch",
        ));
    }
    Ok(steps)
}

pub(crate) fn validate_origin(context: &mut Context, origin: Option<&Table>) -> Result<()> {
    if let Some(origin) =
        origin.filter(|origin| origin.get("type").and_then(Value::as_str) == Some("draft"))
    {
        let draft = load(context, string(origin, "name")?)?;
        if string(origin, "sha256")? != hash(draft.to_string())
            || string(origin, "id")? != string(&draft, "id")?
        {
            return Err(Error::conflict(
                "batch draft changed while execution was starting; inspect and retry",
            ));
        }
        validate(context, &draft)?;
    }
    Ok(())
}

pub(crate) fn capture(context: &mut Context, args: &Args, mut step: Step) -> Result<i32> {
    let name = args.value("batch").expect("capture name");
    for option in ["request", "from", "sync", "message", "message-file"] {
        if args.value(option).is_some() {
            return Err(Error::usage(format!(
                "--batch cannot be combined with --{option}"
            )));
        }
    }
    if args.switch("dry-run") {
        return Err(Error::usage(
            "--batch captures only; use batch apply <name> --dry-run to rehearse",
        ));
    }
    if args.value("step-name").is_some() && step.verb != "new" {
        return Err(Error::usage("--step-name requires a captured creation"));
    }
    let draft_path = path(context, name)?;
    let _lock = crate::execution::transaction::WriterLock::snapshot(&context.workspace)?;
    let mut draft = if draft_path.exists() {
        load(context, name)?
    } else {
        let mut draft = Table::new();
        draft.insert("version".into(), Value::Integer(1));
        draft.insert("name".into(), Value::String(name.into()));
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| Error::io(e.to_string()))?
            .as_nanos();
        draft.insert(
            "id".into(),
            Value::String(hash(format!("{nonce}-{}-{name}", std::process::id()))),
        );
        draft.insert("owner".into(), Value::String(owner(context)?));
        draft
    };
    let mut steps = if draft.contains_key("plan") {
        validate(context, &draft)?
    } else {
        Vec::new()
    };
    if step.verb == "new" {
        step.alias = Some(
            args.value("step-name")
                .map(str::to_string)
                .unwrap_or_else(|| format!("step{}", steps.len() + 1)),
        );
    }
    steps.push(step);
    let plan = table(&steps);
    // Reparse captured steps through the strict shared plan grammar.
    let steps = crate::collaboration::workflow::parse_steps(&plan, context.workspace.root(), true)?;
    crate::collaboration::workflow::validate_plan(context, &steps)?;
    draft.insert("baseline".into(), Value::Table(baseline(context, &steps)?));
    draft.insert("plan".into(), Value::Table(plan));
    crate::execution::transaction::atomic_write(&draft_path, draft.to_string().as_bytes())?;
    if args.switch("json") {
        println!(
            "{}",
            serde_json::json!({"batch":name,"step":steps.len(),"alias":steps.last().and_then(|step|step.alias.as_deref()),"executed":false})
        );
    } else {
        println!(
            "batch {name}: captured step {}{}; not executed",
            steps.len(),
            steps
                .last()
                .and_then(|step| step.alias.as_ref())
                .map(|alias| format!(" (@{alias})"))
                .unwrap_or_default()
        );
    }
    Ok(0)
}

pub fn command(context: &mut Context, args: &Args) -> Result<i32> {
    let sub = args.positional.get(1).map(String::as_str).unwrap_or("");
    if sub.is_empty() {
        return crate::collaboration::templates::dispatch_batch(context, args);
    }
    if args.value("file").is_some()
        || args.value("template").is_some()
        || args.value("input-format").is_some()
    {
        return Err(Error::usage(
            "batch draft commands do not accept --file, --template or --input-format",
        ));
    }
    if sub == "list" {
        if args.positional.len() != 2 || args.switch("preview") || args.switch("dry-run") {
            return Err(Error::usage("usage: clerk batch list [--json]"));
        }
        let _lock = crate::execution::transaction::WriterLock::snapshot(&context.workspace)?;
        let dir = path(context, "placeholder")?
            .parent()
            .unwrap()
            .to_path_buf();
        let mut rows = Vec::new();
        if dir.exists() {
            for entry in std::fs::read_dir(&dir).map_err(|e| io_error(&dir, e))? {
                let path = entry.map_err(|e| io_error(&dir, e))?.path();
                if path.extension().is_none_or(|ext| ext != "toml") {
                    continue;
                }
                let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                let draft = load(context, &name)?;
                rows.push(vec![
                    name,
                    plan(&draft)?
                        .get("step")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len)
                        .to_string(),
                    if applied(context, string(&draft, "id")?)?.is_some() {
                        "applied"
                    } else {
                        "pending"
                    }
                    .into(),
                ]);
            }
        }
        rows.sort();
        if args.switch("json") {
            println!("{}", serde_json::to_string(&rows.iter().map(|row|serde_json::json!({"name":row[0],"steps":row[1].parse::<usize>().expect("step count"),"state":row[2]})).collect::<Vec<_>>()).expect("draft list JSON"));
        } else {
            crate::terminal::output::print_table(
                args,
                &["name".into(), "steps".into(), "state".into()],
                &rows,
                0,
            )?;
        }
        return Ok(0);
    }
    if args.positional.len() != 3 {
        return Err(Error::usage("usage: clerk batch show|apply|drop <name>"));
    }
    let name = &args.positional[2];
    if sub == "apply" {
        let draft = load(context, name)?;
        let steps = {
            let _lock = crate::execution::transaction::WriterLock::snapshot(&context.workspace)?;
            validate(context, &draft)?
        };
        if args.switch("preview") {
            return preview(context, &steps, args.switch("json"));
        }
        let origin = [
            ("type".into(), Value::String("draft".into())),
            ("name".into(), Value::String(name.clone())),
            ("id".into(), Value::String(string(&draft, "id")?.into())),
            ("sha256".into(), Value::String(hash(draft.to_string()))),
        ]
        .into_iter()
        .collect();
        let result = crate::collaboration::workflow::run_steps(
            context,
            steps,
            args.switch("dry-run"),
            args.switch("json"),
            Some(origin),
        );
        if result.is_ok()
            && !args.switch("dry-run")
            && let Some(report) = crate::git::operation::current()
        {
            cleanup(context, &report.id);
        }
        return result;
    }
    let _lock = crate::execution::transaction::WriterLock::snapshot(&context.workspace)?;
    let draft = load(context, name)?;
    if args.switch("dry-run") || args.switch("preview") {
        return Err(Error::usage("--dry-run and --preview require batch apply"));
    }
    match sub {
        "show" => {
            if args.switch("json") {
                println!(
                    "{}",
                    serde_json::json!({"name":name,"state":if applied(context,string(&draft,"id")?)?.is_some(){"applied"}else{"pending"},"plan":plan(&draft)?})
                );
            } else {
                let state = if applied(context, string(&draft, "id")?)?.is_some() {
                    "applied; use operation retry for Git finalization"
                } else {
                    "pending; not executed"
                };
                println!("Batch: {name}\nState: {state}\n{}", plan(&draft)?);
            }
        }
        "drop" => {
            let path = path(context, name)?;
            std::fs::remove_file(&path).map_err(|e| io_error(&path, e))?;
            if args.switch("json") {
                println!("{}", serde_json::json!({"dropped":name}));
            } else {
                println!("dropped batch {name}");
            }
        }
        _ => {
            return Err(Error::usage(
                "batch expects --file, --template, show, apply, list or drop",
            ));
        }
    }
    Ok(0)
}

/// Cleanup is advisory: the accepted local receipt already prevents duplicate execution.
pub(crate) fn cleanup(context: &Context, operation: &str) {
    let result = (|| -> Result<()> {
        let _lock = crate::execution::transaction::WriterLock::snapshot(&context.workspace)?;
        let report = crate::git::operation::load(&context.workspace, Some(operation))?;
        let Some(origin) = report
            .batch
            .as_ref()
            .and_then(|batch| batch.get("draft"))
            .and_then(Value::as_table)
        else {
            return Ok(());
        };
        let name = string(origin, "name")?;
        let draft_path = path(context, name)?;
        if draft_path.exists() {
            let draft = load(context, name)?;
            if string(&draft, "id")? == string(origin, "id")? {
                std::fs::remove_file(&draft_path).map_err(|e| io_error(&draft_path, e))?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!(
            "batch applied; local draft cleanup failed: {error}; use batch drop after inspecting the receipt"
        );
    }
}

pub(crate) fn capture_kind(
    context: &mut Context,
    args: &Args,
    kind: &crate::config::kind::Kind,
    verb: &str,
    keys: Vec<String>,
    values: Values,
    options: crate::execution::engine::Options,
) -> Result<i32> {
    if !crate::interface::command::writes(kind, verb) {
        return Err(Error::usage("--batch requires a write verb"));
    }
    capture(
        context,
        args,
        Step {
            kind: Some(kind.name.clone()),
            verb: verb.into(),
            keys,
            values,
            tokens: options.tokens,
            body: options.body,
            force: options.force,
            ..Default::default()
        },
    )
}
