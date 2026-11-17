//! The loaded workspace state shared by commands.

use std::collections::BTreeMap;

use crate::config::identity::Identity;
use crate::config::kind::Catalog;
use crate::config::registry::Registry;
use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};
use crate::storage::numbering::{Registries, Scheme};

/// Everything a command needs: layout, declarations, registries and identity.
pub struct Context {
    pub workspace: Workspace,
    pub catalog: Catalog,
    pub registries: BTreeMap<String, Registry>,
    pub numbering: Vec<Scheme>,
    pub workflows: Vec<crate::collaboration::workflow::Template>,
    identity: Option<Identity>,
}

impl Context {
    pub fn open(root: Option<&str>) -> Result<Self> {
        let workspace = Workspace::open(root)?;
        let catalog = Catalog::load(&workspace)?;
        let numbering = load_numbering(&workspace)?;
        let workflows = crate::collaboration::workflow::load_templates(&workspace)?;

        let mut names: Vec<String> = Vec::new();
        for template in &workflows {
            for param in &template.params {
                if let Some(name) = &param.registry {
                    names.push(name.clone());
                }
            }
        }
        for kind in catalog.kinds() {
            for field in &kind.fields {
                if let Some(name) = &field.registry {
                    names.push(name.clone());
                }
            }
            if let Some(state) = &kind.state {
                names.push(state.registry.clone());
            }
            for view in &kind.views {
                for entry in &view.entries {
                    if let Some(name) = &entry.registry {
                        names.push(name.clone());
                    }
                }
                if view.renderer.as_deref() == Some("registry")
                    && let Some(name) = &view.source
                {
                    names.push(name.clone());
                }
            }
        }
        for scheme in &numbering {
            for name in scheme.tokens.values() {
                names.push(name.clone());
            }
        }
        names.sort();
        names.dedup();

        let mut registries = BTreeMap::new();
        for name in names {
            registries.insert(name.clone(), Registry::load(&workspace, &name)?);
        }

        let context = Context {
            workspace,
            catalog,
            registries,
            numbering,
            workflows,
            identity: None,
        };
        for kind in context.catalog.kinds() {
            for field in &kind.fields {
                if let Some(value) = &field.default {
                    context.check_field(field, value).map_err(|error| {
                        Error::usage(format!(
                            "{}: default for `{}`: {error}",
                            kind.source, field.name
                        ))
                    })?;
                }
            }
        }
        validate_command_defaults(&context)?;
        crate::collaboration::workflow::validate_templates(&context)
            .map_err(|error| Error::usage(format!("workflow declaration: {error}")))?;
        for kind in context.catalog.kinds() {
            if let Some(logs) = kind.context.as_ref().and_then(|spec| spec.logs.as_ref())
                && context.catalog.require(logs)?.document_dir().is_none()
            {
                return Err(Error::usage(format!(
                    "context logs `{logs}` must store documents"
                )));
            }
        }
        for kind in context.catalog.kinds() {
            for field in &kind.fields {
                if let Some(crate::terminal::style::Spec::Values(styles)) = &field.terminal_style
                    && let Some(registry) = &field.registry
                {
                    for key in styles.keys().filter(|key| key.as_str() != "*") {
                        if !context
                            .registry(registry)?
                            .entries()
                            .iter()
                            .any(|entry| entry.key == *key)
                        {
                            return Err(Error::usage(format!(
                                "{}.terminal_style references unknown value `{key}`",
                                field.name
                            )));
                        }
                    }
                }
            }
        }
        Ok(context)
    }

