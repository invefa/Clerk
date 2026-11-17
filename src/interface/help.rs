//! Build structured read-only help projections from shared command declarations.

use crate::config::kind::{Kind, Role};
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::interface::command::{self, BUILTINS, Builtin};
use crate::terminal::help::{OptionEntry, Page};

fn notes(page: &mut Page) {
    page.text("\nNotes:\n  Enum values accept unique prefixes; full values win.\n  Use --name=value or -Xvalue for attached values; -- ends options.\n  --no <option> disables an option or suppresses its configured default.\n  Agents and scripts use full command names, options and values.\n");
}

fn entry(
    context: Option<&Context>,
    owner: &str,
    verb: &str,
    spec: &command::OptionSpec,
) -> OptionEntry {
    let choices = if spec.name == "no" {
        crate::interface::choice::negatable(context, owner, verb)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    } else {
        command::choices(spec.name)
            .iter()
            .map(|value| value.to_string())
            .collect()
    };
    let argument = spec.argument();
    let default = context
        .and_then(|context| {
            context
                .command_defaults(owner, verb)
                .into_iter()
                .find(|(name, _)| name == spec.name)
                .map(|(_, value)| value)
        })
        .or_else(|| command::initial(spec.name).map(str::to_string));
    OptionEntry {
        indent: 2,
        name: spec.name.into(),
        short: spec.short,
        argument,
        description: command::about(owner, spec.name).into(),
        choices,
        default,
        group: if owner == "batch" && matches!(spec.name, "file" | "template" | "input-format") {
            "Input"
        } else {
            let group = command::group(spec.name);
            if group == "Presentation" {
                "Output"
            } else {
                group
            }
        }
        .into(),
    }
}

fn ordered(mut entries: Vec<OptionEntry>) -> Vec<OptionEntry> {
    entries.sort_by_key(|entry| {
        (
            command::group_order(&entry.group),
            command::help_order(&entry.name),
        )
    });
    entries
}

fn introduction() -> Page {
    let mut page = Page::default();
    page.text("clerk - workspace manager for managed design documents\n\nUsage:\n  clerk <kind> <verb> <key>... [options]\n  clerk <command> [options]\n  clerk help [command]\n");
    for group in [
        "Inspect",
        "Workflows",
        "Collaboration",
        "Maintenance",
        "Help and integration",
    ] {
        page.text(format!("\n{group}:\n"));
        page.pairs(
            BUILTINS
                .iter()
                .filter(|builtin| command::builtin_group(builtin.name) == group)
                .map(|builtin| (builtin.name.into(), builtin.summary.into()))
                .collect(),
            2,
        );
    }
    page
}

fn overview_end(page: &mut Page, context: Option<&Context>) {
    page.text("\nCommon options:\n");
    page.options(
        ["root", "no", "help", "version"]
            .iter()
            .map(|name| {
                let mut option = entry(
                    context,
                    "help",
                    "",
                    command::option(name).expect("common option"),
                );
                option.group.clear();
                option
            })
            .collect(),
    );
    page.text("\nNext:\n  clerk <command> --help\n  clerk <kind> <verb> --help\n");
}

pub fn fallback() -> Page {
    let mut page = introduction();
    overview_end(&mut page, None);
    page
}

pub fn overview(context: &Context) -> Page {
    let mut page = introduction();
    page.text("\nKinds:\n");
    let mut kinds = context.catalog.kinds().to_vec();
    kinds.sort_by(|left, right| left.name.cmp(&right.name));
    page.columns(
        kinds
            .iter()
            .map(|kind| {
                vec![
                    kind.name.clone(),
                    kind.title.clone(),
                    kind.description.clone().unwrap_or_default(),
                ]
            })
            .collect(),
        2,
    );
    overview_end(&mut page, Some(context));
    page
}

