//! Declarative parameters expand into ordinary workflow data, never source text.

use crate::config::decl;
use crate::config::kind::Field;
use crate::config::schema::{json, object, quote};
use crate::config::workspace::Workspace;
use crate::execution::context::Context;
use crate::execution::engine::Values;
use crate::foundation::error::{Error, Result};
use crate::interface::cli::Args;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub struct Template {
    pub name: String,
    pub title: String,
    pub source: String,
    pub params: Vec<Field>,
    pub digest: String,
    table: toml::Table,
    base: PathBuf,
    original: String,
}

pub fn load_templates(workspace: &Workspace) -> Result<Vec<Template>> {
    let kinds = workspace.kinds_dir();
    let dir = kinds
        .parent()
        .expect("kinds directory parent")
        .join("workflows");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = std::fs::read_dir(&dir)
        .map_err(|e| crate::foundation::error::io_error(&dir, e))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| crate::foundation::error::io_error(&dir, e))
        })
        .collect::<Result<Vec<_>>>()?;
    paths.sort();
    let mut out = Vec::new();
    for path in paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
    {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if name.is_empty()
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(Error::usage(
                "workflow template name must be an ASCII identifier",
            ));
        }
        let source = workspace.relative(&path);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| crate::foundation::error::io_error(&path, e))?;
        let mut table = decl::parse(&text, &source)?;
        decl::only_keys(&table, &["version", "title", "param", "step"], &source)?;
        let title = decl::required_text(&table, "title", &source)?;
        let params = table
            .remove("param")
            .unwrap_or_else(|| toml::Value::Array(Vec::new()));
        let array = params
            .as_array()
            .ok_or_else(|| Error::usage("workflow.param must be an array"))?;
        for item in array {
            let item = item
                .as_table()
                .ok_or_else(|| Error::usage("workflow param must be a table"))?;
            decl::only_keys(
                item,
                &[
                    "name", "type", "required", "default", "multi", "registry", "target", "display",
                ],
                "workflow param",
            )?;
            let name = decl::required_text(item, "name", "workflow param")?;
            if name.is_empty()
                || crate::interface::command::option(&name).is_some()
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                return Err(Error::usage(format!(
                    "workflow parameter `{name}` conflicts with an option or is not an identifier"
                )));
            }
            for (key, value) in item {
                if matches!(key.as_str(), "required" | "multi") {
                    if !value.is_bool() {
                        return Err(Error::usage(format!("param.{key} must be a boolean")));
                    }
                } else if !value.is_str() {
                    return Err(Error::usage(format!("param.{key} must be a string")));
                }
            }
        }
        let params = array
            .iter()
            .map(|item| Field::parse(item.as_table().expect("validated parameter"), &source))
            .collect::<Result<Vec<_>>>()?;
        table.remove("title");
        crate::collaboration::workflow::parse_steps(
            &table,
            path.parent().expect("template parent"),
            false,
        )?;
        out.push(Template {
            name: name.into(),
            title,
            source,
            params,
            digest: format!("{:x}", Sha256::digest(text.as_bytes())),
            table,
            base: path.parent().expect("template parent").to_path_buf(),
            original: text,
        });
    }
    Ok(out)
}

fn template<'a>(context: &'a Context, name: &str) -> Result<&'a Template> {
    context
        .workflows
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| {
            Error::usage(format!(
                "unknown workflow template `{name}`; run `clerk workflow list`"
            ))
        })
}

pub fn validate_templates(context: &Context) -> Result<()> {
    for template in &context.workflows {
        let mut seen = std::collections::BTreeSet::new();
        for param in &template.params {
            if !seen.insert(&param.name) {
                return Err(Error::usage(format!(
                    "duplicate workflow parameter `{}`",
                    param.name
                )));
            }
            if let Some(target) = &param.target
                && context.catalog.require(target)?.key.len() != 1
            {
                return Err(Error::usage(format!(
                    "workflow ref target `{target}` requires a single key"
                )));
            }
            if let Some(default) = &param.default {
                if default.trim().is_empty() {
                    if param.required {
                        return Err(Error::usage(format!(
                            "required workflow parameter `{}` has an empty default",
                            param.name
                        )));
                    }
                } else {
                    context.check_field(param, default)?;
                }
            }
        }
        // Validate names and placeholders without resolving record values or body files.
        let dummy: Values = template
            .params
            .iter()
            .map(|p| (p.name.clone(), String::new()))
            .collect();
        substitute(&toml::Value::Table(template.table.clone()), &dummy)?;
        let steps =
            crate::collaboration::workflow::parse_steps(&template.table, &template.base, false)?;
        crate::collaboration::workflow::validate_plan(context, &steps)?;
    }
    Ok(())
}

