//! Verbs: the operations every kind supports.
//!
//! Each verb loads the kind, checks the payload against the declaration and
//! writes through the same path, so every entry point shares one set of rules.
//! The report names each affected file so that sealing and committing can be
//! driven from it.

use std::collections::BTreeMap;

use crate::config::identity::Identity;
use crate::config::kind::{self, Auto, Field, FieldKind, Kind, Role};
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::foundation::time;
use crate::storage::store::{Record, Store, Table};

/// A value supplied on the command line.
pub type Values = BTreeMap<String, String>;

/// Result of one verb.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub kind: String,
    pub verb: String,
    pub columns: Vec<String>,
    pub records: Vec<Record>,
    pub affected: Vec<String>,
    pub notes: Vec<String>,
    /// Paths that no longer exist after the verb, whose seals must be dropped.
    pub removed: Vec<String>,
    /// Findings that do not stop the verb but must be reported.
    pub problems: Vec<String>,
    pub dry_run: bool,
    pub scope: Option<String>,
    pub archived_keys: std::collections::BTreeSet<String>,
}

impl Outcome {
    pub fn new(kind: &Kind, verb: &str) -> Self {
        Outcome {
            kind: kind.name.clone(),
            verb: verb.to_string(),
            columns: kind.list_columns(),
            records: Vec::new(),
            affected: Vec::new(),
            notes: Vec::new(),
            removed: Vec::new(),
            problems: Vec::new(),
            dry_run: false,
            scope: None,
            archived_keys: Default::default(),
        }
    }
}

/// Options that change how a verb behaves.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub dry_run: bool,
    pub tokens: BTreeMap<String, String>,
    pub filters: Vec<(String, String)>,
    /// Condition expression from `--filter`.
    pub filter: Option<String>,
    pub case: Option<String>,
    /// Body text supplied for a document instance.
    pub body: Option<String>,
    /// Read from the archive instead of the live data.
    pub scope: String,
    /// Show display text instead of keys in the printed table.
    pub display: bool,
    /// Allow a destructive verb.
    pub force: bool,
    /// Print one detail block per record.
    pub long: bool,
    /// Only instances this identity claims.
    pub mine: bool,
    /// Only instances that have a claimant.
    pub claimed: bool,
}

/// List instances, optionally filtered.
pub fn list(context: &mut Context, kind_name: &str, options: &Options) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?.clone();
    crate::query::scope::read(context, &kind, "list", &[], options)
}

/// Show one instance by key.
pub fn show(context: &Context, kind_name: &str, keys: &[String]) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?;
    let store = Store::open(&context.workspace, kind)?;
    let index = store.find(keys).ok_or_else(|| missing(kind, keys))?;
    let record = store.table.record(index);
    let mut outcome = Outcome::new(kind, "show");
    outcome.columns = kind.fields.iter().map(|field| field.name.clone()).collect();
    outcome.records.push(record);
    Ok(outcome)
}

/// Create one instance.
pub fn create(
    context: &mut Context,
    kind_name: &str,
    values: &Values,
    options: &Options,
) -> Result<Outcome> {
    create_with(context, kind_name, values, options, &[])
}