fn defaults_block(page: &mut Page, context: &Context, name: &str) {
    let mut rows = Vec::new();
    for (key, value) in &context.workspace.commands {
        let (owner, verb) = key.split_once('.').unwrap_or((key.as_str(), "(all)"));
        if owner != name {
            continue;
        }
        let Some(table) = value.as_table() else {
            continue;
        };
        let values = table
            .iter()
            .filter_map(|(key, value)| {
                command::default_value(key, value).map(|value| {
                    if command::is_switch(key) {
                        key.clone()
                    } else {
                        format!("{key}={value}")
                    }
                })
            })
            .collect::<Vec<_>>();
        if !values.is_empty() {
            rows.push((verb.to_string(), values.join(", ")));
        }
    }
    if !rows.is_empty() {
        rows.sort();
        page.text("\nWorkspace defaults:\n");
        page.pairs(rows, 2);
    }
}

pub fn usage_page(kind: &Kind) -> Page {
    let mut page = Page::default();
    page.text(format!(
        "clerk {} - {}\n\nUsage:\n  clerk {} <verb> [options]\n",
        kind.name, kind.title, kind.name
    ));
    if let Some(description) = &kind.description {
        page.text(format!("\n{description}\n"));
    }
    page.text("\nVerbs:\n");
    let mut groups: Vec<(String, String)> = Vec::new();
    for verb in command::verbs(kind) {
        if let Some((_, names)) = groups.iter_mut().find(|(group, _)| group == verb.group) {
            names.push_str(&format!(", {}", verb.name));
        } else {
            groups.push((verb.group.into(), verb.name));
        }
    }
    page.pairs(groups, 2);
    page.text(format!("\nExamples:\n  clerk {} list\n  clerk {} show <id>\n  clerk {} new --title <text>\n\nRun `clerk {} --help` for fields, transitions and options.\n", kind.name, kind.name, kind.name, kind.name));
    page
}

fn canonical(context: &Context, name: &str) -> Result<String> {
    let mut candidates = BUILTINS
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
    command_name(name, &candidates).or_else(|error| {
        context
            .catalog
            .find(name)
            .map(|kind| kind.name.clone())
            .ok_or(error)
    })
}

fn command_name(name: &str, candidates: &[&str]) -> Result<String> {
    crate::interface::cli::resolve(name, candidates)
        .map(str::to_string)
        .map_err(|error| {
            if error.message().contains("ambiguous") {
                error
            } else {
                Error::usage(format!("no page for `{name}`; try clerk help"))
            }
        })
}

pub fn page(context: &Context, name: &str) -> Result<Page> {
    selected_page(context, name, None)
}