    /// Defaults for one command, generic table first then the verb table.
    pub fn command_defaults(&self, name: &str, verb: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .workspace
            .terminal
            .iter()
            // Workspace reader defaults are inherited only by commands that
            // expose those controls. Explicit command defaults remain validated.
            .filter(|(option, _)| {
                crate::interface::command::applies(
                    option,
                    name,
                    verb,
                    self.catalog.find(name).is_some(),
                )
            })
            .filter_map(|(name, value)| {
                value
                    .as_str()
                    .map(|value| (name.clone(), value.to_string()))
            })
            .collect();
        let mut keys = vec![name.to_string()];
        if !verb.is_empty() {
            keys.push(format!("{name}.{verb}"));
        }
        for key in keys {
            let Some(table) = self
                .workspace
                .commands
                .get(&key)
                .and_then(|value| value.as_table())
            else {
                continue;
            };
            for (name, value) in table {
                let entry = crate::interface::command::default_value(name, value)
                    .map(|value| (name.clone(), value));
                if let Some((name, value)) = entry {
                    if let Some(existing) = out.iter_mut().find(|(key, _)| key == &name) {
                        existing.1 = value;
                    } else {
                        out.push((name, value));
                    }
                }
            }
        }
        out
    }

    pub fn registry(&self, name: &str) -> Result<&Registry> {
        self.registries
            .get(name)
            .ok_or_else(|| Error::usage(format!("registry `{name}` is not loaded")))
    }