/// Create one instance, with keys already taken by an earlier row.
pub fn create_with(
    context: &mut Context,
    kind_name: &str,
    values: &Values,
    options: &Options,
    taken: &[String],
) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?.clone();
    let identity = if needs_identity(&kind) {
        context.identity()?.clone()
    } else {
        Identity::default()
    };
    let mut store = Store::open(&context.workspace, &kind)?;
    let mut outcome = Outcome::new(&kind, "new");
    let offset = context.workspace.policy.offset_hours;

    let mut record = Record::new();
    for field in &kind.fields {
        record.insert(
            field.name.clone(),
            field.default.clone().unwrap_or_default(),
        );
    }
    if let Some(state) = &kind.state
        && let Some(default) = &state.default
    {
        record.insert(state.field.clone(), default.clone());
    }
    apply_identity(&kind, &mut record, &identity, offset);
    apply_provided(context, &kind, &mut record, values, true)?;
    apply_time(&kind, &mut record, offset, true);
    with_hash(context, &kind, &mut record)?;

    if let Some(scheme_name) = &kind.numbering {
        let scheme = context.numbering(scheme_name)?;
        let mut existing = store.keys();
        existing.extend(crate::storage::issued::keys(context, &kind)?);
        existing.extend(taken.iter().cloned());
        let registries = numbering_registries(context)?;
        let mut tokens = options.tokens.clone();
        let key = scheme.allocate_with_defaults(&existing, &mut tokens, &registries)?;
        if let Some(first) = kind.key.first() {
            record.insert(first.clone(), key);
        }
    }

    validate_record(context, &kind, &record)?;

    let key_values: Vec<String> = kind
        .key
        .iter()
        .map(|key| record.get(key).cloned().unwrap_or_default())
        .collect();
    if store.find(&key_values).is_some() {
        return Err(Error::conflict(format!(
            "`{}` is already in the live table",
            key_values.join(" ")
        )));
    }

    outcome.records.push(record.clone());
    outcome.affected.push(store.relative());
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    store.table.push(&record);
    let keys: Vec<String> = kind
        .key
        .first()
        .and_then(|key| record.get(key))
        .cloned()
        .into_iter()
        .collect();
    if let Some(path) = crate::storage::issued::remember(context, &kind, &keys)? {
        outcome.affected.push(path);
    }
    store.save()?;
    Ok(outcome)
}

/// Update fields of one instance.
pub fn set(
    context: &Context,
    kind_name: &str,
    keys: &[String],
    values: &Values,
    options: &Options,
) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?;
    let mut store = Store::open(&context.workspace, kind)?;
    let index = store.find(keys).ok_or_else(|| missing(kind, keys))?;
    let mut record = store.table.record(index);
    let before = record.clone();

    if let Some(state) = &kind.state
        && values.contains_key(&state.field)
    {
        return Err(Error::usage(format!(
            "field `{}` is driven by a transition; run one of: {}",
            state.field,
            transition_names(kind)
        )));
    }
    apply_provided(context, kind, &mut record, values, false)?;
    apply_time(
        kind,
        &mut record,
        context.workspace.policy.offset_hours,
        false,
    );
    with_hash(context, kind, &mut record)?;
    kind::validate_change(kind, &before, &record)?;
    validate_record(context, kind, &record)?;

    let mut outcome = Outcome::new(kind, "set");
    outcome.records.push(record.clone());
    outcome.affected.push(store.relative());
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    store.table.apply(index, &record);
    store.save()?;
    Ok(outcome)
}

/// Run a declared transition.
pub fn transition(
    context: &Context,
    kind_name: &str,
    transition_name: &str,
    keys: &[String],
    values: &Values,
    options: &Options,
) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?;
    let state = kind
        .state
        .as_ref()
        .ok_or_else(|| Error::usage(format!("kind `{}` declares no state machine", kind.name)))?;
    let transition = kind.transition(transition_name).ok_or_else(|| {
        Error::usage(format!(
            "kind `{}` has no transition `{transition_name}`; available: {}",
            kind.name,
            transition_names(kind)
        ))
    })?;
    let mut store = Store::open(&context.workspace, kind)?;
    let index = store.find(keys).ok_or_else(|| missing(kind, keys))?;
    let mut record = store.table.record(index);
    let before = record.clone();

    let registry = context.registry(&state.registry)?;
    let current = record.get(&state.field).cloned().unwrap_or_default();
    let current_key = registry.key_of(&current).unwrap_or(current.clone());
    if !transition.from.iter().any(|item| item == &current_key) {
        return Err(Error::conflict(format!(
            "`{}` is in state `{current_key}`; `{transition_name}` allows {}",
            keys.join(" "),
            transition.from.join(", ")
        )));
    }

    apply_provided(context, kind, &mut record, values, false)?;
    for name in &transition.requires {
        if record
            .get(name)
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(Error::validation(format!(
                "transition `{transition_name}` requires `--{name}`"
            )));
        }
    }
    record.insert(state.field.clone(), transition.to.clone());
    apply_time(
        kind,
        &mut record,
        context.workspace.policy.offset_hours,
        false,
    );
    kind::validate_change(kind, &before, &record)?;
    validate_record(context, kind, &record)?;

    let mut outcome = Outcome::new(kind, transition_name);
    outcome.records.push(record.clone());
    outcome.affected.push(store.relative());
    if transition.archive {
        outcome.notes.push(format!(
            "state `{}` allows archiving into {}",
            transition.to, kind.policy.archive_dir
        ));
    }
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    store.table.apply(index, &record);
    store.save()?;
    Ok(outcome)
}

