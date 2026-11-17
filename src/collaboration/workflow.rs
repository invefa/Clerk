//! Ordered application operations under one rollback and Git boundary.

use crate::config::decl::{self, Table};
use crate::execution::context::Context;
use crate::execution::engine::{Options, Values};
use crate::foundation::error::{Error, Result};
use crate::storage::store::Record;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

pub use crate::collaboration::templates::{
    Template, accepts_option, command as templates_command, dispatch_batch, load_templates,
    validate_templates,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct Step {
    pub kind: Option<String>,
    pub verb: String,
    pub alias: Option<String>,
    pub keys: Vec<String>,
    pub values: Values,
    pub tokens: Values,
    pub body: Option<String>,
    pub force: bool,
}

fn strings(table: &Table, key: &str) -> Result<Values> {
    match table.get(key) {
        None => Ok(Values::new()),
        Some(toml::Value::Table(values)) => values
            .iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|v| (k.clone(), v.into()))
                    .ok_or_else(|| Error::usage(format!("step.{key}.{k} must be a string")))
            })
            .collect(),
        Some(_) => Err(Error::usage(format!("step.{key} must be a table"))),
    }
}

pub(crate) fn load(source: &str, format: &str) -> Result<Vec<Step>> {
    if !matches!(format, "toml" | "json") {
        return Err(Error::usage("--input-format needs toml or json"));
    }
    let (text, base) = if source == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| Error::io(e.to_string()))?;
        (
            text,
            std::env::current_dir().map_err(|e| Error::io(e.to_string()))?,
        )
    } else {
        let path = Path::new(source)
            .canonicalize()
            .map_err(|e| crate::foundation::error::io_error(Path::new(source), e))?;
        (
            std::fs::read_to_string(&path)
                .map_err(|e| crate::foundation::error::io_error(&path, e))?,
            path.parent().expect("file parent").to_path_buf(),
        )
    };
    let table = if format == "json" {
        serde_json::from_str::<Table>(&text)
            .map_err(|error| Error::usage(format!("invalid batch JSON: {error}")))?
    } else {
        decl::parse(&text, "batch")?
    };
    parse_steps(&table, &base, true)
}

pub(crate) fn parse_steps(table: &Table, base: &Path, read_files: bool) -> Result<Vec<Step>> {
    decl::only_keys(table, &["version", "step"], "workflow")?;
    if table.get("version").and_then(toml::Value::as_integer) != Some(1) {
        return Err(Error::usage("workflow.version must be 1"));
    }
    let items = table
        .get("step")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| Error::usage("workflow needs [[step]] entries"))?;
    if items.is_empty() {
        return Err(Error::usage("workflow has no steps"));
    }
    let mut aliases = BTreeSet::new();
    let mut steps = Vec::new();
    for item in items {
        let table = item
            .as_table()
            .ok_or_else(|| Error::usage("each workflow step must be a table"))?;
        decl::only_keys(
            table,
            &[
                "kind", "verb", "as", "keys", "values", "tokens", "body", "file", "force",
            ],
            "step",
        )?;
        for key in ["kind", "verb", "as", "body", "file"] {
            if table.get(key).is_some_and(|v| !v.is_str()) {
                return Err(Error::usage(format!("step.{key} must be a string")));
            }
        }
        if table.get("force").is_some_and(|v| !v.is_bool()) {
            return Err(Error::usage("step.force must be a boolean"));
        }
        if table.get("keys").is_some_and(|v| {
            !v.as_array()
                .is_some_and(|a| a.iter().all(toml::Value::is_str))
        }) {
            return Err(Error::usage("step.keys must be an array of strings"));
        }
        let verb = decl::required_text(table, "verb", "step")?;
        let kind = decl::text(table, "kind");
        if kind.as_deref() == Some("focus") {
            decl::only_keys(table, &["kind", "verb", "keys"], "batch step")?;
            crate::collaboration::focus::Change::parse(&verb)?;
        }
        if kind.is_none() && (verb != "render" || table.len() != 1) {
            return Err(Error::usage(
                "a step without kind must contain only verb = render",
            ));
        }
        let alias = decl::text(table, "as");
        if let Some(alias) = &alias
            && (verb != "new"
                || alias == "params"
                || alias.is_empty()
                || !alias
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_')
                || !aliases.insert(alias.clone()))
        {
            return Err(Error::usage(
                "step.as must be a unique identifier on a new step",
            ));
        }
        let body = decl::text(table, "body");
        let file = decl::text(table, "file");
        if body.is_some() && file.is_some() {
            return Err(Error::usage(
                "step.body and step.file are mutually exclusive",
            ));
        }
        let body = if let Some(file) = file {
            if file == "-" {
                return Err(Error::usage("step.file cannot read stdin; use step.body"));
            }
            let path = base.join(file);
            if read_files {
                Some(
                    std::fs::read_to_string(&path)
                        .map_err(|e| crate::foundation::error::io_error(&path, e))?,
                )
            } else {
                None
            }
        } else {
            body
        };
        steps.push(Step {
            kind,
            verb,
            alias,
            keys: decl::strings(table, "keys"),
            values: strings(table, "values")?,
            tokens: strings(table, "tokens")?,
            body,
            force: decl::flag(table, "force").unwrap_or(false),
        });
    }
    Ok(steps)
}

