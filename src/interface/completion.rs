//! Shell adapters query shared declarations; completion never runs command dispatch.

use crate::config::kind::{Field, FieldKind, Kind, Role};
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::interface::cli::Args;
use crate::interface::command;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Candidate {
    pub insert: String,
    pub label: String,
    pub description: String,
    pub kind: String,
}

fn candidate(value: &str, description: &str, kind: &str) -> Candidate {
    Candidate {
        insert: value.into(),
        label: value.into(),
        description: description.into(),
        kind: kind.into(),
    }
}

fn keys(context: &Context, kind: &Kind, column: usize, scope: &str) -> Result<Vec<Candidate>> {
    let Some(field) = kind.key.get(column) else {
        return Ok(Vec::new());
    };
    let records = crate::query::scope::load(context, kind, scope)?
        .into_iter()
        .map(|(record, _)| record)
        .collect::<Vec<_>>();
    Ok(records
        .iter()
        .filter_map(|record| {
            record.get(field).map(|key| {
                candidate(
                    key,
                    record
                        .get("title")
                        .map(String::as_str)
                        .unwrap_or(&kind.title),
                    "value",
                )
            })
        })
        .collect())
}

fn values(context: &Context, field: &Field) -> Result<Vec<Candidate>> {
    if field.kind == FieldKind::Path {
        return Ok(vec![candidate("", "file path", "path")]);
    }
    let choices = crate::interface::choice::field_values(context, field)?;
    if !choices.is_empty() {
        let registry = field
            .registry
            .as_deref()
            .map(|name| context.registry(name))
            .transpose()?;
        return Ok(choices
            .iter()
            .map(|value| {
                let description = registry
                    .and_then(|registry| {
                        registry.entries().iter().find(|entry| entry.key == *value)
                    })
                    .map(|entry| entry.display.as_str())
                    .unwrap_or("field value");
                candidate(value, description, "value")
            })
            .collect());
    }
    if let Some(target) = &field.target {
        return keys(context, context.catalog.require(target)?, 0, "all");
    }
    Ok(Vec::new())
}

struct Input {
    positionals: Vec<String>,
    options: BTreeMap<String, String>,
    waiting: Option<String>,
    literal: bool,
}

fn option_name(text: &str) -> Option<String> {
    if let Some(name) = text.strip_prefix("--") {
        Some(name.to_string())
    } else {
        text.strip_prefix('-')
            .filter(|value| value.len() == 1)
            .and_then(|value| command::long_of_short(value.chars().next().unwrap()))
            .map(str::to_string)
    }
}

fn input(words: &[String]) -> Input {
    let mut input = Input {
        positionals: Vec::new(),
        options: BTreeMap::new(),
        waiting: None,
        literal: false,
    };
    for word in words {
        if let Some(name) = input.waiting.take() {
            input.options.insert(name, word.clone());
            continue;
        }
        if input.literal {
            input.positionals.push(word.clone());
            continue;
        }
        if word == "--" {
            input.literal = true;
            continue;
        }
        if let Some((name, value)) = word.split_once('=')
            && let Some(name) = option_name(name)
        {
            input.options.insert(name, value.into());
            continue;
        }
        if let Some(body) = word
            .strip_prefix('-')
            .filter(|body| !body.starts_with('-') && !body.is_empty())
        {
            if let Ok(options) = command::short_options(body) {
                for (name, value) in options {
                    if command::is_switch(name) {
                        continue;
                    }
                    if let Some(value) = value {
                        input.options.insert(name.into(), value.into());
                    } else {
                        input.waiting = Some(name.into());
                    }
                }
            }
        } else if let Some(name) = option_name(word) {
            if command::option(&name).is_some_and(|spec| spec.switch()) || name.starts_with("no-") {
                continue;
            }
            input.waiting = Some(name);
        } else {
            input.positionals.push(word.clone());
        }
    }
    for (name, value) in &mut input.options {
        if !command::choices(name).is_empty()
            && let Ok(full) = command::resolve_choice(name, value)
        {
            *value = full;
        }
    }
    input
}