/// Add a relation between two instances.
pub fn link(
    context: &Context,
    kind_name: &str,
    from_keys: &[String],
    relation_name: &str,
    to_kind_name: &str,
    to_keys: &[String],
) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?;
    let relation = kind
        .relations
        .iter()
        .find(|item| item.name == relation_name)
        .ok_or_else(|| {
            Error::usage(format!(
                "kind `{}` declares no relation `{relation_name}`; declared: {}",
                kind.name,
                relation_names(kind)
            ))
        })?;
    if relation.target != to_kind_name {
        return Err(Error::usage(format!(
            "relation `{relation_name}` points at kind `{}`, not `{to_kind_name}`",
            relation.target
        )));
    }
    let path = relations_path(context, kind)?;
    if !crate::storage::records::contains(context, kind, from_keys)? {
        return Err(missing(kind, from_keys));
    }
    let target_kind = context.catalog.require(to_kind_name)?;
    if !crate::storage::records::contains(context, target_kind, to_keys)? {
        return Err(missing(target_kind, to_keys));
    }

    let mut table = load_relations(&path);
    let from_key = from_keys.join(" ");
    let to_key = to_keys.join(" ");
    let duplicate = table.rows.iter().any(|row| {
        row.first().map(String::as_str) == Some(from_key.as_str())
            && row.get(1).map(String::as_str) == Some(relation_name)
            && row.get(2).map(String::as_str) == Some(to_key.as_str())
    });
    if duplicate {
        return Err(Error::conflict(format!(
            "`{relation_name}` already links {from_key} to {to_key}"
        )));
    }
    table.rows.push(vec![
        from_key.clone(),
        relation_name.to_string(),
        to_key.clone(),
    ]);
    save_relations(&path, &table)?;

    let mut outcome = Outcome::new(kind, "link");
    outcome
        .notes
        .push(format!("{from_key} {relation_name} {to_key}"));
    outcome.affected.push(context.workspace.relative(&path));
    Ok(outcome)
}

/// Remove a relation between two instances.
pub fn unlink(
    context: &Context,
    kind_name: &str,
    from_keys: &[String],
    relation_name: &str,
    to_kind_name: &str,
    to_keys: &[String],
) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?;
    let relation = kind
        .relations
        .iter()
        .find(|item| item.name == relation_name)
        .ok_or_else(|| {
            Error::usage(format!(
                "kind `{}` declares no relation `{relation_name}`; declared: {}",
                kind.name,
                relation_names(kind)
            ))
        })?;
    if relation.target != to_kind_name {
        return Err(Error::usage(format!(
            "relation `{relation_name}` points at kind `{}`, not `{to_kind_name}`",
            relation.target
        )));
    }
    let path = relations_path(context, kind)?;
    let mut table = load_relations(&path);
    let from_key = from_keys.join(" ");
    let to_key = to_keys.join(" ");
    let before = table.rows.len();
    table.rows.retain(|row| {
        !(row.first().map(String::as_str) == Some(from_key.as_str())
            && row.get(1).map(String::as_str) == Some(relation_name)
            && row.get(2).map(String::as_str) == Some(to_key.as_str()))
    });
    if table.rows.len() == before {
        return Err(Error::usage(format!(
            "no `{relation_name}` link from {from_key} to {to_key}"
        )));
    }
    save_relations(&path, &table)?;

    let mut outcome = Outcome::new(kind, "unlink");
    outcome
        .notes
        .push(format!("removed {from_key} {relation_name} {to_key}"));
    outcome.affected.push(context.workspace.relative(&path));
    Ok(outcome)
}