pub fn accepts_option(context: &Context, command: &str, args: &Args, name: &str) -> Result<bool> {
    let selected = if command == "batch" {
        args.value("template")
    } else if command == "workflow" && args.positional.get(1).is_some_and(|s| s == "preview") {
        args.positional.get(2).map(String::as_str)
    } else {
        None
    };
    Ok(selected
        .map(|selected| {
            template(context, selected).map(|t| t.params.iter().any(|p| p.name == name))
        })
        .transpose()?
        .unwrap_or(false))
}

fn substitute(value: &toml::Value, params: &Values) -> Result<toml::Value> {
    Ok(match value {
        toml::Value::String(text) => {
            let mut rest = text.as_str();
            let mut out = String::new();
            while let Some(at) = rest.find("${params.") {
                out.push_str(&rest[..at]);
                rest = &rest[at + 9..];
                let end = rest
                    .find('}')
                    .ok_or_else(|| Error::usage("unclosed template parameter"))?;
                let name = &rest[..end];
                let value = params
                    .get(name)
                    .ok_or_else(|| Error::usage(format!("unknown template parameter `{name}`")))?;
                // Parameters remain literal even when they contain runtime alias syntax.
                out.push_str(&value.replace("${", "$${"));
                rest = &rest[end + 1..];
            }
            out.push_str(rest);
            toml::Value::String(out)
        }
        toml::Value::Array(items) => toml::Value::Array(
            items
                .iter()
                .map(|v| substitute(v, params))
                .collect::<Result<_>>()?,
        ),
        toml::Value::Table(table) => toml::Value::Table(
            table
                .iter()
                .map(|(k, v)| Ok((k.clone(), substitute(v, params)?)))
                .collect::<Result<_>>()?,
        ),
        other => other.clone(),
    })
}

fn expanded(context: &Context, template: &Template, args: &Args) -> Result<toml::Table> {
    let mut params = Values::new();
    for param in &template.params {
        let value = args
            .value(&param.name)
            .map(str::to_string)
            .or_else(|| param.default.clone())
            .unwrap_or_default();
        if param.required && value.trim().is_empty() {
            return Err(Error::usage(format!(
                "workflow {} requires --{}",
                template.name, param.name
            )));
        }
        if !value.trim().is_empty() {
            context.check_field(param, &value)?;
        }
        params.insert(param.name.clone(), value);
    }
    let mut table = substitute(&toml::Value::Table(template.table.clone()), &params)?
        .as_table()
        .expect("table")
        .clone();
    if let Some(steps) = table.get_mut("step").and_then(toml::Value::as_array_mut) {
        for (index, step) in steps.iter_mut().enumerate() {
            if let Some(keys) = step.get_mut("keys").and_then(toml::Value::as_array_mut) {
                for (number, key) in keys.iter_mut().enumerate() {
                    let original = template.table["step"][index]["keys"][number]
                        .as_str()
                        .expect("validated key");
                    let value = key.as_str().expect("expanded key");
                    if original.starts_with("${params.") && value.starts_with('@') {
                        *key = toml::Value::String(format!("@{value}"));
                    }
                }
            }
            if let Some(file) = step.get_mut("file") {
                let literal = file.as_str().expect("validated file").replace("$${", "${");
                let path = template.base.join(literal);
                *file = toml::Value::String(path.to_string_lossy().into());
            }
        }
    }
    Ok(table)
}

