//! Normalize explicit CLI enum values before defaults, capture and dispatch.

use crate::config::kind::{Field, FieldKind};
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::interface::{cli::Args, command};

/// Negation targets include declared fields and numbering tokens with configurable defaults.
pub fn negatable(
    context: Option<&Context>,
    owner: &str,
    verb: &str,
) -> Vec<(String, command::Negation)> {
    let kind = context.and_then(|context| context.catalog.find(owner));
    let mut out = command::negatable(owner, verb, kind)
        .into_iter()
        .map(|spec| {
            (
                spec.name.to_string(),
                spec.negation.expect("negatable option"),
            )
        })
        .collect::<Vec<_>>();
    if let Some(kind) = kind {
        out.extend(
            kind.fields
                .iter()
                .filter(|field| {
                    !command::reserved(&field.name)
                        && command::kind_applies(kind, verb, &field.name)
                        && !out.iter().any(|(name, _)| name == &field.name)
                })
                .map(|field| (field.name.clone(), command::Negation::Reset))
                .collect::<Vec<_>>(),
        );
        if matches!(verb, "" | "new")
            && let Some(scheme) = context.and_then(|context| {
                kind.numbering
                    .as_deref()
                    .and_then(|name| context.numbering(name).ok())
            })
        {
            for name in scheme.tokens.keys() {
                if !out.iter().any(|(existing, _)| existing == name) {
                    out.push((name.clone(), command::Negation::Reset));
                }
            }
        }
    }
    out
}

/// Only finite field values use prefixes. References and free text keep exact semantics.
pub fn field_values(context: &Context, field: &Field) -> Result<Vec<String>> {
    if field.target.is_some() {
        return Ok(Vec::new());
    }
    if field.kind == FieldKind::Boolean {
        return Ok(vec!["true".into(), "false".into()]);
    }
    if let Some(registry) = &field.registry {
        return Ok(context
            .registry(registry)?
            .entries()
            .iter()
            .map(|entry| entry.key.clone())
            .collect());
    }
    Ok(Vec::new())
}

fn expand(name: &str, value: &str, choices: &[String], multi: bool) -> Result<String> {
    // Empty optional fields retain their existing clear/unset meaning.
    if choices.is_empty() || value.is_empty() {
        return Ok(value.into());
    }
    let choices = choices.iter().map(String::as_str).collect::<Vec<_>>();
    let result = if multi {
        value
            .split('|')
            .map(|item| command::resolve_value(name, item.trim(), &choices))
            .collect::<Result<Vec<_>>>()
            .map(|items| items.join("|"))
    } else {
        command::resolve_value(name, value, &choices)
    };
    result.map_err(|error| Error::validation(error.to_string()))
}

pub fn normalize(context: &Context, name: &str, args: &mut Args) -> Result<()> {
    let kind = context.catalog.find(name);
    let selected = if name == "batch" {
        args.value("template")
    } else if name == "workflow" && args.positional.get(1).is_some_and(|verb| verb == "preview") {
        args.positional.get(2).map(String::as_str)
    } else {
        None
    };
    let template = selected.and_then(|name| {
        context
            .workflows
            .iter()
            .find(|template| template.name == name)
    });
    for (option, value) in &mut args.values {
        if command::reserved(option) {
            continue;
        }
        if let Some(field) = kind.and_then(|kind| kind.field(option)).or_else(|| {
            template.and_then(|template| template.params.iter().find(|field| field.name == *option))
        }) {
            *value = expand(option, value, &field_values(context, field)?, field.multi)?;
        } else if let Some(registry) = kind
            .and_then(|kind| kind.numbering.as_ref())
            .map(|name| context.numbering(name))
            .transpose()?
            .and_then(|scheme| scheme.tokens.get(option))
        {
            let choices = context
                .registry(registry)?
                .entries()
                .iter()
                .map(|entry| entry.key.clone())
                .collect::<Vec<_>>();
            *value = expand(option, value, &choices, false)?;
        }
    }
    Ok(())
}