fn relations_path(context: &Context, kind: &Kind) -> Result<std::path::PathBuf> {
    kind.relations_table
        .clone()
        .map(|path| context.workspace.resolve(&path))
        .ok_or_else(|| {
            Error::usage(format!(
                "kind `{}` declares relations but no relations_table",
                kind.name
            ))
        })
}

fn relation_headers() -> Vec<String> {
    vec!["from".to_string(), "kind".to_string(), "to".to_string()]
}

fn load_relations(path: &std::path::Path) -> Table {
    if path.exists()
        && let Ok(mut table) = Table::load(path)
    {
        if table.headers.is_empty() {
            table.headers = relation_headers();
        }
        return table;
    }
    Table {
        headers: relation_headers(),
        rows: Vec::new(),
    }
}

fn save_relations(path: &std::path::Path, table: &Table) -> Result<()> {
    if let Some(parent) = path.parent() {
        crate::execution::transaction::create_dir_all(parent)?;
    }
    let mut rows = Vec::with_capacity(table.rows.len() + 1);
    rows.push(table.headers.clone());
    rows.extend(table.rows.iter().cloned());
    crate::execution::transaction::write(path, crate::foundation::csv::serialize(&rows))?;
    Ok(())
}

fn missing(kind: &Kind, keys: &[String]) -> Error {
    Error::usage(format!("no `{}` with key {}", kind.name, keys.join(" ")))
}