fn filter_candidates(context: &Context, kind: &Kind, expression: &str) -> Result<Vec<Candidate>> {
    // Only complete syntactically unambiguous slots; leave regex and unknown input alone.
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in expression.chars() {
        if let Some(delimiter) = quote {
            current.push(ch);
            if ch == delimiter {
                quote = None;
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            current.push(ch);
        } else if ch.is_whitespace() || ch == '(' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if quote.is_some() {
        return Ok(Vec::new());
    }
    let partial = if expression.chars().last().is_some_and(char::is_whitespace) {
        String::new()
    } else {
        std::mem::take(&mut current)
    };
    if !current.is_empty() {
        tokens.push(current);
    }
    let start = tokens
        .iter()
        .rposition(|word| word.eq_ignore_ascii_case("and") || word.eq_ignore_ascii_case("or"))
        .map(|index| index + 1)
        .unwrap_or(0);
    let terms = &tokens[start..];
    let candidates = match terms {
        [] => kind
            .fields
            .iter()
            .map(|field| candidate(&field.name, &field.display, "value"))
            .collect(),
        [_field] => [
            "=",
            "!=",
            "~",
            "!~",
            "in",
            "matches",
            "not matches",
            ">",
            ">=",
            "<",
            "<=",
        ]
        .iter()
        .map(|value| candidate(value, "filter operator", "value"))
        .collect(),
        [field, op] if !op.eq_ignore_ascii_case("matches") => kind
            .field(field)
            .map(|field| values(context, field))
            .transpose()?
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let prefix = &expression[..expression.len() - partial.len()];
    Ok(candidates
        .into_iter()
        .filter(|candidate| {
            candidate
                .insert
                .to_lowercase()
                .starts_with(&partial.to_lowercase())
        })
        .map(|mut candidate| {
            candidate.label = candidate.insert.clone();
            candidate.insert = format!("{prefix}{}", candidate.insert);
            candidate
        })
        .collect())
}

pub fn candidates(words: &[String], current: &str, root: Option<&str>) -> Result<Vec<Candidate>> {
    let mut input = input(words);
    let context = Context::open(input.options.get("root").map(String::as_str).or(root)).ok();
    if let Some(context) = &context {
        if let Some(name) = input.positionals.first().cloned() {
            let mut candidates = command::BUILTINS
                .iter()
                .map(|builtin| builtin.name)
                .collect::<Vec<_>>();
            candidates.extend(
                context
                    .catalog
                    .kinds()
                    .iter()
                    .map(|kind| kind.name.as_str()),
            );
            if let Ok(canonical) = crate::interface::cli::resolve(&name, &candidates) {
                input.positionals[0] = canonical.into();
            }
        }
        if let Some(verb) = input.positionals.get(1).cloned() {
            if let Some(kind) = input
                .positionals
                .first()
                .and_then(|name| context.catalog.find(name))
            {
                let names = command::verb_names(kind);
                if let Ok(canonical) = crate::interface::cli::resolve(
                    &verb,
                    &names.iter().map(String::as_str).collect::<Vec<_>>(),
                ) {
                    input.positionals[1] = canonical.into();
                }
            } else if let Some(builtin) = input
                .positionals
                .first()
                .and_then(|name| command::builtin(name))
                && !builtin.subcommands.is_empty()
                && let Ok(canonical) = crate::interface::cli::resolve(&verb, builtin.subcommands)
            {
                input.positionals[1] = canonical.into();
            }
        }
    }
    let command_name = input.positionals.first().map(String::as_str).unwrap_or("");
    let kind = context
        .as_ref()
        .and_then(|context| context.catalog.find(command_name));
    let verb = input.positionals.get(1).map(String::as_str).unwrap_or("");
    let template = context.as_ref().and_then(|context| {
        let selected = input
            .options
            .get("template")
            .map(String::as_str)
            .or_else(|| {
                (command_name == "workflow" && verb == "preview")
                    .then(|| input.positionals.get(2).map(String::as_str))
                    .flatten()
            });
        selected.and_then(|name| {
            context
                .workflows
                .iter()
                .find(|template| template.name == name)
        })
    });
    // Keep the original spelling when replacing a partially attached value.
    let inline = if input.literal || input.waiting.is_some() {
        None
    } else if let Some((spelling, value)) = current.split_once('=')
        && let Some(name) = option_name(spelling)
    {
        Some((name, format!("{spelling}="), value.to_string()))
    } else if let Some(body) = current
        .strip_prefix('-')
        .filter(|body| !body.starts_with('-'))
    {
        command::short_options(body).ok().and_then(|options| {
            options.last().and_then(|(name, value)| {
                value.map(|value| {
                    let stem = &current[..current.len() - value.len()];
                    (name.to_string(), stem.to_string(), value.to_string())
                })
            })
        })
    } else {
        None
    };
    let waiting = inline
        .as_ref()
        .map(|(name, _, _)| name.as_str())
        .or(input.waiting.as_deref());
    let prefix = inline
        .as_ref()
        .map(|(_, _, value)| value.as_str())
        .unwrap_or(current);
    let mut output = if let Some(option) = waiting {
        if option == "no" {
            crate::interface::choice::negatable(
                context.as_ref(),
                if command_name.is_empty() {
                    "help"
                } else {
                    command_name
                },
                verb,
            )
            .iter()
            .map(|(name, behavior)| {
                candidate(
                    name,
                    match behavior {
                        command::Negation::Value(value) => value,
                        command::Negation::Reset => "suppress configured default",
                    },
                    "value",
                )
            })
            .collect()
        } else if !command::choices(option).is_empty() {
            command::choices(option)
                .iter()
                .map(|value| candidate(value, "option value", "value"))
                .collect()
        } else if option == "template" {
            context
                .as_ref()
                .map(|context| {
                    context
                        .workflows
                        .iter()
                        .map(|template| candidate(&template.name, &template.title, "value"))
                        .collect()
                })
                .unwrap_or_default()
        } else if matches!(option, "file" | "message-file" | "root" | "from") {
            vec![candidate("", "file path", "path")]
        } else if let (Some(context), Some(kind)) = (&context, kind) {
            if option == "filter" {
                return filter_candidates(context, kind, prefix);
            }
            if option == "columns" || option == "sort" {
                let (stem, fragment) = prefix
                    .rsplit_once(',')
                    .map(|(stem, fragment)| (format!("{stem},"), fragment))
                    .unwrap_or((String::new(), prefix));
                return Ok(kind
                    .fields
                    .iter()
                    .filter(|field| {
                        field
                            .name
                            .to_lowercase()
                            .starts_with(&fragment.to_lowercase())
                    })
                    .map(|field| {
                        candidate(&format!("{stem}{}", field.name), &field.display, "value")
                    })
                    .collect());
            }
            if let Some(field) = kind.field(option) {
                values(context, field)?
            } else if let Some(registry) = kind
                .numbering
                .as_ref()
                .and_then(|name| context.numbering(name).ok())
                .and_then(|scheme| scheme.tokens.get(option))
            {
                context
                    .registry(registry)?
                    .entries()
                    .iter()
                    .map(|entry| candidate(&entry.key, &entry.display, "value"))
                    .collect()
            } else {
                Vec::new()
            }
        } else if let (Some(context), Some(template)) = (&context, template) {
            template
                .params
                .iter()
                .find(|field| field.name == option)
                .map(|field| values(context, field))
                .transpose()?
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    } else if !input.literal && current.starts_with('-') {
        let mut options = command::OPTIONS
            .iter()
            .filter(|option| {
                kind.map(|kind| command::kind_applies(kind, verb, option.name))
                    .unwrap_or_else(|| command::applies(option.name, command_name, verb, false))
            })
            .filter(|option| {
                option.global
                    || kind.is_some_and(|kind| {
                        command::kind_argument(
                            kind,
                            kind.numbering
                                .as_ref()
                                .and_then(|name| context.as_ref()?.numbering(name).ok()),
                            option.name,
                        )
                    })
                    || command::builtin(command_name)
                        .is_some_and(|builtin| command::builtin_option(builtin, option.name))
            })
            .flat_map(|option| {
                let mut choices = vec![candidate(
                    &format!("--{}", option.name),
                    command::about(command_name, option.name),
                    "option",
                )];
                if !current.starts_with("--")
                    && let Some(short) = option.short
                {
                    let mut alias = candidate(
                        &format!("-{short}"),
                        command::about(command_name, option.name),
                        "option",
                    );
                    alias.label = format!("-{short}, --{}", option.name);
                    choices.push(alias);
                }
                choices
            })
            .collect::<Vec<_>>();
        if let Some(kind) = kind {
            options.extend(
                kind.fields
                    .iter()
                    .filter(|field| {
                        !command::reserved(&field.name)
                            && match verb {
                                "new" => field.role == Role::Spec || field.create_only,
                                "set" => {
                                    field.role == Role::Spec
                                        && !field.create_only
                                        && !field.immutable
                                }
                                "list" => true,
                                _ => {
                                    kind.transition(verb).is_some()
                                        && field.role == Role::Spec
                                        && !field.immutable
                                        && !field.create_only
                                }
                            }
                    })
                    .map(|field| candidate(&format!("--{}", field.name), &field.display, "option")),
            );
            if let (Some(context), Some(name)) = (&context, &kind.numbering)
                && (verb.is_empty() || verb == "new")
            {
                options.extend(
                    context
                        .numbering(name)?
                        .tokens
                        .keys()
                        .map(|name| candidate(&format!("--{name}"), "numbering token", "option")),
                );
            }
        }
        if let Some(template) = template {
            options.extend(template.params.iter().map(|param| {
                candidate(&format!("--{}", param.name), "template parameter", "option")
            }));
        }
        options
    } else if input.positionals.is_empty() {
        let mut names = command::BUILTINS
            .iter()
            .map(|builtin| candidate(builtin.name, builtin.summary, "command"))
            .collect::<Vec<_>>();
        if let Some(context) = &context {
            names.extend(
                context
                    .catalog
                    .kinds()
                    .iter()
                    .map(|kind| candidate(&kind.name, &kind.title, "command")),
            );
        }
        names
    } else if input.positionals.len() == 1 {
        if command_name == "context" {
            context
                .as_ref()
                .map(|context| {
                    context
                        .catalog
                        .kinds()
                        .iter()
                        .map(|kind| candidate(&kind.name, &kind.title, "command"))
                        .collect()
                })
                .unwrap_or_default()
        } else if command_name == "help" {
            let mut names = command::BUILTINS
                .iter()
                .map(|builtin| candidate(builtin.name, builtin.summary, "command"))
                .collect::<Vec<_>>();
            if let Some(context) = &context {
                names.extend(
                    context
                        .catalog
                        .kinds()
                        .iter()
                        .map(|kind| candidate(&kind.name, &kind.title, "command")),
                );
            }
            names
        } else if let Some(kind) = kind {
            command::verbs(kind)
                .iter()
                .map(|verb| candidate(&verb.name, &verb.about, "command"))
                .collect()
        } else if let Some(builtin) = command::builtin(command_name) {
            builtin
                .subcommands
                .iter()
                .map(|name| candidate(name, builtin.summary, "command"))
                .collect()
        } else {
            Vec::new()
        }
    } else if let (Some(context), Some(kind)) = (&context, kind) {
        if verb == "new" {
            Vec::new()
        } else {
            keys(
                context,
                kind,
                input.positionals.len() - 2,
                if verb == "restore" {
                    "archived"
                } else {
                    input
                        .options
                        .get("data-scope")
                        .map(String::as_str)
                        .unwrap_or("current")
                },
            )?
        }
    } else if let Some(context) = &context {
        if command_name == "context" {
            if input.positionals.len() == 2 {
                context
                    .catalog
                    .find(verb)
                    .map(|kind| keys(context, kind, 0, "all"))
                    .transpose()?
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        } else if command_name == "workflow" {
            context
                .workflows
                .iter()
                .map(|template| candidate(&template.name, &template.title, "value"))
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    let multi = waiting
        .and_then(|name| {
            kind.and_then(|kind| kind.field(name)).or_else(|| {
                template
                    .and_then(|template| template.params.iter().find(|field| field.name == name))
            })
        })
        .is_some_and(|field| field.multi);
    let (value_stem, prefix) = if multi {
        prefix
            .rsplit_once('|')
            .map(|(stem, part)| (format!("{stem}|"), part.trim_start()))
            .unwrap_or((String::new(), prefix))
    } else {
        (String::new(), prefix)
    };
    output.retain(|candidate| {
        candidate.kind == "path"
            || candidate
                .insert
                .to_lowercase()
                .starts_with(&prefix.to_lowercase())
    });
    output.sort_by(|left, right| left.insert.cmp(&right.insert));
    output.dedup_by(|left, right| left.insert == right.insert);
    if !value_stem.is_empty() {
        for candidate in &mut output {
            candidate.insert = format!("{value_stem}{}", candidate.insert);
        }
    }
    if let Some((_, stem, _)) = inline {
        for candidate in &mut output {
            candidate.insert = format!("{stem}{}", candidate.insert);
        }
    }
    Ok(output)
}

pub fn command(args: &Args) -> Result<i32> {
    for name in args
        .values
        .keys()
        .chain(args.switches.iter())
        .chain(args.off.iter())
    {
        if !matches!(name.as_str(), "root" | "words-json" | "current" | "json") {
            return Err(Error::usage(format!("completion does not accept --{name}")));
        }
    }
    match args.positional.get(1).map(String::as_str) {
        Some("powershell") if args.positional.len() == 2 => {
            print!("{POWERSHELL}");
            Ok(0)
        }
        Some("complete") if args.positional.len() == 2 => {
            let words: Vec<String> = serde_json::from_str(args.value("words-json").unwrap_or("[]"))
                .map_err(|error| {
                    Error::usage(format!("--words-json needs an array of strings: {error}"))
                })?;
            let candidates = candidates(
                &words,
                args.value("current").unwrap_or(""),
                args.value("root"),
            )?;
            println!(
                "[{}]",
                candidates
                    .iter()
                    .map(|candidate| crate::config::schema::object([
                        ("insert", crate::config::schema::quote(&candidate.insert)),
                        ("label", crate::config::schema::quote(&candidate.label)),
                        (
                            "description",
                            crate::config::schema::quote(&candidate.description)
                        ),
                        ("kind", crate::config::schema::quote(&candidate.kind))
                    ]))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            Ok(0)
        }
        _ => Err(Error::usage(
            "usage: clerk completion powershell | complete --words-json <json> --current <word> --json",
        )),
    }
}

const POWERSHELL: &str = r#"# Load in this session or dot-source this file from your PowerShell profile.
Register-ArgumentCompleter -Native -CommandName clerk -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $items = @($commandAst.CommandElements)
    $words = @()
    for ($i = 1; $i -lt $items.Count; $i++) {
        $element = $items[$i]
        if ($element.Extent.EndOffset -gt $cursorPosition) { break }
        if ($wordToComplete -ne '' -and $element.Extent.EndOffset -eq $cursorPosition) { break }
        if ($element -is [System.Management.Automation.Language.StringConstantExpressionAst]) { $words += $element.Value }
        else { $words += $element.Extent.Text }
    }
    $exe = $items[0].Value
    $current = $wordToComplete
    if ($current.Length -ge 1 -and ($current[0] -eq "'" -or $current[0] -eq '"')) { $current = $current.Substring(1) }
    if ($current.Length -ge 1 -and ($current[-1] -eq "'" -or $current[-1] -eq '"')) { $current = $current.Substring(0, $current.Length - 1) }
    $payload = ConvertTo-Json -InputObject @($words) -Compress
    $results = & $exe completion complete --words-json $payload "--current=$current" --json 2>$null | ConvertFrom-Json
    foreach ($result in $results) {
        if ($result.kind -eq 'path') { [System.Management.Automation.CompletionCompleters]::CompleteFilename($wordToComplete); continue }
        $insert = $result.insert
        if ($insert -match '[\s''"`$;|&(){}<>]') { $insert = "'" + $insert.Replace("'", "''") + "'" }
        $description = if ([string]::IsNullOrEmpty($result.description)) { $result.label } else { $result.description }
        [System.Management.Automation.CompletionResult]::new($insert, $result.label, 'ParameterValue', $description)
    }
}
"#;