fn template_json(template: &Template) -> String {
    object([
        ("version", "1".into()),
        ("name", quote(&template.name)),
        ("title", quote(&template.title)),
        ("source", quote(&template.source)),
        ("sha256", quote(&template.digest)),
        (
            "params",
            format!(
                "[{}]",
                template
                    .params
                    .iter()
                    .map(crate::config::schema::field_json)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
        (
            "workflow",
            json(&toml::Value::Table(template.table.clone())),
        ),
    ])
}

pub fn command(context: &Context, args: &Args) -> Result<i32> {
    if args.switch("dry-run") || args.value("request").is_some() {
        return Err(Error::usage(
            "workflow inspection is read-only; use batch --template for execution or dry-run",
        ));
    }
    let sub = args.positional.get(1).map(String::as_str).unwrap_or("list");
    if sub == "list" {
        if args.positional.len() > 2 {
            return Err(Error::usage("workflow list takes no template name"));
        }
        if args.switch("json") {
            println!(
                "[{}]",
                context
                    .workflows
                    .iter()
                    .map(template_json)
                    .collect::<Vec<_>>()
                    .join(",")
            );
        } else {
            let rows = context
                .workflows
                .iter()
                .map(|template| vec![template.name.clone(), template.title.clone()])
                .collect::<Vec<_>>();
            let headers = vec!["template".into(), "title".into()];
            let output = crate::terminal::text::table(
                crate::interface::cli::headers_shown(args)?.then_some(headers.as_slice()),
                &rows,
                args.max_width(),
                args.switch("compact"),
            );
            crate::terminal::reader::plain(args, &output)?;
        }
        return Ok(0);
    }
    if args.positional.len() != 3 {
        return Err(Error::usage(
            "workflow show/preview requires a template name",
        ));
    }
    let template = template(context, &args.positional[2])?;
    if sub == "show" {
        if args.switch("json") {
            println!("{}", template_json(template));
        } else {
            let compact = args.switch("compact");
            let reading = crate::terminal::size::reading(
                args.switch("plain"),
                args.value("width").and_then(|value| value.parse().ok()),
            );
            let mut output = String::new();
            output.push_str(&crate::terminal::text::pairs_with(
                &[
                    ("template".into(), template.name.clone()),
                    ("title".into(), template.title.clone()),
                    ("source".into(), template.source.clone()),
                ],
                0,
                compact,
            ));
            let params = template
                .params
                .iter()
                .map(|param| {
                    (
                        format!("--{} <{}>", param.name, param.kind.name()),
                        format!(
                            "{}{}",
                            if param.required {
                                "required"
                            } else {
                                "optional"
                            },
                            param
                                .default
                                .as_ref()
                                .map(|v| format!(" default={v}"))
                                .unwrap_or_default()
                        ),
                    )
                })
                .collect::<Vec<_>>();
            output.push_str(&crate::terminal::text::group(
                "Parameters:",
                "",
                &crate::terminal::text::pairs_with(&params, 0, compact),
            ));
            output.push_str(&crate::terminal::text::group(
                "Definition:",
                "",
                &reading.literal(&template.original, 1),
            ));
            crate::terminal::reader::plain(args, &output)?;
        }
    } else if sub == "preview" {
        let table = expanded(context, template, args)?;
        if args.switch("json") {
            println!("{}", json(&toml::Value::Table(table)));
        } else {
            println!("{table}");
        }
    } else {
        return Err(Error::usage("workflow expects list, show or preview"));
    }
    Ok(0)
}

pub fn dispatch_batch(context: &mut Context, args: &Args) -> Result<i32> {
    if args.positional.len() != 1 {
        return Err(Error::usage(
            "batch takes --file or --template, not positional arguments",
        ));
    }
    if args.switch("preview") && args.switch("dry-run") {
        return Err(Error::usage(
            "--preview and --dry-run are mutually exclusive",
        ));
    }
    if args.value("input-format").is_some() && args.value("file").is_none() {
        return Err(Error::usage("--input-format requires --file"));
    }
    match (args.value("file"), args.value("template")) {
        (Some(file), None) => {
            let steps = crate::collaboration::workflow::load(
                file,
                args.value("input-format").unwrap_or("toml"),
            )?;
            if args.switch("preview") {
                return crate::execution::batch::preview(context, &steps, args.switch("json"));
            }
            crate::collaboration::workflow::run_steps(
                context,
                steps,
                args.switch("dry-run"),
                args.switch("json"),
                None,
            )
        }
        (None, Some(name)) => {
            let template = template(context, name)?;
            let table = expanded(context, template, args)?;
            let steps = crate::collaboration::workflow::parse_steps(&table, &template.base, true)?;
            if args.switch("preview") {
                return crate::execution::batch::preview(context, &steps, args.switch("json"));
            }
            let origin = [
                ("name".into(), toml::Value::String(template.name.clone())),
                (
                    "source".into(),
                    toml::Value::String(template.source.clone()),
                ),
                (
                    "sha256".into(),
                    toml::Value::String(template.digest.clone()),
                ),
            ]
            .into_iter()
            .collect();
            crate::collaboration::workflow::run_steps(
                context,
                steps,
                args.switch("dry-run"),
                args.switch("json"),
                Some(origin),
            )
        }
        _ => Err(Error::usage(
            "batch requires exactly one of --file or --template",
        )),
    }
}