pub fn selected_page(context: &Context, name: &str, verb: Option<&str>) -> Result<Page> {
    let name = canonical(context, name)?;
    if let Some(kind) = context.catalog.find(&name) {
        if let Some(verb) = verb {
            let names = command::verb_names(kind);
            let verb = crate::interface::cli::resolve(
                verb,
                &names.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
            return Ok(kind_verb_page(context, kind, verb));
        }
        return Ok(kind_page(context, kind));
    }
    let builtin = command::builtin(&name).expect("canonical builtin");
    let selected = verb
        .filter(|_| !builtin.subcommands.is_empty())
        .map(|verb| crate::interface::cli::resolve(verb, builtin.subcommands))
        .transpose()?;
    Ok(builtin_page(context, builtin, selected))
}

fn builtin_page(context: &Context, builtin: &Builtin, selected: Option<&str>) -> Page {
    let mut page = Page::default();
    page.text(format!(
        "clerk {}{} - {}\n\nUsage:\n",
        builtin.name,
        selected.map(|verb| format!(" {verb}")).unwrap_or_default(),
        builtin.summary
    ));
    for line in builtin.usage {
        if let Some(verb) = selected {
            let token = line.split_whitespace().nth(2).unwrap_or("");
            if !token
                .trim_matches(['<', '>'])
                .split('|')
                .any(|name| name == verb)
            {
                continue;
            }
            page.text(format!("  {}\n", line.replacen(token, verb, 1)));
        } else {
            page.text(format!("  {line}\n"));
        }
    }
    if selected.is_none() && !builtin.subcommands.is_empty() {
        page.text("\nSubcommands:\n");
        page.pairs(
            builtin
                .subcommands
                .iter()
                .map(|verb| {
                    (
                        verb.to_string(),
                        format!("clerk {} {verb} --help", builtin.name),
                    )
                })
                .collect(),
            2,
        );
    }
    page.text("\nOptions:\n");
    page.options(ordered(
        command::OPTIONS
            .iter()
            .filter(|option| {
                command::builtin_option(builtin, option.name)
                    && command::applies(option.name, builtin.name, selected.unwrap_or(""), false)
            })
            .map(|option| entry(Some(context), builtin.name, selected.unwrap_or(""), option))
            .collect(),
    ));
    if matches!(builtin.name, "workflow" | "batch")
        && selected.is_none_or(|verb| builtin.name == "workflow" && verb == "preview")
        && !context.workflows.is_empty()
    {
        page.text("\nWorkflow templates:\n");
        for template in &context.workflows {
            page.text(format!("  {} - {}\n", template.name, template.title));
            page.options(
                template
                    .params
                    .iter()
                    .map(|param| OptionEntry {
                        indent: 4,
                        name: param.name.clone(),
                        short: None,
                        argument: format!("<{}>", param.kind.name()),
                        description: if param.required {
                            "required"
                        } else {
                            "optional"
                        }
                        .into(),
                        choices: crate::interface::choice::field_values(context, param)
                            .unwrap_or_default(),
                        default: param.default.clone(),
                        group: String::new(),
                    })
                    .collect(),
            );
        }
    }
    defaults_block(&mut page, context, builtin.name);
    let examples = builtin
        .examples
        .iter()
        .filter(|line| selected.is_none_or(|verb| line.split_whitespace().nth(2) == Some(verb)))
        .collect::<Vec<_>>();
    if !examples.is_empty() {
        page.text("\nExamples:\n");
        for example in examples {
            page.text(format!("  {example}\n"));
        }
    }
    notes(&mut page);
    page
}

fn kind_options(context: &Context, kind: &Kind, verb: Option<&str>) -> Vec<OptionEntry> {
    let mut rows = command::OPTIONS
        .iter()
        .filter(|option| {
            (option.global
                || command::kind_option(kind, option.name) && kind.field(option.name).is_none())
                && verb.is_none_or(|verb| command::kind_applies(kind, verb, option.name))
        })
        .map(|option| entry(Some(context), &kind.name, verb.unwrap_or(""), option))
        .collect::<Vec<_>>();
    if verb.is_none_or(|verb| {
        matches!(verb, "list" | "new" | "set") || kind.transition(verb).is_some()
    }) {
        rows.push(OptionEntry {
            indent: 2,
            name: "<field>".into(),
            short: None,
            argument: "<value>".into(),
            description: if verb == Some("list") {
                "equality filter on one field"
            } else {
                "declared field value; see the kind field permissions"
            }
            .into(),
            choices: Vec::new(),
            default: None,
            group: "General".into(),
        });
    }
    if verb.is_none_or(|verb| verb == "new")
        && let Some(scheme) = kind
            .numbering
            .as_ref()
            .and_then(|name| context.numbering(name).ok())
    {
        rows.extend(scheme.tokens.iter().map(|(name, registry)| OptionEntry {
            indent: 2,
            name: name.clone(),
            short: None,
            argument: "<value>".into(),
            description: format!("numbering token from {registry}"),
            choices: Vec::new(),
            default: None,
            group: "General".into(),
        }));
    }
    ordered(rows)
}

fn kind_verb_page(context: &Context, kind: &Kind, verb: &str) -> Page {
    let mut page = Page::default();
    page.text(format!(
        "clerk {} {verb} - {}\n\nUsage:\n  clerk {} {verb}{} [options]\n\nOptions:\n",
        kind.name,
        kind.title,
        kind.name,
        if matches!(verb, "list" | "new") {
            ""
        } else {
            " <key>"
        }
    ));
    page.options(kind_options(context, kind, Some(verb)));
    page.text(format!(
        "\nRun `clerk {} --help` for fields, transitions and declarations.\n",
        kind.name
    ));
    notes(&mut page);
    page
}

fn kind_page(context: &Context, kind: &Kind) -> Page {
    let mut page = Page::default();
    page.text(format!("clerk {} - {}\n", kind.name, kind.title));
    if let Some(description) = &kind.description {
        page.text(format!("\n{description}\n"));
    }
    page.text(format!(
        "\nUsage:\n  clerk {} <verb> [options]\n\nVerbs:\n",
        kind.name
    ));
    page.pairs(
        command::verbs(kind)
            .iter()
            .map(|verb| (verb.name.clone(), verb.about.clone()))
            .collect(),
        2,
    );
    page.text("\nOptions:\n");
    page.options(kind_options(context, kind, None));
    page.text("\nDeclaration:\n");
    page.pairs(
        vec![
            ("declaration:".into(), kind.source.clone()),
            (
                "storage:".into(),
                format!(
                    "{} {}",
                    if kind.document_dir().is_some() {
                        "document"
                    } else {
                        "table"
                    },
                    kind.table_path()
                ),
            ),
            ("key:".into(), kind.key.join(", ")),
        ],
        0,
    );
    page.text("\nFields:\n");
    let rows = kind
        .fields
        .iter()
        .map(|field| {
            let mut notes = Vec::new();
            if let Some(registry) = &field.registry {
                notes.push(format!("registry {registry}"));
            }
            if let Some(target) = &field.target {
                notes.push(format!("target {target}"));
            }
            notes.push(
                if field.role == Role::Status {
                    "tool maintained"
                } else if field.create_only
                    || kind
                        .document_filename()
                        .is_some_and(|pattern| pattern.contains(&format!("{{{}}}", field.name)))
                {
                    "create only"
                } else {
                    "create and update"
                }
                .into(),
            );
            if field.multi {
                notes.push("multi".into());
            }
            if field.immutable {
                notes.push("immutable after creation".into());
            }
            if field.required {
                notes.push("required".into());
            }
            vec![
                field.name.clone(),
                field.display.clone(),
                field.kind.name().into(),
                notes.join(", "),
            ]
        })
        .collect();
    page.table(
        ["field", "display", "type", "notes"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        rows,
    );
    if !kind.transitions.is_empty() {
        page.text("\nTransitions:\n");
        page.pairs(
            kind.transitions
                .iter()
                .map(|transition| {
                    let mut note = format!("-> {}", transition.to);
                    if !transition.from.is_empty() {
                        note.push_str(&format!(" from {}", transition.from.join(", ")));
                    }
                    if !transition.requires.is_empty() {
                        note.push_str(&format!(" requires {}", transition.requires.join(", ")));
                    }
                    (transition.name.clone(), note)
                })
                .collect(),
            2,
        );
    }
    if !kind.actions.is_empty() {
        page.text("\nActions:\n");
        page.pairs(
            kind.actions
                .iter()
                .map(|action| {
                    (
                        action.name.clone(),
                        format!(
                            "handler {}{}",
                            action.handler,
                            if action.fields.is_empty() {
                                String::new()
                            } else {
                                format!(" fields {}", action.fields.join(", "))
                            }
                        ),
                    )
                })
                .collect(),
            2,
        );
    }
    if !kind.relations.is_empty() {
        page.text("\nRelations:\n");
        page.pairs(
            kind.relations
                .iter()
                .map(|relation| (relation.name.clone(), format!("-> {}", relation.target)))
                .collect(),
            2,
        );
    }
    if !kind.views.is_empty() {
        page.text("\nViews:\n");
        page.pairs(
            kind.views
                .iter()
                .map(|view| {
                    let mut note = view.renderer.clone().unwrap_or_else(|| "table".into());
                    if let Some(region) = &view.region {
                        note.push_str(&format!(" region {region}"));
                    }
                    if let Some(target) = &view.target {
                        note.push_str(&format!(" -> {target}"));
                    }
                    if !view.columns.is_empty() {
                        note.push_str(&format!(" columns {}", view.columns.join(", ")));
                    }
                    (view.name.clone(), note)
                })
                .collect(),
            2,
        );
    }
    defaults_block(&mut page, context, &kind.name);
    notes(&mut page);
    page
}