fn expand(text: &str, aliases: &BTreeMap<String, Record>) -> Result<String> {
    let mut rest = text;
    let mut out = String::new();
    while let Some(at) = rest.find("${") {
        if rest[..at].ends_with('$') {
            out.push_str(&rest[..at - 1]);
            out.push_str("${");
            rest = &rest[at + 2..];
            continue;
        }
        out.push_str(&rest[..at]);
        rest = &rest[at + 2..];
        let end = rest
            .find('}')
            .ok_or_else(|| Error::usage("unclosed workflow substitution"))?;
        let (alias, field) = rest[..end]
            .split_once('.')
            .ok_or_else(|| Error::usage("workflow substitution needs ${alias.field}"))?;
        let value = aliases
            .get(alias)
            .and_then(|record| record.get(field))
            .ok_or_else(|| Error::usage(format!("unknown earlier step field `{alias}.{field}`")))?;
        out.push_str(value);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn expand_keys(
    keys: &[String],
    aliases: &BTreeMap<String, Record>,
    alias_keys: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for key in keys {
        if let Some(literal) = key.strip_prefix("@@") {
            out.push(format!("@{}", expand(literal, aliases)?));
        } else if let Some(alias) = key.strip_prefix('@') {
            out.extend(
                alias_keys
                    .get(alias)
                    .ok_or_else(|| Error::usage(format!("unknown earlier step `{alias}`")))?
                    .clone(),
            );
        } else {
            out.push(expand(key, aliases)?);
        }
    }
    Ok(out)
}

/// Check declaration symbols without allocating records or reading body files.
pub(crate) fn validate_plan(context: &Context, steps: &[Step]) -> Result<()> {
    let mut aliases = BTreeMap::<String, usize>::new();
    for step in steps {
        let key_count = step
            .keys
            .iter()
            .map(|key| {
                if key.starts_with("@@") {
                    Ok(1)
                } else if let Some(alias) = key.strip_prefix('@') {
                    aliases
                        .get(alias)
                        .copied()
                        .ok_or_else(|| Error::usage(format!("unknown earlier step `{alias}`")))
                } else {
                    Ok(1)
                }
            })
            .collect::<Result<Vec<_>>>()?
            .iter()
            .sum();
        if let Some(name) = step.kind.as_deref().filter(|name| *name != "focus") {
            let kind = context.catalog.require(name)?;
            if !crate::interface::command::writes(kind, &step.verb) {
                return Err(Error::usage(format!(
                    "workflow requires a declared write verb, found `{name} {}`",
                    step.verb
                )));
            }
            crate::execution::application::validate_shape(
                kind,
                &step.verb,
                key_count,
                &step.values,
            )?;
            let scheme = kind
                .numbering
                .as_deref()
                .map(|name| context.numbering(name))
                .transpose()?;
            crate::interface::command::validate_tokens(kind, scheme, &step.tokens)?;
            if let Some(alias) = &step.alias {
                aliases.insert(alias.clone(), kind.key.len());
            }
        }
    }
    Ok(())
}

pub(crate) fn run_steps(
    context: &mut Context,
    steps: Vec<Step>,
    dry_run: bool,
    json: bool,
    origin: Option<Table>,
) -> Result<i32> {
    validate_plan(context, &steps)?;
    crate::execution::batch::validate_origin(context, origin.as_ref())?;
    // First version deliberately refuses all preexisting changes, including staging.
    crate::git::commit::require_clean(&context.workspace)?;
    let mut write = crate::execution::pipeline::begin(context)?;
    crate::execution::batch::validate_origin(context, origin.as_ref())?;
    crate::git::commit::require_clean(&context.workspace)?;
    write.require_check = true;
    let mut aliases = BTreeMap::<String, Record>::new();
    let mut alias_keys = BTreeMap::<String, Vec<String>>::new();
    let mut affected = Vec::new();
    let mut removed = Vec::new();
    let mut receipt_steps = Vec::new();
    for (number, step) in steps.iter().enumerate() {
        let result: Result<()> = (|| {
            let mut receipt = toml::Table::new();
            receipt.insert("number".into(), toml::Value::Integer((number + 1) as i64));
            receipt.insert(
                "at".into(),
                toml::Value::String(crate::foundation::time::stamp(
                    crate::foundation::time::now_epoch(),
                    context.workspace.policy.offset_hours,
                )),
            );
            receipt.insert("verb".into(), toml::Value::String(step.verb.clone()));
            if step.kind.as_deref() == Some("focus") {
                if !step.values.is_empty()
                    || !step.tokens.is_empty()
                    || step.body.is_some()
                    || step.force
                    || step.alias.is_some()
                {
                    return Err(Error::usage("batch steps accept only kind, verb and keys"));
                }
                let keys = expand_keys(&step.keys, &aliases, &alias_keys)?;
                let outcome = crate::collaboration::focus::apply(
                    context,
                    crate::collaboration::focus::Change::parse(&step.verb)?,
                    &keys,
                )?;
                receipt.insert("kind".into(), toml::Value::String("focus".into()));
                for (name, keys) in [
                    ("added", outcome.added),
                    ("removed", outcome.removed),
                    ("unchanged", outcome.unchanged),
                ] {
                    receipt.insert(
                        name.into(),
                        toml::Value::Array(keys.into_iter().map(toml::Value::String).collect()),
                    );
                }
                receipt.insert(
                    "keys".into(),
                    toml::Value::Array(keys.into_iter().map(toml::Value::String).collect()),
                );
                affected.extend(outcome.affected);
            } else if let Some(name) = &step.kind {
                let kind = context.catalog.require(name)?.clone();
                if !crate::interface::command::writes(&kind, &step.verb) {
                    return Err(Error::usage(format!(
                        "workflow requires a declared write verb, found `{name} {}`",
                        step.verb
                    )));
                }
                let keys = expand_keys(&step.keys, &aliases, &alias_keys)?;
                let values = step
                    .values
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), expand(v, &aliases)?)))
                    .collect::<Result<Values>>()?;
                let tokens = step
                    .tokens
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), expand(v, &aliases)?)))
                    .collect::<Result<Values>>()?;
                let options = Options {
                    tokens,
                    body: step
                        .body
                        .as_deref()
                        .map(|body| expand(body, &aliases))
                        .transpose()?,
                    force: step.force,
                    ..Default::default()
                };
                let outcome = crate::execution::application::execute(
                    context, &kind, &step.verb, &keys, &values, &options,
                )?;
                if let Some(alias) = &step.alias {
                    let record = outcome
                        .records
                        .first()
                        .ok_or_else(|| Error::usage("aliased creation returned no record"))?;
                    alias_keys.insert(
                        alias.clone(),
                        kind.key
                            .iter()
                            .map(|key| record.get(key).cloned().unwrap_or_default())
                            .collect(),
                    );
                    aliases.insert(alias.clone(), record.clone());
                    receipt.insert("as".into(), toml::Value::String(alias.clone()));
                }
                let keys = if step.verb == "new" {
                    outcome
                        .records
                        .iter()
                        .map(|record| {
                            kind.key
                                .iter()
                                .filter_map(|key| record.get(key))
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("/")
                        })
                        .collect()
                } else {
                    keys
                };
                receipt.insert("kind".into(), toml::Value::String(kind.name));
                receipt.insert(
                    "keys".into(),
                    toml::Value::Array(keys.into_iter().map(toml::Value::String).collect()),
                );
                receipt.insert(
                    "records".into(),
                    toml::Value::Array(
                        outcome
                            .records
                            .iter()
                            .map(|record| {
                                toml::Value::Table(
                                    record
                                        .iter()
                                        .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
                                        .collect(),
                                )
                            })
                            .collect(),
                    ),
                );
                affected.extend(outcome.affected);
                removed.extend(outcome.removed);
            } else {
                affected.extend(crate::execution::render::render(context)?.changed);
            }
            receipt_steps.push(toml::Value::Table(receipt));
            Ok(())
        })();
        result.map_err(|error| {
            Error::new(
                error.failure(),
                format!(
                    "batch step {} ({} {} {}): {error}",
                    number + 1,
                    step.kind.as_deref().unwrap_or("page"),
                    step.verb,
                    step.keys.join(" ")
                ),
            )
        })?;
    }
    let operation = crate::git::operation::current().expect("write operation");
    let mut receipt = toml::Table::new();
    receipt.insert("version".into(), toml::Value::Integer(1));
    if let Some(origin) = origin {
        let key = if origin.get("type").and_then(toml::Value::as_str) == Some("draft") {
            "draft"
        } else {
            "template"
        };
        receipt.insert(key.into(), toml::Value::Table(origin));
    }
    let identity = context.identity()?.clone();
    for (key, value) in [
        ("id", operation.id.clone()),
        ("operator", identity.operator.unwrap_or_default()),
        ("author", identity.git_name),
        ("email", identity.git_email),
        ("baseline", operation.before.clone()),
    ] {
        receipt.insert(key.into(), toml::Value::String(value));
    }
    receipt.insert("step".into(), toml::Value::Array(receipt_steps.clone()));
    crate::git::operation::change(|report| report.batch = Some(receipt));
    if dry_run {
        // Check after computing temporary seals; rollback includes them too.
        crate::execution::pipeline::check_paths(context, &affected, &removed)?;
        drop(write);
        if json {
            println!(
                "{{\"dry_run\":true,\"steps\":{},\"results\":{},\"execution\":{}}}",
                steps.len(),
                serde_json::to_string(&receipt_steps).expect("receipt JSON"),
                crate::git::operation::current().expect("operation").json()
            );
        } else {
            crate::execution::batch::print_results(&receipt_steps);
            println!(
                "dry run: {} step(s) validated; nothing retained",
                steps.len()
            );
        }
    } else {
        let notes = crate::execution::pipeline::finish_paths(
            context,
            "workflow",
            &mut affected,
            &removed,
            write,
        )?;
        if json {
            println!(
                "{{\"dry_run\":false,\"steps\":{},\"results\":{},\"execution\":{}}}",
                steps.len(),
                serde_json::to_string(&receipt_steps).expect("receipt JSON"),
                crate::git::operation::current().expect("operation").json()
            );
        } else {
            crate::execution::batch::print_results(&receipt_steps);
            for note in notes {
                println!("{note}");
            }
            println!("{} step(s) applied", steps.len());
        }
    }
    Ok(0)
}
