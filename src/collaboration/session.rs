//! Local advisory leases, shared by worktrees through Git's common directory.

use crate::config::schema::{json, object, quote};
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result, io_error};
use crate::interface::cli::Args;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| Error::io(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::usage(format!(
            "session requires a Git checkout: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

fn directory(context: &Context) -> Result<PathBuf> {
    let common = PathBuf::from(git(
        context.workspace.root(),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    Ok(crate::execution::transaction::metadata_dir(&common)?.join("sessions"))
}

fn read(path: &Path) -> Result<Vec<toml::Table>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let body = std::fs::read_to_string(path).map_err(|e| io_error(path, e))?;
    let mut table: toml::Table =
        toml::from_str(&body).map_err(|e| Error::validation(format!("invalid sessions: {e}")))?;
    crate::config::decl::only_keys(&table, &["version", "session"], "sessions")?;
    if table.remove("version").and_then(|v| v.as_integer()) != Some(1) {
        return Err(Error::validation("session format version must be 1"));
    }
    let rows = table
        .remove("session")
        .and_then(|v| v.as_array().cloned())
        .ok_or_else(|| Error::validation("session data needs an array"))?;
    rows.into_iter()
        .map(|row| {
            let table = row
                .as_table()
                .cloned()
                .ok_or_else(|| Error::validation("invalid session row"))?;
            crate::config::decl::only_keys(
                &table,
                &[
                    "id",
                    "chat",
                    "owner",
                    "email",
                    "checkout",
                    "branch",
                    "tasks",
                    "paths",
                    "started",
                    "last_activity",
                    "expires",
                    "closed",
                ],
                "session",
            )?;
            for field in ["id", "chat", "owner", "email", "checkout", "branch"] {
                if table.get(field).and_then(toml::Value::as_str).is_none() {
                    return Err(Error::validation(format!("session needs string `{field}`")));
                }
            }
            for field in ["started", "last_activity", "expires"] {
                if table
                    .get(field)
                    .and_then(toml::Value::as_integer)
                    .is_none_or(|n| n < 0)
                {
                    return Err(Error::validation(format!("session needs epoch `{field}`")));
                }
            }
            for field in ["tasks", "paths"] {
                if table
                    .get(field)
                    .and_then(toml::Value::as_array)
                    .is_none_or(|v| v.iter().any(|s| s.as_str().is_none()))
                {
                    return Err(Error::validation(format!(
                        "session needs string array `{field}`"
                    )));
                }
            }
            if table.get("closed").and_then(toml::Value::as_bool).is_none() {
                return Err(Error::validation("session needs boolean closed"));
            }
            Ok(table)
        })
        .collect()
}

fn text<'a>(row: &'a toml::Table, key: &str) -> &'a str {
    row.get(key).and_then(toml::Value::as_str).unwrap_or("")
}
fn array(row: &toml::Table, key: &str) -> Vec<String> {
    row.get(key)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_string)
        .collect()
}
fn strings(values: Vec<String>) -> toml::Value {
    toml::Value::Array(values.into_iter().map(toml::Value::String).collect())
}
fn active(row: &toml::Table, now: u64) -> bool {
    row.get("closed").and_then(toml::Value::as_bool) != Some(true)
        && row
            .get("expires")
            .and_then(toml::Value::as_integer)
            .unwrap_or(0)
            > now as i64
}

fn paths(value: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for item in value.split(',') {
        let item = item.trim().replace('\\', "/");
        let path = Path::new(&item);
        if item.is_empty()
            || item.contains(':')
            || path.is_absolute()
            || path.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(Error::usage(
                "session paths must be non-empty repository-relative paths without ..",
            ));
        }
        let value = path
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        let value = if value.is_empty() { ".".into() } else { value };
        if !out.contains(&value) {
            out.push(value);
        }
    }
    Ok(out)
}

