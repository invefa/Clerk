//! Shared application dispatch for CLI, requests and workflow steps.

use crate::config::kind::Kind;
use crate::execution::context::Context;
use crate::execution::engine::{self, Options, Values};
use crate::foundation::error::{Error, Result};
use crate::storage::document;

/// Typed operation independent of command-line parsing and output.
#[derive(Clone, Debug)]
pub struct Command {
    pub kind: String,
    pub verb: String,
    pub keys: Vec<String>,
    pub values: Values,
    pub options: Options,
}
impl Command {
    pub fn execute(&self, context: &mut Context) -> Result<engine::Outcome> {
        let kind = context.catalog.require(&self.kind)?.clone();
        dispatch(
            context,
            &kind,
            &self.verb,
            &self.keys,
            &self.values,
            &self.options,
        )
    }
}

/// Borrowed adapters construct the same application operation.
pub fn execute(
    context: &mut Context,
    kind: &Kind,
    verb: &str,
    keys: &[String],
    values: &Values,
    options: &Options,
) -> Result<engine::Outcome> {
    Command {
        kind: kind.name.clone(),
        verb: verb.into(),
        keys: keys.to_vec(),
        values: values.clone(),
        options: options.clone(),
    }
    .execute(context)
}

/// Static command shape shared by direct commands and batch plans.
pub(crate) fn validate_shape(
    kind: &Kind,
    verb: &str,
    key_count: usize,
    values: &Values,
) -> Result<()> {
    if crate::interface::command::writes(kind, verb) {
        let expected = if verb == "new"
            || kind
                .action(verb)
                .is_some_and(|action| matches!(action.handler.as_str(), "append" | "prune"))
        {
            0
        } else if matches!(verb, "link" | "unlink") {
            4
        } else {
            kind.key.len()
        };
        if key_count != expected {
            return Err(Error::usage(format!(
                "`{} {verb}` expects {expected} key argument(s), found {}",
                kind.name, key_count
            )));
        }
    }
    if crate::interface::command::writes(kind, verb)
        && !matches!(verb, "new" | "set")
        && kind.transition(verb).is_none()
        && !values.is_empty()
    {
        return Err(Error::usage(
            "this action does not accept field values; use declared action inputs",
        ));
    }
    if crate::interface::command::writes(kind, verb) {
        if verb == "set"
            && kind
                .state
                .as_ref()
                .is_some_and(|state| values.contains_key(&state.field))
        {
            return Err(Error::usage(
                "state fields require a declared transition verb",
            ));
        }
        engine::validate_fields(kind, values, verb == "new")?;
    }
    Ok(())
}

/// Run one verb against a kind.
fn dispatch(
    context: &mut Context,
    kind: &Kind,
    verb: &str,
    rest: &[String],
    values: &Values,
    options: &Options,
) -> Result<engine::Outcome> {
    validate_shape(kind, verb, rest.len(), values)?;
    if matches!(verb, "list" | "show") {
        return crate::query::scope::read(context, kind, verb, rest, options);
    }
    if !options.scope.is_empty() && options.scope != "current" {
        return Err(Error::usage(
            "--data-scope is only available for list and show",
        ));
    }
    let scheme = kind
        .numbering
        .as_ref()
        .map(|name| context.numbering(name))
        .transpose()?;
    crate::interface::command::validate_tokens(kind, scheme, &options.tokens)?;
    let reserved = if !options.dry_run && crate::interface::command::writes(kind, verb) {
        crate::storage::issued::remember(context, kind, &[])?
    } else {
        None
    };
    let mut outcome = if kind.document_dir().is_some() {
        run_document_verb(context, kind, verb, rest, values, options)
    } else {
        run_table_verb(context, kind, verb, rest, values, options)
    }?;
    if let Some(path) = reserved {
        outcome.affected.push(path);
    }
    if crate::interface::command::writes(kind, verb) {
        let keys = outcome
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
            .filter(|key| !key.is_empty())
            .collect::<Vec<_>>();
        let keys = if keys.is_empty() { rest.to_vec() } else { keys };
        crate::git::commit_message::record(&kind.name, verb, &keys);
    }
    Ok(outcome)
}

fn run_table_verb(
    context: &mut Context,
    kind: &Kind,
    verb: &str,
    rest: &[String],
    values: &Values,
    options: &Options,
) -> Result<engine::Outcome> {
    match verb {
        "list" => engine::list(context, &kind.name, options),
        "show" => engine::show(context, &kind.name, rest),
        "new" => engine::create(context, &kind.name, values, options),
        "set" => engine::set(context, &kind.name, rest, values, options),
        "delete" => engine::delete(context, &kind.name, rest, options),
        "archive" => engine::archive(context, &kind.name, rest, options),
        "restore" => engine::restore(context, &kind.name, rest, options),
        "link" | "unlink" => run_link(context, kind, verb, rest),
        other => {
            if kind.action(other).is_some() {
                return engine::action(context, &kind.name, other, rest, options);
            }
            if kind.transition(other).is_none() {
                return Err(Error::usage(format!(
                    "kind `{}` has no verb `{other}`; verbs: {}",
                    kind.name,
                    crate::interface::command::verb_names(kind).join(", ")
                )));
            }
            engine::transition(context, &kind.name, other, rest, values, options)
        }
    }
}

fn run_link(
    context: &Context,
    kind: &Kind,
    verb: &str,
    rest: &[String],
) -> Result<engine::Outcome> {
    let usage = || Error::usage("usage: <kind> link <key> <relation> <kind> <key>");
    let from = rest.first().cloned().ok_or_else(usage)?;
    let relation = rest.get(1).cloned().ok_or_else(usage)?;
    let target_kind = rest.get(2).cloned().ok_or_else(usage)?;
    let target_key = rest.get(3).cloned().ok_or_else(usage)?;
    let from_keys = vec![from];
    let to_keys = vec![target_key];
    if verb == "link" {
        engine::link(
            context,
            &kind.name,
            &from_keys,
            &relation,
            &target_kind,
            &to_keys,
        )
    } else {
        engine::unlink(
            context,
            &kind.name,
            &from_keys,
            &relation,
            &target_kind,
            &to_keys,
        )
    }
}

fn run_document_verb(
    context: &mut Context,
    kind: &Kind,
    verb: &str,
    rest: &[String],
    values: &Values,
    options: &Options,
) -> Result<engine::Outcome> {
    match verb {
        "list" => document::list(context, kind, options),
        "show" => document::show(context, kind, rest),
        "new" => document::create(context, kind, values, options),
        "set" => document::set(context, kind, rest, values, options),
        "delete" => document::delete(context, kind, rest, options),
        "archive" => document::archive(context, kind, rest, options),
        "restore" => document::restore(context, kind, rest, options),
        "link" | "unlink" => Err(Error::usage(format!(
            "kind `{}` stores documents, so it has no relation table",
            kind.name
        ))),
        other => {
            if let Some(action) = kind.action(other) {
                if action.handler == "append" || action.handler == "prune" {
                    return engine::action(context, &kind.name, other, rest, options);
                }
                return Err(Error::usage(format!(
                    "kind `{}` is document backed; action `{other}` is not available there yet",
                    kind.name
                )));
            }
            if kind.transition(other).is_none() {
                return Err(Error::usage(format!(
                    "kind `{}` has no verb `{other}`; verbs: {}",
                    kind.name,
                    crate::interface::command::verb_names(kind).join(", ")
                )));
            }
            document::transition(context, kind, other, rest, values, options)
        }
    }
}