fn transition_names(kind: &Kind) -> String {
    let names: Vec<&str> = kind
        .transitions
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

fn relation_names(kind: &Kind) -> String {
    let names: Vec<&str> = kind
        .relations
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

pub(crate) fn apply_provided(
    context: &Context,
    kind: &Kind,
    record: &mut Record,
    values: &Values,
    creating: bool,
) -> Result<()> {
    validate_fields(kind, values, creating)?;
    for (name, value) in values {
        let field = kind.field(name).expect("validated field");
        let value = normalize(context, field, value)?;
        context.check_field(field, &value)?;
        record.insert(name.clone(), value);
    }
    Ok(())
}

/// Field permissions without record lookup or value substitution.
pub(crate) fn validate_fields(kind: &Kind, values: &Values, creating: bool) -> Result<()> {
    for name in values.keys() {
        let field = kind.field(name).ok_or_else(|| {
            Error::usage(format!(
                "kind `{}` has no field `{name}`; fields: {}",
                kind.name,
                kind.fields
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        if field.role == Role::Status {
            return Err(Error::usage(format!(
                "field `{name}` is maintained by the tool and cannot be set"
            )));
        }
        if !creating
            && (field.create_only
                || kind
                    .document_filename()
                    .is_some_and(|pattern| pattern.contains(&format!("{{{name}}}"))))
        {
            return Err(Error::usage(format!(
                "field `{name}` can only be set when creating an instance"
            )));
        }
    }
    Ok(())
}

pub(crate) fn normalize(context: &Context, field: &Field, value: &str) -> Result<String> {
    if field.kind != FieldKind::Enum {
        return Ok(value.trim().to_string());
    }
    let registry_name = field
        .registry
        .as_ref()
        .ok_or_else(|| Error::usage(format!("enum field `{}` has no registry", field.name)))?;
    let registry = context.registry(registry_name)?;
    if field.multi {
        let mut out: Vec<String> = Vec::new();
        for item in value.split('|') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let key = registry.key_of(item).ok_or_else(|| {
                Error::validation(format!(
                    "`{item}` is not registered in `{registry_name}`; known values: {}",
                    registry.keys().join(", ")
                ))
            })?;
            if !out.contains(&key) {
                out.push(key);
            }
        }
        return Ok(out.join("|"));
    }
    registry.key_of(value).ok_or_else(|| {
        Error::validation(format!(
            "`{value}` is not registered in `{registry_name}`; known values: {}",
            registry.keys().join(", ")
        ))
    })
}

pub(crate) fn needs_identity(kind: &Kind) -> bool {
    kind.fields.iter().any(|field| {
        matches!(
            field.auto,
            Some(Auto::GitName) | Some(Auto::GitEmail) | Some(Auto::Person) | Some(Auto::Operator)
        )
    })
}

pub(crate) fn apply_identity(
    kind: &Kind,
    record: &mut Record,
    identity: &Identity,
    offset_hours: i32,
) {
    for field in &kind.fields {
        let Some(auto) = field.auto else { continue };
        if matches!(
            auto,
            Auto::GitName | Auto::GitEmail | Auto::Person | Auto::Operator
        ) {
            record.insert(
                field.name.clone(),
                kind::auto_value(auto, identity, offset_hours),
            );
        }
    }
}

pub(crate) fn apply_time(kind: &Kind, record: &mut Record, offset_hours: i32, creating: bool) {
    let epoch = time::now_epoch();
    for field in &kind.fields {
        if field.immutable && !creating {
            continue;
        }
        if kind
            .document_filename()
            .is_some_and(|pattern| pattern.contains(&format!("{{{}}}", field.name)))
            && record
                .get(&field.name)
                .is_some_and(|value| !value.is_empty())
        {
            continue;
        }
        let value = match field.auto {
            Some(Auto::Clock) => time::stamp(epoch, offset_hours)[9..].to_string(),
            Some(Auto::Today) => time::date(epoch, offset_hours),
            Some(Auto::Now) => time::stamp(epoch, offset_hours),
            _ => continue,
        };
        record.insert(field.name.clone(), value);
    }
}

pub fn validate_record(context: &Context, kind: &Kind, record: &Record) -> Result<()> {
    for name in &kind.key {
        let value = record.get(name).map(|item| item.trim()).unwrap_or("");
        if value.is_empty() {
            return Err(Error::validation(format!("key field `{name}` is empty")));
        }
    }
    for field in &kind.fields {
        let value = record.get(&field.name).cloned().unwrap_or_default();
        let trimmed = value.trim();
        let needed = field.required
            || field
                .required_when
                .as_ref()
                .map(|condition| condition.holds(record))
                .unwrap_or(false);
        if needed && trimmed.is_empty() {
            let hint = match &field.required_when {
                Some(condition) => format!(" when {} == {}", condition.field, condition.value),
                None => String::new(),
            };
            return Err(Error::validation(format!(
                "field `{}` is required{hint}",
                field.name
            )));
        }
        if trimmed.is_empty() {
            continue;
        }
        context
            .check_field(field, trimmed)
            .map_err(|error| Error::validation(format!("field `{}`: {error}", field.name)))?;
    }
    if let Some(state) = &kind.state {
        let registry = context.registry(&state.registry)?;
        let value = record.get(&state.field).cloned().unwrap_or_default();
        if !value.trim().is_empty() && registry.key_of(&value).is_none() {
            return Err(Error::validation(format!(
                "state `{value}` is not registered in `{}`",
                state.registry
            )));
        }
    }
    if let Some(scheme_name) = &kind.numbering
        && let Some(first) = kind.key.first()
        && let Some(value) = record.get(first)
        && !value.trim().is_empty()
    {
        let scheme = context.numbering(scheme_name)?;
        let registries = numbering_registries(context)?;
        if !scheme.matches(value, &registries)? {
            return Err(Error::validation(format!(
                "`{value}` does not match numbering scheme `{scheme_name}`"
            )));
        }
    }
    Ok(())
}

/// Build the registry set numbering needs.
pub fn numbering_registries(context: &Context) -> Result<crate::storage::numbering::Registries> {
    let mut names: Vec<String> = Vec::new();
    for scheme in &context.numbering {
        for name in scheme.tokens.values() {
            names.push(name.clone());
        }
    }
    names.sort();
    names.dedup();
    let mut loaded = crate::storage::numbering::Registries::default();
    for name in names {
        if let Some(registry) = context.registries.get(&name) {
            loaded = loaded.with(name, registry.clone());
        }
    }
    Ok(loaded)
}

/// Whether a state allows archiving.
pub fn archivable(kind: &Kind, state_key: &str) -> bool {
    kind.transitions
        .iter()
        .any(|item| item.archive && item.to == state_key)
}

pub fn state_key_of(context: &Context, kind: &Kind, record: &Record) -> Result<String> {
    let Some(state) = &kind.state else {
        return Ok(String::new());
    };
    let registry = context.registry(&state.registry)?;
    let value = record.get(&state.field).cloned().unwrap_or_default();
    Ok(registry.key_of(&value).unwrap_or(value))
}

fn write_table(path: &std::path::Path, table: &Table) -> Result<()> {
    if let Some(parent) = path.parent() {
        crate::execution::transaction::create_dir_all(parent)?;
    }
    let mut rows = Vec::with_capacity(table.rows.len() + 1);
    rows.push(table.headers.clone());
    rows.extend(table.rows.iter().cloned());
    crate::execution::transaction::write(path, crate::foundation::csv::serialize(&rows))?;
    Ok(())
}

/// Move one instance into the archive table.
pub fn archive(
    context: &Context,
    kind_name: &str,
    keys: &[String],
    options: &Options,
) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?;
    let mut store = Store::open(&context.workspace, kind)?;
    let index = store.find(keys).ok_or_else(|| missing(kind, keys))?;
    let record = store.table.record(index);
    let state_key = state_key_of(context, kind, &record)?;
    if !archivable(kind, &state_key) {
        return Err(Error::conflict(format!(
            "`{}` is in state `{state_key}`, which does not allow archiving",
            keys.join(" ")
        )));
    }
    let path = context.workspace.resolve(&kind.archive_table());
    let mut table = if path.exists() {
        Table::load(&path)?
    } else {
        Table {
            headers: store.table.headers.clone(),
            rows: Vec::new(),
        }
    };
    let headers = store.table.headers.clone();
    table.ensure_columns(&headers);
    if table.find(&kind.key, keys).is_some() {
        return Err(Error::conflict(format!(
            "`{}` is already archived",
            keys.join(" ")
        )));
    }
    table.push(&record);

    let mut outcome = Outcome::new(kind, "archive");
    outcome.columns = kind.fields.iter().map(|field| field.name.clone()).collect();
    outcome.records.push(record);
    outcome.affected.push(store.relative());
    outcome.affected.push(context.workspace.relative(&path));
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    write_table(&path, &table)?;
    if let Some(path) = crate::storage::issued::remember(context, kind, &[])? {
        outcome.affected.push(path);
    }
    store.table.remove(index);
    store.save()?;
    Ok(outcome)
}

/// Move one instance back from the archive table.
pub fn restore(
    context: &Context,
    kind_name: &str,
    keys: &[String],
    options: &Options,
) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?;
    let path = context.workspace.resolve(&kind.archive_table());
    if !path.exists() {
        return Err(Error::usage(format!(
            "no archive table at {}",
            context.workspace.relative(&path)
        )));
    }
    let mut table = Table::load(&path)?;
    let index = table
        .find(&kind.key, keys)
        .ok_or_else(|| missing(kind, keys))?;
    let record = table.record(index);
    let mut store = Store::open(&context.workspace, kind)?;
    if store.find(keys).is_some() {
        return Err(Error::conflict(format!(
            "`{}` is already in the live table",
            keys.join(" ")
        )));
    }

    let mut outcome = Outcome::new(kind, "restore");
    outcome.columns = kind.fields.iter().map(|field| field.name.clone()).collect();
    outcome.records.push(record);
    outcome.affected.push(store.relative());
    outcome.affected.push(context.workspace.relative(&path));
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    let source = table.record(index);
    let headers = store.table.headers.clone();
    let mut restored = Record::new();
    for header in &headers {
        restored.insert(
            header.clone(),
            source.get(header).cloned().unwrap_or_default(),
        );
    }
    store.table.push(&restored);
    store.save()?;
    table.remove(index);
    write_table(&path, &table)?;
    Ok(outcome)
}

/// Fill the digest field from the current content of the file field.
fn with_hash(context: &Context, kind: &Kind, record: &mut Record) -> Result<()> {
    let Some(spec) = &kind.hash else {
        return Ok(());
    };
    let path = record.get(&spec.file).cloned().unwrap_or_default();
    let path = path.trim();
    if path.is_empty() {
        return Ok(());
    }
    let digest = crate::storage::seal::hash_key(&context.workspace, path)?;
    record.insert(spec.digest.clone(), digest);
    Ok(())
}

/// Compare every registered file with its recorded digest.
pub fn verify(context: &Context, kind_name: &str, options: &Options) -> Result<Outcome> {
    let kind = context.catalog.require(kind_name)?;
    let spec = kind.hash.clone().ok_or_else(|| {
        Error::usage(format!(
            "kind `{}` declares no [hash] section, so there is nothing to verify",
            kind.name
        ))
    })?;
    let records = if kind.document_dir().is_some() {
        crate::storage::document::records(context, kind)?
    } else {
        let store = Store::open(&context.workspace, kind)?;
        (0..store.table.rows.len())
            .map(|index| store.table.record(index))
            .collect()
    };

    let mut outcome = Outcome::new(kind, "verify");
    outcome.columns = vec!["key".to_string(), spec.file.clone(), "result".to_string()];
    for record in records {
        let key = kind
            .key
            .iter()
            .map(|name| record.get(name).cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ");
        let path = record.get(&spec.file).cloned().unwrap_or_default();
        let expected = record.get(&spec.digest).cloned().unwrap_or_default();
        let mut row = Record::new();
        row.insert("key".to_string(), key.clone());
        row.insert(spec.file.clone(), path.clone());
        match crate::storage::seal::hash_key(&context.workspace, path.trim()) {
            Ok(actual) if actual == expected => {
                row.insert("result".to_string(), "ok".to_string());
            }
            Ok(_) => {
                row.insert("result".to_string(), "mismatch".to_string());
                outcome
                    .problems
                    .push(format!("{key}: {} does not match its digest", path.trim()));
            }
            Err(error) => {
                row.insert("result".to_string(), "missing".to_string());
                outcome.problems.push(format!("{key}: {error}"));
            }
        }
        outcome.records.push(row);
    }
    let _ = options;
    Ok(outcome)
}

/// Run a declared action on one instance.
pub fn action(
    context: &mut Context,
    kind_name: &str,
    action_name: &str,
    keys: &[String],
    options: &Options,
) -> Result<Outcome> {
    let identity = context.identity()?.clone();
    let kind = context.catalog.require(kind_name)?.clone();
    let spec = kind.action(action_name).cloned().ok_or_else(|| {
        Error::usage(format!(
            "kind `{}` declares no action `{action_name}`",
            kind.name
        ))
    })?;
    if kind.document_dir().is_some() {
        match spec.handler.as_str() {
            "append" => return crate::storage::document::append(context, &kind, &spec, options),
            "prune" => return crate::storage::document::prune(context, &kind, options),
            _ => {}
        }
    }
    let mut store = Store::open(&context.workspace, &kind)?;
    let index = store.find(keys).ok_or_else(|| missing(&kind, keys))?;
    let mut record = store.table.record(index);
    let before = record.clone();

    if let Some(state) = &kind.state
        && !spec.states.is_empty()
    {
        let registry = context.registry(&state.registry)?;
        let current = record.get(&state.field).cloned().unwrap_or_default();
        let current_key = registry.key_of(&current).unwrap_or(current);
        if !spec.states.iter().any(|item| item == &current_key) {
            return Err(Error::conflict(format!(
                "`{}` is in state `{current_key}`; `{action_name}` allows {}",
                keys.join(" "),
                spec.states.join(", ")
            )));
        }
    }
    for name in &spec.require_empty {
        let value = record.get(name).cloned().unwrap_or_default();
        if !value.trim().is_empty() {
            return Err(Error::conflict(format!(
                "`{action_name}` needs `{name}` to be empty; it holds `{value}`"
            )));
        }
    }
    if spec.require_clean && !clean_tree(&context.workspace) {
        return Err(Error::conflict(format!(
            "`{action_name}` needs a clean worktree"
        )));
    }
    for name in &spec.requires {
        let value = record.get(name).cloned().unwrap_or_default();
        if value.trim().is_empty() {
            return Err(Error::conflict(format!(
                "`{action_name}` needs `{name}` to be filled first"
            )));
        }
    }

    let epoch = time::now_epoch();
    let offset = context.workspace.policy.offset_hours;
    if spec.handler == "attach" {
        let field = spec.field.clone().unwrap_or_default();
        let template = spec.filename.clone().unwrap_or_default();
        let relative = crate::execution::render::expand(&template, &record);
        let path = context.workspace.resolve(&relative);
        let text = options.body.clone().unwrap_or_default();
        if let Some(parent) = path.parent() {
            crate::execution::transaction::create_dir_all(parent)?;
        }
        record.insert(field, relative.clone());
        apply_time(&kind, &mut record, offset, false);
        validate_record(context, &kind, &record)?;
        kind::validate_change(&kind, &before, &record)?;
        crate::execution::transaction::write(&path, text)?;
        let mut outcome = Outcome::new(&kind, action_name);
        outcome.records.push(record.clone());
        outcome.affected.push(relative);
        outcome.affected.push(store.relative());
        if options.dry_run {
            outcome.dry_run = true;
            return Ok(outcome);
        }
        store.table.apply(index, &record);
        store.save()?;
        return Ok(outcome);
    }
    if spec.handler == "set" {
        for (field, template) in &spec.values {
            let value = action_value(&record, template, &identity, epoch, offset);
            record.insert(field.clone(), value);
        }
    }
    for field in &spec.fields {
        let value = match spec.handler.as_str() {
            "identity" => {
                if field.ends_with("_email") {
                    identity.git_email.clone()
                } else if field.ends_with("_at") {
                    time::stamp(epoch, offset)
                } else {
                    identity.display()
                }
            }
            _ => String::new(),
        };
        record.insert(field.clone(), value);
    }
    apply_time(&kind, &mut record, offset, false);
    kind::validate_change(&kind, &before, &record)?;
    validate_record(context, &kind, &record)?;

    let mut outcome = Outcome::new(&kind, action_name);
    outcome.records.push(record.clone());
    outcome.affected.push(store.relative());
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    store.table.apply(index, &record);
    store.save()?;
    Ok(outcome)
}

/// Expand a `set` template: identity and time words first, then record fields.
fn action_value(
    record: &Record,
    template: &str,
    identity: &Identity,
    epoch: u64,
    offset: i32,
) -> String {
    // The record expansion replaces every unknown placeholder with an empty
    // string, so the identity words have to be substituted before it runs.
    let text = template
        .replace("{today}", &time::date(epoch, offset))
        .replace("{now}", &time::stamp(epoch, offset))
        .replace("{identity}", &identity.display())
        .replace("{git_name}", &identity.git_name)
        .replace("{git_email}", &identity.git_email)
        .replace(
            "{operator}",
            &identity
                .operator
                .clone()
                .unwrap_or_else(|| identity.display()),
        );
    crate::execution::render::expand(&text, record)
}
/// Whether the workspace tree has no pending changes.
fn clean_tree(workspace: &crate::config::workspace::Workspace) -> bool {
    crate::git::commit::dirty_paths(workspace.root())
        .is_ok_and(|paths| paths.is_empty() || crate::execution::transaction::owns_changes(&paths))
}

/// Remove one instance permanently.
pub fn delete(
    context: &Context,
    kind_name: &str,
    keys: &[String],
    options: &Options,
) -> Result<Outcome> {
    if !options.force {
        return Err(Error::usage(format!(
            "`{kind_name} delete` removes the instance; pass --force to confirm"
        )));
    }
    let kind = context.catalog.require(kind_name)?;
    let mut store = Store::open(&context.workspace, kind)?;
    let index = store.find(keys).ok_or_else(|| missing(kind, keys))?;
    let record = store.table.record(index);
    let mut outcome = Outcome::new(kind, "delete");
    outcome.records.push(record);
    outcome.affected.push(store.relative());
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    store.table.remove(index);
    store.save()?;
    Ok(outcome)
}