fn intersects(left: &str, right: &str) -> bool {
    let normalize = |s: &str| {
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let (left, right) = (normalize(left), normalize(right));
    left == "."
        || right == "."
        || Path::new(&left).starts_with(&right)
        || Path::new(&right).starts_with(&left)
}

fn overlaps(rows: &[toml::Table], row: &toml::Table, now: u64) -> Vec<String> {
    let paths = array(row, "paths");
    rows.iter()
        .filter(|other| text(other, "id") != text(row, "id") && active(other, now))
        .filter(|other| {
            array(other, "paths")
                .iter()
                .any(|p| paths.iter().any(|mine| intersects(mine, p)))
        })
        .map(|other| text(other, "id").to_string())
        .collect()
}

pub fn snapshot(context: &Context, task: &str) -> Result<Vec<toml::Table>> {
    Ok(read(&directory(context)?.join("sessions.toml"))?
        .into_iter()
        .filter(|row| array(row, "tasks").iter().any(|key| key == task))
        .map(|mut row| {
            let live = active(&row, crate::foundation::time::now_epoch());
            row.insert("active".into(), toml::Value::Boolean(live));
            row
        })
        .collect())
}

pub fn command(context: &Context, args: &Args) -> Result<i32> {
    let verb = args.positional.get(1).map(String::as_str).unwrap_or("list");
    let writes = matches!(verb, "open" | "touch" | "scope" | "close");
    let specific = match verb {
        "open" => &["chat", "tasks", "paths", "ttl", "strict"][..],
        "touch" => &["ttl", "strict"][..],
        "scope" => &["add", "remove", "ttl", "strict"][..],
        "close" | "list" | "show" => &[][..],
        _ => return Err(Error::usage("unknown session verb")),
    };
    for key in args
        .values
        .keys()
        .chain(args.switches.iter())
        .chain(args.off.iter())
    {
        if ["chat", "tasks", "paths", "ttl", "strict", "add", "remove"].contains(&key.as_str())
            && !specific.contains(&key.as_str())
        {
            return Err(Error::usage(format!(
                "session {verb} does not accept --{key}"
            )));
        }
    }

    if args.value("sync").is_some() || args.switch("dry-run") || args.value("request").is_some() {
        return Err(Error::usage(
            "local sessions do not support sync, requests or dry-run",
        ));
    }
    let dir = directory(context)?;
    let path = dir.join("sessions.toml");
    let _lock = if writes || path.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| io_error(&dir, e))?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("lock"))
            .map_err(|e| io_error(&dir, e))?;
        file.try_lock()
            .map_err(|_| Error::conflict("another session command is active; retry"))?;
        Some(file)
    } else {
        None
    };
    let mut rows = read(&path)?;
    let now = crate::foundation::time::now_epoch();
    let mut warnings = Vec::new();
    let selected;
    if writes {
        let identity =
            crate::config::identity::Identity::resolve(&context.workspace, args.value("as"))?;
        let ttl = args
            .value("ttl")
            .unwrap_or("1800")
            .parse::<u64>()
            .ok()
            .filter(|n| (1..=86400).contains(n))
            .ok_or_else(|| Error::usage("--ttl needs seconds between 1 and 86400"))?;
        let mut row = if verb == "open" {
            if args.positional.len() != 2 {
                return Err(Error::usage(
                    "session open takes options, not a positional id",
                ));
            }
            let chat = args
                .value("chat")
                .filter(|v| !v.trim().is_empty() && !v.contains(['\n', '\r']))
                .ok_or_else(|| Error::usage("session open needs --chat <id>"))?;
            let tasks = args
                .value("tasks")
                .map(crate::collaboration::log_entry::keys)
                .transpose()?
                .unwrap_or_default();
            if !tasks.is_empty() {
                crate::collaboration::log_entry::validate_refs(context, "tasks", &tasks)?;
            }
            let scope = paths(
                args.value("paths")
                    .ok_or_else(|| Error::usage("session open needs --paths <paths>"))?,
            )?;
            let mut row = toml::Table::new();
            for (key, value) in [
                ("id", crate::collaboration::log_entry::unique_id()),
                ("chat", chat.into()),
                ("owner", identity.display()),
                ("email", identity.git_email.clone()),
                (
                    "checkout",
                    context.workspace.root().to_string_lossy().into_owned(),
                ),
                (
                    "branch",
                    git(
                        context.workspace.root(),
                        &["rev-parse", "--abbrev-ref", "HEAD"],
                    )?,
                ),
            ] {
                row.insert(key.into(), toml::Value::String(value));
            }
            row.insert("tasks".into(), strings(tasks));
            row.insert("paths".into(), strings(scope));
            row.insert("started".into(), toml::Value::Integer(now as i64));
            row.insert("closed".into(), toml::Value::Boolean(false));
            row
        } else {
            if args.positional.len() != 3 {
                return Err(Error::usage(
                    "session mutation needs exactly one session id",
                ));
            }
            let id = &args.positional[2];
            let row = rows
                .iter()
                .find(|row| text(row, "id") == id)
                .cloned()
                .ok_or_else(|| Error::usage(format!("unknown session `{id}`")))?;
            if text(&row, "email") != identity.git_email {
                return Err(Error::conflict("session belongs to another Git identity"));
            }
            if text(&row, "checkout") != context.workspace.root().to_string_lossy() {
                return Err(Error::conflict(
                    "mutate this session from its registered checkout",
                ));
            }
            if row.get("closed").and_then(toml::Value::as_bool) == Some(true) {
                return Err(Error::conflict("session is closed; open a new session"));
            }
            row
        };
        if verb == "scope" {
            if args.value("add").is_none() && args.value("remove").is_none() {
                return Err(Error::usage("session scope needs --add or --remove"));
            }
            let mut scope = array(&row, "paths");
            if let Some(value) = args.value("remove") {
                let remove = paths(value)?;
                scope.retain(|p| !remove.contains(p));
            }
            if let Some(value) = args.value("add") {
                for p in paths(value)? {
                    if !scope.contains(&p) {
                        scope.push(p);
                    }
                }
            }
            row.insert("paths".into(), strings(scope));
        }
        row.insert("last_activity".into(), toml::Value::Integer(now as i64));
        row.insert("expires".into(), toml::Value::Integer((now + ttl) as i64));
        row.insert(
            "branch".into(),
            toml::Value::String(git(
                context.workspace.root(),
                &["rev-parse", "--abbrev-ref", "HEAD"],
            )?),
        );
        if verb == "close" {
            row.insert("closed".into(), toml::Value::Boolean(true));
        } else {
            warnings = overlaps(&rows, &row, now);
        }
        if args.switch("strict") && !warnings.is_empty() {
            return Err(Error::conflict(format!(
                "session scope overlaps {}; no session data was changed",
                warnings.join(", ")
            )));
        }
        rows.retain(|other| text(other, "id") != text(&row, "id"));
        rows.push(row.clone());
        let mut table = toml::Table::new();
        table.insert("version".into(), toml::Value::Integer(1));
        table.insert(
            "session".into(),
            toml::Value::Array(rows.iter().cloned().map(toml::Value::Table).collect()),
        );
        crate::execution::transaction::write(
            &path,
            toml::to_string(&table).map_err(|e| Error::io(e.to_string()))?,
        )?;
        selected = vec![row];
    } else if verb == "show" {
        if args.positional.len() != 3 {
            return Err(Error::usage("session show needs one id"));
        }
        selected = rows
            .into_iter()
            .filter(|row| text(row, "id") == args.positional[2])
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Err(Error::usage("unknown session id"));
        }
    } else {
        if args.positional.len() > 2 {
            return Err(Error::usage("session list takes no id"));
        }
        selected = rows;
    }
    if args.switch("json") {
        let items = selected
            .iter()
            .map(|row| {
                let mut row = row.clone();
                row.insert("active".into(), toml::Value::Boolean(active(&row, now)));
                json(&toml::Value::Table(row))
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{}",
            object([
                ("version", "1".into()),
                ("sessions", format!("[{items}]")),
                ("overlaps", crate::config::schema::strings_json(&warnings)),
                ("scope", quote("local clone; shared worktrees"))
            ])
        );
    } else {
        for row in selected {
            let details: Vec<(String, String)> = [
                ("owner", text(&row, "owner").into()),
                ("chat", text(&row, "chat").into()),
                ("checkout", text(&row, "checkout").into()),
                ("branch", text(&row, "branch").into()),
                ("tasks", array(&row, "tasks").join(", ")),
                ("paths", array(&row, "paths").join(", ")),
                ("active", active(&row, now).to_string()),
                ("expires", row["expires"].to_string()),
            ]
            .into_iter()
            .map(|(key, value)| (format!("{key}:"), value))
            .collect();
            print!(
                "{}",
                crate::terminal::text::group(
                    "Session:",
                    text(&row, "id"),
                    &crate::terminal::text::pairs_with(&details, 0, args.switch("compact"))
                )
            );
        }
        for warning in warnings {
            println!("warning: scope overlaps session {warning}");
        }
    }
    Ok(0)
}
