//! Machine-readable projections of the same declarations used at runtime.

use crate::config::kind::{Field, Kind, Role};
use crate::execution::context::Context;
use crate::foundation::error::Result;

pub fn quote(value: &str) -> String {
    format!("\"{}\"", crate::terminal::output::escape(value))
}

pub fn strings_json(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|v| quote(v))
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub fn json(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => quote(s),
        toml::Value::Integer(n) => n.to_string(),
        toml::Value::Float(n) if n.is_finite() => n.to_string(),
        toml::Value::Float(_) => "null".into(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => quote(&d.to_string()),
        toml::Value::Array(items) => {
            format!("[{}]", items.iter().map(json).collect::<Vec<_>>().join(","))
        }
        toml::Value::Table(table) => object(table.iter().map(|(k, v)| (k.as_str(), json(v)))),
    }
}

pub fn object<'a>(items: impl IntoIterator<Item = (&'a str, String)>) -> String {
    format!(
        "{{{}}}",
        items
            .into_iter()
            .map(|(k, v)| format!("{}:{v}", quote(k)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub fn field_json(field: &Field) -> String {
    object([
        ("name", quote(&field.name)),
        (
            "terminal_style",
            match &field.terminal_style {
                None => "null".into(),
                Some(crate::terminal::style::Spec::Role(role)) => quote(role),
                Some(crate::terminal::style::Spec::Values(values)) => object(
                    values
                        .iter()
                        .map(|(key, value)| (key.as_str(), quote(value))),
                ),
            },
        ),
        ("type", quote(field.kind.name())),
        (
            "role",
            quote(if field.role == Role::Spec {
                "spec"
            } else {
                "status"
            }),
        ),
        (
            "write",
            quote(if field.role == Role::Status {
                "tool"
            } else if field.create_only {
                "create"
            } else {
                "always"
            }),
        ),
        ("required", field.required.to_string()),
        ("immutable", field.immutable.to_string()),
        (
            "required_when",
            field
                .required_when
                .as_ref()
                .map(|condition| {
                    object([
                        ("field", quote(&condition.field)),
                        ("equals", quote(&condition.value)),
                    ])
                })
                .unwrap_or_else(|| "null".into()),
        ),
        ("multi", field.multi.to_string()),
        (
            "cli_prefix",
            (field.target.is_none()
                && (field.registry.is_some()
                    || field.kind == crate::config::kind::FieldKind::Boolean))
                .to_string(),
        ),
        (
            "registry",
            field
                .registry
                .as_deref()
                .map(quote)
                .unwrap_or_else(|| "null".into()),
        ),
        (
            "target",
            field
                .target
                .as_deref()
                .map(quote)
                .unwrap_or_else(|| "null".into()),
        ),
        (
            "default",
            field
                .default
                .as_deref()
                .map(quote)
                .unwrap_or_else(|| "null".into()),
        ),
        (
            "auto",
            field
                .auto
                .map(|v| quote(v.name()))
                .unwrap_or_else(|| "null".into()),
        ),
        ("display", quote(&field.display)),
    ])
}

pub fn kind_json(context: &Context, kind: &Kind) -> Result<String> {
    let source = std::fs::read_to_string(context.workspace.resolve(&kind.source)).map_err(|e| {
        crate::foundation::error::io_error(&context.workspace.resolve(&kind.source), e)
    })?;
    let declaration = crate::config::decl::parse(&source, &kind.source)?;
    let verbs = crate::interface::command::verbs(kind)
        .iter()
        .map(|verb| {
            let options = crate::interface::command::action_inputs(
                kind.action(&verb.name)
                    .map(|a| a.handler.as_str())
                    .unwrap_or(""),
            );
            object([
                ("name", quote(&verb.name)),
                ("writes", verb.writes.to_string()),
                (
                    "inputs",
                    strings_json(
                        &options
                            .iter()
                            .filter(|name| crate::interface::command::kind_option(kind, name))
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>(),
                    ),
                ),
                ("about", quote(&verb.about)),
            ])
        })
        .collect::<Vec<_>>();
    let sort_parts = kind
        .numbering
        .as_deref()
        .map(|n| context.numbering(n))
        .transpose()?
        .map(|scheme| {
            let mut parts = vec![scheme.counter.clone()];
            parts.extend(scheme.tokens.keys().cloned());
            parts
        })
        .unwrap_or_default();
    Ok(object([
        ("version", "1".into()),
        ("kind", quote(&kind.name)),
        (
            "commit_policy",
            object([
                (
                    "auto_commit",
                    context.workspace.policy.auto_commit.to_string(),
                ),
                (
                    "mode",
                    quote(match context.workspace.policy.commit_mode {
                        crate::config::workspace::CommitMode::Required => "required",
                        crate::config::workspace::CommitMode::Auto => "auto",
                        crate::config::workspace::CommitMode::Default => "default",
                    }),
                ),
                ("message_option", quote("--message")),
                ("file_option", quote("--message-file")),
            ]),
        ),
        ("title", quote(&kind.title)),
        ("table", quote(kind.table_path())),
        ("key", strings_json(&kind.key)),
        ("sort_parts", strings_json(&sort_parts)),
        (
            "fields",
            format!(
                "[{}]",
                kind.fields
                    .iter()
                    .map(field_json)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
        ("verbs", format!("[{}]", verbs.join(","))),
        (
            "numbering",
            kind.numbering
                .as_deref()
                .map(|name| context.numbering(name))
                .transpose()?
                .map(|scheme| {
                    object([
                        ("name", quote(&scheme.name)),
                        ("counter", quote(&scheme.counter)),
                        (
                            "tokens",
                            object(
                                scheme
                                    .tokens
                                    .iter()
                                    .map(|(key, value)| (key.as_str(), quote(value))),
                            ),
                        ),
                    ])
                })
                .unwrap_or_else(|| "null".into()),
        ),
        (
            "actions",
            declaration
                .get("action")
                .map(json)
                .unwrap_or_else(|| "[]".into()),
        ),
        (
            "transitions",
            declaration
                .get("transition")
                .map(json)
                .unwrap_or_else(|| "[]".into()),
        ),
        (
            "relations",
            declaration
                .get("relation")
                .map(json)
                .unwrap_or_else(|| "[]".into()),
        ),
        (
            "command_options",
            format!(
                "[{}]",
                crate::interface::command::OPTIONS
                    .iter()
                    .filter(|option| crate::interface::command::kind_option(kind, option.name))
                    .map(|option| {
                        let applicable = crate::interface::command::verb_names(kind)
                            .into_iter()
                            .filter(|verb| {
                                crate::interface::command::kind_applies(kind, verb, option.name)
                            })
                            .collect::<Vec<_>>();
                        object([
                            ("name", quote(option.name)),
                            ("argument", quote(&option.argument())),
                            (
                                "description",
                                quote(crate::interface::command::about(&kind.name, option.name)),
                            ),
                            (
                                "help_order",
                                crate::interface::command::help_order(option.name).to_string(),
                            ),
                            (
                                "initial",
                                crate::interface::command::initial(option.name)
                                    .map(quote)
                                    .unwrap_or_else(|| "null".into()),
                            ),
                            (
                                "group",
                                quote(crate::interface::command::group(option.name)),
                            ),
                            (
                                "short",
                                option
                                    .short
                                    .map(|c| quote(&c.to_string()))
                                    .unwrap_or_else(|| "null".into()),
                            ),
                            (
                                "choices",
                                strings_json(&if option.name == "no" {
                                    crate::interface::choice::negatable(
                                        Some(context),
                                        &kind.name,
                                        "",
                                    )
                                    .into_iter()
                                    .map(|(name, _)| name)
                                    .collect::<Vec<_>>()
                                } else {
                                    crate::interface::command::choices(option.name)
                                        .iter()
                                        .map(|s| s.to_string())
                                        .collect::<Vec<_>>()
                                }),
                            ),
                            (
                                "cli_prefix",
                                (option.name == "no"
                                    || !crate::interface::command::choices(option.name).is_empty())
                                .to_string(),
                            ),
                            ("default_allowed", option.default.to_string()),
                            (
                                "negation",
                                match option.negation {
                                    None => "null".into(),
                                    Some(crate::interface::command::Negation::Reset) => object([
                                        ("behavior", quote("reset")),
                                        ("value", "null".into()),
                                    ]),
                                    Some(crate::interface::command::Negation::Value(value)) => {
                                        object([
                                            ("behavior", quote("value")),
                                            ("value", quote(value)),
                                        ])
                                    }
                                },
                            ),
                            (
                                "no_targets",
                                if option.name == "no" {
                                    object(applicable.iter().map(|verb| {
                                        (
                                            verb.as_str(),
                                            strings_json(
                                                &crate::interface::choice::negatable(
                                                    Some(context),
                                                    &kind.name,
                                                    verb,
                                                )
                                                .into_iter()
                                                .map(|(name, _)| name)
                                                .collect::<Vec<_>>(),
                                            ),
                                        )
                                    }))
                                } else {
                                    "null".into()
                                },
                            ),
                            ("verbs", strings_json(&applicable)),
                            (
                                "requires",
                                format!(
                                    "[{}]",
                                    crate::interface::command::choices(option.name)
                                        .iter()
                                        .flat_map(|value| {
                                            crate::interface::command::requirements(
                                                option.name,
                                                value,
                                            )
                                            .iter()
                                            .map(
                                                move |(required, allowed)| {
                                                    object([
                                                        ("when", quote(value)),
                                                        ("option", quote(required)),
                                                        (
                                                            "any_of",
                                                            strings_json(
                                                                &allowed
                                                                    .iter()
                                                                    .map(|v| v.to_string())
                                                                    .collect::<Vec<_>>(),
                                                            ),
                                                        ),
                                                    ])
                                                },
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join(",")
                                ),
                            ),
                            (
                                "conflicts",
                                strings_json(
                                    &crate::interface::command::conflicts(option.name)
                                        .iter()
                                        .map(|name| name.to_string())
                                        .collect::<Vec<_>>(),
                                ),
                            ),
                        ])
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
        ("declaration", json(&toml::Value::Table(declaration))),
    ]))
}