    /// Field constraints shared by writes, command defaults and workspace checks.
    pub fn check_field(&self, field: &crate::config::kind::Field, value: &str) -> Result<()> {
        let registry = field
            .registry
            .as_ref()
            .map(|name| self.registry(name))
            .transpose()?;
        crate::config::kind::check_value(field, value, registry)?;
        if value.trim().is_empty() {
            return Ok(());
        }
        if let Some(name) = &field.target {
            let target = self.catalog.require(name)?;
            let key = &target.key[0];
            let records = crate::storage::records::including_archived(self, target)?;
            for item in if field.multi {
                value.split('|').collect::<Vec<_>>()
            } else {
                vec![value]
            } {
                let item = item.trim();
                if !item.is_empty()
                    && !records
                        .iter()
                        .any(|record| record.get(key).is_some_and(|v| v == item))
                {
                    return Err(Error::validation(format!(
                        "field `{}`: `{item}` does not reference a live or archived `{name}` key",
                        field.name
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn numbering(&self, name: &str) -> Result<&Scheme> {
        self.numbering
            .iter()
            .find(|scheme| scheme.name == name)
            .ok_or_else(|| Error::usage(format!("numbering scheme `{name}` is not declared")))
    }

    /// Load the identity once and reuse it.
    pub fn identity(&mut self) -> Result<&Identity> {
        if self.identity.is_none() {
            self.identity = Some(Identity::resolve(&self.workspace, None)?);
        }
        Ok(self.identity.as_ref().expect("identity set"))
    }

    /// Registries in the shape numbering expects.
    pub fn numbering_registries(&self) -> Result<Registries> {
        let mut loaded = Registries::default();
        for name in &self.registries.keys().cloned().collect::<Vec<_>>() {
            loaded = loaded.with(name.clone(), self.registries[name].clone());
        }
        Ok(loaded)
    }
}

/// Check every `[commands."..."]` entry against the declared kinds and options.
fn validate_command_defaults(loaded: &Context) -> Result<()> {
    let workspace = &loaded.workspace;
    let catalog = &loaded.catalog;
    for (key, value) in &workspace.commands {
        let context = format!("{}: commands.`{key}`", crate::config::workspace::MARKER);
        let Some(table) = value.as_table() else {
            return Err(Error::usage(format!(
                "{context}: expected a table of defaults"
            )));
        };
        let (command, verb) = match key.split_once('.') {
            Some((command, verb)) => (command, Some(verb)),
            None => (key.as_str(), None),
        };
        let kind = catalog.find(command);
        if kind.is_none() && crate::interface::command::builtin(command).is_none() {
            return Err(Error::usage(format!(
                "{context}: no command `{command}`; declare the kind or use a built in name"
            )));
        }
        if let (Some(kind), Some(verb)) = (kind, verb)
            && !crate::interface::command::verb_names(kind)
                .iter()
                .any(|name| name == verb)
        {
            return Err(Error::usage(format!(
                "{context}: kind `{}` has no verb `{verb}`",
                kind.name
            )));
        }
        if let (Some(builtin), Some(verb)) = (crate::interface::command::builtin(command), verb)
            && !builtin.subcommands.contains(&verb)
        {
            return Err(Error::usage(format!(
                "{context}: command `{command}` has no subcommand `{verb}`"
            )));
        }
        for (name, value) in table {
            validate_default(loaded, kind, command, name, value, &context)?;
        }
    }
    Ok(())
}

fn validate_default(
    loaded: &Context,
    kind: Option<&crate::config::kind::Kind>,
    command: &str,
    name: &str,
    value: &crate::config::decl::Value,
    location: &str,
) -> Result<()> {
    use crate::config::decl::Value;
    let invalid = |reason: String| Error::usage(format!("{location}.{name}: {reason}"));
    let spec = crate::interface::command::option(name);
    let field = kind
        .and_then(|kind| kind.field(name))
        .filter(|_| !crate::interface::command::reserved(name));
    let scheme = kind
        .and_then(|kind| kind.numbering.as_ref())
        .map(|name| loaded.numbering(name))
        .transpose()?;
    let token_registry = scheme.and_then(|scheme| scheme.tokens.get(name));
    if let Some(field) = field {
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Boolean(value) if field.kind == crate::config::kind::FieldKind::Boolean => {
                value.to_string()
            }
            Value::Integer(value) if field.kind == crate::config::kind::FieldKind::Integer => {
                value.to_string()
            }
            _ => {
                return Err(invalid(format!(
                    "expected a {} field value",
                    field.kind.name()
                )));
            }
        };
        return loaded
            .check_field(field, &text)
            .map_err(|error| invalid(error.to_string()));
    }
    if let Some(registry) = token_registry {
        let text = value
            .as_str()
            .ok_or_else(|| invalid("numbering tokens must be strings".into()))?;
        if !loaded.registry(registry)?.has(text) {
            return Err(invalid(format!(
                "`{text}` is not registered in `{registry}`"
            )));
        }
        return Ok(());
    }
    let allowed = match kind {
        Some(kind) => crate::interface::command::kind_option(kind, name),
        None => crate::interface::command::builtin(command)
            .is_some_and(|builtin| crate::interface::command::builtin_option(builtin, name)),
    };
    let spec = spec.ok_or_else(|| invalid(format!("unknown option or field `{name}`")))?;
    if !allowed {
        return Err(invalid("unknown option or field for this command".into()));
    }
    if !spec.default {
        return Err(invalid(format!("`{name}` may not be a default")));
    }
    if spec.switch() {
        return if value.is_bool() {
            Ok(())
        } else {
            Err(invalid("switch defaults must be boolean".into()))
        };
    }
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Integer(number)
            if matches!(
                spec.value,
                Some(
                    crate::interface::command::ValueKind::Width
                        | crate::interface::command::ValueKind::TerminalWidth
                )
            ) =>
        {
            number.to_string()
        }
        Value::Array(items)
            if spec.value == Some(crate::interface::command::ValueKind::List)
                && !items.is_empty()
                && items.iter().all(|item| item.is_str()) =>
        {
            items
                .iter()
                .map(|item| item.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(",")
        }
        _ => {
            return Err(invalid(
                "expected a string, non-negative width, or non-empty column/sort string array"
                    .into(),
            ));
        }
    };
    spec.validate(&text)
        .map_err(|error| invalid(error.to_string()))
}
fn load_numbering(workspace: &Workspace) -> Result<Vec<Scheme>> {
    let mut schemes = Vec::new();
    for (name, table) in workspace.numbering()? {
        schemes.push(Scheme::parse(&name, &table)?);
    }
    Ok(schemes)
}
