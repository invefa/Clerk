//! Document storage: one file per instance.
//!
//! Two metadata styles are supported. The block style keeps fields between
//! `<!-- clerk:meta` and `-->`; the bullets style keeps them as `- key: value`
//! lines right after the heading, which is what the existing documents use.
//! Fields may also come from the file name, so `01-name.md` and
//! `20261027-185600-reservation.md` need no metadata at all.

use std::path::{Path, PathBuf};

use crate::config::identity::Identity;
use crate::config::kind::{Kind, MetaStyle};
use crate::execution::context::Context;
use crate::execution::engine::{self, Options, Outcome, Values};
use crate::execution::render;
use crate::foundation::error::{Error, Result};
use crate::storage::store::Record;

const OPEN: &str = "<!-- clerk:meta";
const CLOSE: &str = "-->";

/// One document instance.
#[derive(Clone, Debug)]
pub struct Page {
    pub path: PathBuf,
    pub meta: Record,
    pub title: String,
    pub body: String,
}

impl Page {
    pub fn read(path: &Path, kind: &Kind) -> Result<Page> {
        Self::read_named(path, kind, "")
    }

    /// Read a page whose instance name may include directories below the kind
    /// directory, which is how a dated log page is addressed.
    pub fn read_named(path: &Path, kind: &Kind, relative_name: &str) -> Result<Page> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| crate::foundation::error::io_error(path, error))?;
        let (mut meta, title, body) = match kind.meta_style() {
            MetaStyle::Block => split_block(&text),
            MetaStyle::Bullets => split_bullets(&text),
        };
        remap_display(kind, &mut meta);
        if let Some(pattern) = kind.document_filename() {
            let name = if relative_name.is_empty() {
                path.file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                relative_name.to_string()
            };
            for (key, value) in parse_name(pattern, &name) {
                meta.entry(key).or_insert(value);
            }
        }
        Ok(Page {
            path: path.to_path_buf(),
            meta,
            title,
            body,
        })
    }
}

/// Map bullet keys written with display names back to field names.
fn remap_display(kind: &Kind, meta: &mut Record) {
    let mut out = Record::new();
    for (key, value) in meta.iter() {
        let name = kind
            .fields
            .iter()
            .find(|field| &field.display == key)
            .map(|field| field.name.clone())
            .unwrap_or_else(|| key.clone());
        out.insert(name, value.clone());
    }
    *meta = out;
}

/// Split a block style document into metadata, title and body.
pub fn split_block(text: &str) -> (Record, String, String) {
    let title = heading_of(text);
    let Some(open) = text.find(OPEN) else {
        return (Record::new(), title, text.to_string());
    };
    let Some(close) = text[open..].find(CLOSE).map(|offset| open + offset) else {
        return (Record::new(), title, text.to_string());
    };
    let block = &text[open + OPEN.len()..close];
    let mut meta = Record::new();
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            meta.insert(key.trim().to_string(), unbracket(value.trim()));
        }
    }
    let mut body = String::new();
    body.push_str(&text[..open]);
    body.push_str(&text[close + CLOSE.len()..]);
    (meta, title, strip_heading(&body))
}

/// Split a bullets style document into metadata, title and body.
pub fn split_bullets(text: &str) -> (Record, String, String) {
    let title = heading_of(text);
    let mut meta = Record::new();
    let mut rest = text;
    if let Some(index) = text.find('\n') {
        rest = &text[index + 1..];
    }
    let mut consumed = 0;
    let mut started = false;
    for line in rest.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            consumed += line.len() + 1;
            if started {
                break;
            }
            continue;
        }
        let Some(item) = trimmed.strip_prefix("- ") else {
            break;
        };
        started = true;
        if let Some((key, value)) = item.split_once(':') {
            meta.insert(key.trim().to_string(), unbracket(value.trim()));
        }
        consumed += line.len() + 1;
    }
    if consumed > rest.len() {
        consumed = rest.len();
    }
    let body = &rest[consumed..];
    (meta, title, body.trim_matches('\n').to_string())
}

fn unbracket(value: &str) -> String {
    let inner = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(value);
    // The document convention writes times as [T185600]; drop the marker so
    // that the stored value is the time itself.
    if let Some(rest) = inner.strip_prefix('T')
        && !rest.is_empty()
        && rest.chars().all(|character| character.is_ascii_digit())
    {
        return rest.to_string();
    }
    inner.to_string()
}

fn heading_of(text: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
}

/// Drop the leading heading, which is regenerated from the title.
fn strip_heading(body: &str) -> String {
    let trimmed = body.trim_start_matches('\n');
    if let Some(rest) = trimmed.strip_prefix("# ") {
        let rest = match rest.find('\n') {
            Some(index) => &rest[index + 1..],
            None => "",
        };
        return rest.trim_matches('\n').to_string();
    }
    trimmed.trim_matches('\n').to_string()
}

/// Fill fields from a file name pattern such as `{date}-{time}-{slug}.md`.
pub fn parse_name(pattern: &str, name: &str) -> Record {
    let mut out = Record::new();
    let parts = split_pattern(pattern);
    let mut rest = name;
    for (index, part) in parts.iter().enumerate() {
        match part {
            PatternPart::Literal(text) => {
                if let Some(tail) = rest.strip_prefix(text.as_str()) {
                    rest = tail;
                } else {
                    return Record::new();
                }
            }
            PatternPart::Slot(slot) => {
                let next = parts.get(index + 1);
                match next {
                    Some(PatternPart::Literal(text)) if !text.is_empty() => {
                        let Some(offset) = rest.find(text.as_str()) else {
                            return Record::new();
                        };
                        out.insert(slot.clone(), rest[..offset].to_string());
                        rest = &rest[offset..];
                    }
                    _ => {
                        out.insert(slot.clone(), rest.to_string());
                        rest = "";
                    }
                }
            }
        }
    }
    if rest.is_empty() { out } else { Record::new() }
}

enum PatternPart {
    Literal(String),
    Slot(String),
}

fn split_pattern(pattern: &str) -> Vec<PatternPart> {
    let mut parts = Vec::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        let literal = &rest[..open];
        if !literal.is_empty() {
            parts.push(PatternPart::Literal(literal.to_string()));
        }
        let Some(close) = rest[open..].find('}').map(|offset| open + offset) else {
            break;
        };
        parts.push(PatternPart::Slot(rest[open + 1..close].to_string()));
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        parts.push(PatternPart::Literal(rest.to_string()));
    }
    parts
}

/// Compose a document from metadata, title and body.
pub fn compose(kind: &Kind, meta: &Record, title: &str, body: &str) -> String {
    let title = if title.trim().is_empty() {
        meta.get("title")
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| kind.title.clone())
    } else {
        title.to_string()
    };
    let mut out = String::new();
    out.push_str(&format!("# {title}\n"));
    match kind.meta_style() {
        MetaStyle::Block => {
            out.push('\n');
            out.push_str(OPEN);
            out.push('\n');
            for field in &kind.fields {
                if let Some(value) = meta.get(&field.name)
                    && !value.trim().is_empty()
                {
                    out.push_str(&format!("{} = {}\n", field.name, value.trim()));
                }
            }
            out.push_str(CLOSE);
            out.push('\n');
        }
        MetaStyle::Bullets => {
            for field in &kind.fields {
                if field.name == "title" {
                    continue;
                }
                if let Some(value) = meta.get(&field.name)
                    && !value.trim().is_empty()
                {
                    out.push_str(&format!("- {}: {}\n", field.display, value.trim()));
                }
            }
            out.push('\n');
        }
    }
    let trimmed = body.trim_matches('\n');
    if !trimmed.is_empty() {
        out.push('\n');
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}

/// All documents of a kind.
pub(crate) fn pages(context: &Context, kind: &Kind) -> Result<Vec<Page>> {
    let dir = kind
        .document_dir()
        .ok_or_else(|| Error::usage(format!("kind `{}` is not document backed", kind.name)))?;
    pages_in(context, kind, dir)
}

/// All documents in one directory.
pub(crate) fn pages_in(context: &Context, kind: &Kind, dir: &str) -> Result<Vec<Page>> {
    let path = context.workspace.resolve(dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let index = index_path(context, kind);
    let archive = context.workspace.resolve(&kind.archive_dir());
    // The archive may sit below the live directory, as it does for log pages.
    let archive_below = archive != path && archive.starts_with(&path);
    let mut out = Vec::new();
    for file in crate::storage::records::markdown_files(&path)? {
        // The index page and the archive live below the same directory but are
        // not live instances.
        if index.as_deref() == Some(file.as_path()) || (archive_below && file.starts_with(&archive))
        {
            continue;
        }
        let relative = file
            .strip_prefix(&path)
            .map(|tail| tail.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if let Some(pattern) = kind.document_filename()
            && parse_name(pattern, &relative).is_empty()
        {
            continue;
        }
        out.push(Page::read_named(&file, kind, &relative)?);
    }
    out.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(out)
}

/// Identity of a page: the values of the declared key fields.
pub(crate) fn key_of(kind: &Kind, page: &Page) -> String {
    if kind.key.is_empty() {
        return String::new();
    }
    let values: Vec<String> = kind
        .key
        .iter()
        .map(|name| page.meta.get(name).cloned().unwrap_or_default())
        .collect();
    if values.iter().all(|value| value.trim().is_empty()) {
        return page
            .path
            .file_stem()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    values.join(" ")
}

fn find<'a>(kind: &Kind, pages: &'a [Page], keys: &[String]) -> Option<&'a Page> {
    let wanted = keys.join(" ");
    pages.iter().find(|page| key_of(kind, page) == wanted)
}

pub(crate) fn record_of(kind: &Kind, page: &Page) -> Record {
    let mut record = page.meta.clone();
    apply_key(kind, &mut record, page);
    if kind.field("title").is_some() {
        record
            .entry("title".to_string())
            .or_insert_with(|| page.title.clone());
    }
    record
}

/// Full metadata of every document, with the key filled in.
pub fn records(context: &Context, kind: &Kind) -> Result<Vec<Record>> {
    let dir = kind
        .document_dir()
        .ok_or_else(|| Error::usage(format!("kind `{}` is not document backed", kind.name)))?;
    records_in(context, kind, dir)
}

/// Full metadata of the documents in one directory.
pub(crate) fn records_in(context: &Context, kind: &Kind, dir: &str) -> Result<Vec<Record>> {
    let mut out = Vec::new();
    for page in pages_in(context, kind, dir)? {
        out.push(record_of(kind, &page));
    }
    Ok(out)
}

/// List documents.
pub fn list(context: &mut Context, kind: &Kind, options: &Options) -> Result<Outcome> {
    crate::query::scope::read(context, kind, "list", &[], options)
}

fn show_columns(kind: &Kind) -> Vec<String> {
    kind.fields.iter().map(|field| field.name.clone()).collect()
}

/// Show one document.
pub fn show(context: &Context, kind: &Kind, keys: &[String]) -> Result<Outcome> {
    let all = pages(context, kind)?;
    let page = find(kind, &all, keys).ok_or_else(|| missing(kind, keys))?;
    let mut outcome = Outcome::new(kind, "show");
    outcome.columns = show_columns(kind);
    outcome.records.push(record_of(kind, page));
    outcome
        .affected
        .push(context.workspace.relative(&page.path));
    Ok(outcome)
}

/// Create one document.
pub fn create(
    context: &mut Context,
    kind: &Kind,
    values: &Values,
    options: &Options,
) -> Result<Outcome> {
    let identity = if engine::needs_identity(kind) {
        context.identity()?.clone()
    } else {
        Identity::default()
    };
    let offset = context.workspace.policy.offset_hours;
    let all = pages(context, kind)?;

    let mut meta = Record::new();
    for field in &kind.fields {
        meta.insert(
            field.name.clone(),
            field.default.clone().unwrap_or_default(),
        );
    }
    if let Some(state) = &kind.state
        && let Some(default) = &state.default
    {
        meta.insert(state.field.clone(), default.clone());
    }
    engine::apply_identity(kind, &mut meta, &identity, offset);
    engine::apply_provided(context, kind, &mut meta, values, true)?;
    engine::apply_time(kind, &mut meta, offset, true);

    if let Some(scheme_name) = &kind.numbering {
        let scheme = context.numbering(scheme_name)?;
        let mut existing: Vec<String> = all.iter().map(|page| key_of(kind, page)).collect();
        existing.extend(crate::storage::issued::keys(context, kind)?);
        let registries = engine::numbering_registries(context)?;
        let mut tokens = options.tokens.clone();
        let key = scheme.allocate_with_defaults(&existing, &mut tokens, &registries)?;
        if let Some(first) = kind.key.first() {
            meta.insert(first.clone(), key);
        }
    }

    engine::validate_record(context, kind, &meta)?;
    let identity: Vec<_> = kind
        .key
        .iter()
        .map(|key| meta.get(key).cloned().unwrap_or_default())
        .collect();
    let mut existing: Vec<_> = all.iter().map(|page| record_of(kind, page)).collect();
    existing.extend(records_in(context, kind, &kind.archive_dir())?);
    if existing.iter().any(|record| {
        kind.key
            .iter()
            .map(|key| record.get(key).cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            == identity
    }) {
        return Err(Error::conflict(format!(
            "document key `{}` already exists; use a new identity or retry after the generated clock advances",
            identity.join(" ")
        )));
    }

    let dir = kind
        .document_dir()
        .ok_or_else(|| Error::usage(format!("kind `{}` is not document backed", kind.name)))?;
    let template = kind.document_filename().unwrap_or("{id}.md");
    for field in &kind.fields {
        if template.contains(&format!("{{{}}}", field.name)) {
            let value = meta.get(&field.name).map(String::as_str).unwrap_or("");
            if value.is_empty()
                || value.contains(['/', char::from(92), ':', '{', '}'])
                || value.contains("..")
            {
                return Err(Error::validation(format!(
                    "filename field `{}` must be a nonempty path component",
                    field.name
                )));
            }
        }
    }
    let filename = render::expand(template, &meta);
    let path = context.workspace.resolve(dir).join(&filename);
    if path.exists() {
        return Err(Error::conflict(format!(
            "{} already exists",
            context.workspace.relative(&path)
        )));
    }
    let title = meta.get("title").cloned().unwrap_or_default();
    let body = match &options.body {
        Some(text) => text.clone(),
        None => match &kind.template {
            Some(relative) => {
                let path = context.workspace.resolve(relative);
                if !path.exists() {
                    return Err(Error::usage(format!("template {relative} is missing")));
                }
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| crate::foundation::error::io_error(&path, error))?;
                render::expand(&text, &meta)
            }
            None => String::new(),
        },
    };
    let text = compose(kind, &meta, &title, &body);

    let mut outcome = Outcome::new(kind, "new");
    outcome.columns = show_columns(kind);
    outcome.records.push(meta.clone());
    outcome.affected.push(context.workspace.relative(&path));
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    if let Some(parent) = path.parent() {
        crate::execution::transaction::create_dir_all(parent)?;
    }
    let keys: Vec<String> = kind
        .key
        .first()
        .and_then(|key| meta.get(key))
        .cloned()
        .into_iter()
        .collect();
    if let Some(path) = crate::storage::issued::remember(context, kind, &keys)? {
        outcome.affected.push(path);
    }
    crate::execution::transaction::write(&path, text)?;
    if let Some(index) = index_append(context, kind, &filename, &meta)? {
        outcome.affected.push(index);
    }
    Ok(outcome)
}

/// Update metadata of one document.
pub fn set(
    context: &Context,
    kind: &Kind,
    keys: &[String],
    values: &Values,
    options: &Options,
) -> Result<Outcome> {
    let all = pages(context, kind)?;
    let page = find(kind, &all, keys).ok_or_else(|| missing(kind, keys))?;
    let mut meta = page.meta.clone();
    apply_key(kind, &mut meta, page);
    if let Some(state) = &kind.state
        && values.contains_key(&state.field)
    {
        return Err(Error::usage(format!(
            "field `{}` is driven by a transition",
            state.field
        )));
    }
    engine::apply_provided(context, kind, &mut meta, values, false)?;
    engine::apply_time(
        kind,
        &mut meta,
        context.workspace.policy.offset_hours,
        false,
    );
    crate::config::kind::validate_change(kind, &record_of(kind, page), &meta)?;
    engine::validate_record(context, kind, &meta)?;

    let title = meta
        .get("title")
        .cloned()
        .unwrap_or_else(|| page.title.clone());

    let mut outcome = Outcome::new(kind, "set");
    outcome.columns = show_columns(kind);
    outcome.records.push(meta.clone());
    outcome
        .affected
        .push(context.workspace.relative(&page.path));
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    let text = compose(kind, &meta, &title, &page.body);
    crate::execution::transaction::write(&page.path, text)?;
    if kind.index.is_some() {
        let filename = page
            .path
            .strip_prefix(
                context
                    .workspace
                    .resolve(kind.document_dir().expect("document")),
            )
            .map_err(|_| Error::usage("document lies outside its declared directory"))?
            .to_string_lossy()
            .replace(char::from(92), "/");
        index_remove(context, kind, &filename)?;
        if let Some(path) = index_append(context, kind, &filename, &meta)? {
            outcome.affected.push(path);
        }
    }
    Ok(outcome)
}

/// Run a declared transition on one document.
pub fn transition(
    context: &Context,
    kind: &Kind,
    name: &str,
    keys: &[String],
    values: &Values,
    options: &Options,
) -> Result<Outcome> {
    let state = kind
        .state
        .as_ref()
        .ok_or_else(|| Error::usage(format!("kind `{}` declares no state machine", kind.name)))?;
    let transition = kind
        .transition(name)
        .ok_or_else(|| Error::usage(format!("kind `{}` has no transition `{name}`", kind.name)))?;
    let all = pages(context, kind)?;
    let page = find(kind, &all, keys).ok_or_else(|| missing(kind, keys))?;
    let mut meta = page.meta.clone();
    apply_key(kind, &mut meta, page);

    let registry = context.registry(&state.registry)?;
    let current = meta.get(&state.field).cloned().unwrap_or_default();
    let current_key = registry.key_of(&current).unwrap_or(current.clone());
    if !transition.from.iter().any(|item| item == &current_key) {
        return Err(Error::conflict(format!(
            "`{}` is in state `{current_key}`; `{name}` allows {}",
            keys.join(" "),
            transition.from.join(", ")
        )));
    }

    engine::apply_provided(context, kind, &mut meta, values, false)?;
    for field in &transition.requires {
        if meta
            .get(field)
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(Error::validation(format!(
                "transition `{name}` requires `--{field}`"
            )));
        }
    }
    meta.insert(state.field.clone(), transition.to.clone());
    engine::apply_time(
        kind,
        &mut meta,
        context.workspace.policy.offset_hours,
        false,
    );
    crate::config::kind::validate_change(kind, &record_of(kind, page), &meta)?;
    engine::validate_record(context, kind, &meta)?;

    let title = meta
        .get("title")
        .cloned()
        .unwrap_or_else(|| page.title.clone());

    let mut outcome = Outcome::new(kind, name);
    outcome.columns = show_columns(kind);
    outcome.records.push(meta.clone());
    outcome
        .affected
        .push(context.workspace.relative(&page.path));
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    let text = compose(kind, &meta, &title, &page.body);
    crate::execution::transaction::write(&page.path, text)?;
    if kind.index.is_some() {
        let filename = page
            .path
            .strip_prefix(
                context
                    .workspace
                    .resolve(kind.document_dir().expect("document")),
            )
            .map_err(|_| Error::usage("document lies outside its declared directory"))?
            .to_string_lossy()
            .replace(char::from(92), "/");
        index_remove(context, kind, &filename)?;
        if let Some(path) = index_append(context, kind, &filename, &meta)? {
            outcome.affected.push(path);
        }
    }
    Ok(outcome)
}

/// Move one document into the archive directory.
pub fn archive(
    context: &Context,
    kind: &Kind,
    keys: &[String],
    options: &Options,
) -> Result<Outcome> {
    let all = pages(context, kind)?;
    let page = find(kind, &all, keys).ok_or_else(|| missing(kind, keys))?;
    let mut meta = page.meta.clone();
    apply_key(kind, &mut meta, page);
    let state = kind
        .state
        .as_ref()
        .ok_or_else(|| Error::usage(format!("kind `{}` declares no state machine", kind.name)))?;
    let registry = context.registry(&state.registry)?;
    let current = meta.get(&state.field).cloned().unwrap_or_default();
    let current_key = registry.key_of(&current).unwrap_or(current);
    if !engine::archivable(kind, &current_key) {
        return Err(Error::conflict(format!(
            "`{}` is in state `{current_key}`, which does not allow archiving",
            keys.join(" ")
        )));
    }
    let filename = page
        .path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let target = context
        .workspace
        .resolve(&kind.archive_dir())
        .join(&filename);

    let mut outcome = Outcome::new(kind, "archive");
    outcome.columns = show_columns(kind);
    outcome.records.push(meta);
    outcome.affected.push(context.workspace.relative(&target));
    outcome.removed.push(context.workspace.relative(&page.path));
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    if let Some(parent) = target.parent() {
        crate::execution::transaction::create_dir_all(parent)?;
    }
    crate::execution::transaction::rename(&page.path, &target)?;
    if let Some(index) = index_remove(context, kind, &filename)? {
        outcome.affected.push(index);
    }
    Ok(outcome)
}

/// Move every page dated before `--before` into the archive directory.
pub fn prune(
    context: &Context,
    kind: &Kind,
    options: &crate::execution::engine::Options,
) -> Result<Outcome> {
    let before = options
        .tokens
        .get("before")
        .cloned()
        .ok_or_else(|| Error::usage("prune needs --before <YYYYMMDD>"))?;
    let live = kind
        .document_dir()
        .ok_or_else(|| Error::usage(format!("kind `{}` is not document backed", kind.name)))?;
    let root = context.workspace.resolve(live);
    let target_root = context.workspace.resolve(&kind.archive_dir());
    let mut outcome = Outcome::new(kind, "prune");
    for page in pages(context, kind)? {
        let date = page.meta.get("date").cloned().unwrap_or_default();
        if date.trim().is_empty() || date.trim() >= before.trim() {
            continue;
        }
        let relative = page
            .path
            .strip_prefix(&root)
            .map(|tail| tail.to_path_buf())
            .unwrap_or_else(|_| page.path.clone());
        let target = target_root.join(&relative);
        outcome.records.push(record_of(kind, &page));
        outcome.affected.push(context.workspace.relative(&target));
        outcome.removed.push(context.workspace.relative(&page.path));
        if options.dry_run {
            continue;
        }
        if let Some(parent) = target.parent() {
            crate::execution::transaction::create_dir_all(parent)?;
        }
        crate::execution::transaction::rename(&page.path, &target)?;
    }
    if options.dry_run {
        outcome.dry_run = true;
    }
    Ok(outcome)
}
/// Move one document back from the archive directory.
pub fn restore(
    context: &Context,
    kind: &Kind,
    keys: &[String],
    options: &Options,
) -> Result<Outcome> {
    let dir = kind.archive_dir();
    let all = pages_in(context, kind, &dir)?;
    let page = find(kind, &all, keys).ok_or_else(|| missing(kind, keys))?;
    let mut meta = page.meta.clone();
    apply_key(kind, &mut meta, page);
    let live_dir = kind
        .document_dir()
        .ok_or_else(|| Error::usage(format!("kind `{}` is not document backed", kind.name)))?;
    let filename = page
        .path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let target = context.workspace.resolve(live_dir).join(&filename);
    if target.exists() {
        return Err(Error::conflict(format!(
            "{} already exists",
            context.workspace.relative(&target)
        )));
    }

    let mut outcome = Outcome::new(kind, "restore");
    outcome.columns = show_columns(kind);
    outcome.records.push(meta.clone());
    outcome.affected.push(context.workspace.relative(&target));
    outcome.removed.push(context.workspace.relative(&page.path));
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    if let Some(parent) = target.parent() {
        crate::execution::transaction::create_dir_all(parent)?;
    }
    crate::execution::transaction::rename(&page.path, &target)?;
    if let Some(index) = index_append(context, kind, &filename, &meta)? {
        outcome.affected.push(index);
    }
    Ok(outcome)
}

fn missing(kind: &Kind, keys: &[String]) -> Error {
    Error::usage(format!("no `{}` with key {}", kind.name, keys.join(" ")))
}

/// Fill the key field when the key is a single field.
///
/// A composite key is already spread over its fields by the metadata, so
/// nothing is synthesised for it.
fn apply_key(kind: &Kind, record: &mut Record, page: &Page) {
    if kind.key.len() == 1
        && let Some(first) = kind.key.first()
    {
        record.insert(first.clone(), key_of(kind, page));
    }
}

/// Structural checks that go beyond field values.
pub(crate) fn structure_problems(
    context: &Context,
    kind: &Kind,
    page: &Page,
    relative: &str,
) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();

    if !kind.sections.is_empty() {
        let headings: Vec<String> = page
            .body
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                if !trimmed.starts_with('#') {
                    return None;
                }
                Some(trimmed.trim_start_matches('#').trim().to_string())
            })
            .collect();
        for section in &kind.sections {
            if !headings.iter().any(|heading| heading.contains(section)) {
                out.push((
                    relative.to_string(),
                    format!("section `{section}` is missing"),
                ));
            }
        }
    }

    for key in &kind.metadata_required {
        let value = page.meta.get(key).cloned().unwrap_or_default();
        if value.trim().is_empty() {
            out.push((
                relative.to_string(),
                format!("metadata `{key}` is required"),
            ));
        }
    }

    if kind.metadata_strict {
        for key in page.meta.keys() {
            let declared = kind.field(key).is_some() || kind.metadata_required.contains(key);
            if !declared {
                out.push((
                    relative.to_string(),
                    format!("metadata `{key}` is not declared"),
                ));
            }
        }
    }

    for (line, message) in table_problems(&page.body) {
        out.push((relative.to_string(), format!("line {line}: {message}")));
    }

    for target in broken_links(&page.body, page.path.parent()) {
        out.push((
            relative.to_string(),
            format!("link `{target}` does not resolve"),
        ));
    }

    let _ = context;
    Ok(out)
}

/// Link targets in a body that do not resolve against a directory.
pub fn broken_links(body: &str, directory: Option<&std::path::Path>) -> Vec<String> {
    crate::collaboration::references::broken(body, directory)
}
/// Tables whose rows do not line up with their header.
pub fn table_problems(body: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = body.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        if !lines[index].trim_start().starts_with('|') {
            index += 1;
            continue;
        }
        let start = index;
        let mut expected = None;
        while index < lines.len() && lines[index].trim_start().starts_with('|') {
            let cells = count_cells(lines[index]);
            match expected {
                None => expected = Some(cells),
                Some(value) if value != cells => out.push((
                    start + index - start + 1,
                    format!("table row has {cells} cells where the header has {value}"),
                )),
                _ => {}
            }
            index += 1;
        }
    }
    out
}

/// Cells of a table row, ignoring escaped separators.
fn count_cells(line: &str) -> usize {
    let replaced = line.replace("\\|", "");
    let trimmed = replaced.trim().trim_matches('|');
    if trimmed.is_empty() {
        0
    } else {
        trimmed.split('|').count()
    }
}
/// Link targets inside a markdown body.
pub fn links_of(body: &str) -> Vec<String> {
    crate::collaboration::references::parse(body, false)
        .into_iter()
        .map(|reference| reference.target)
        .collect()
}

/// Header lines of a new page: the kind template, else the old defaults.
fn header_lines(context: &Context, kind: &Kind) -> Result<Vec<String>> {
    if let Some(template) = &kind.header {
        let text = crate::execution::render::template_text(context, template)?;
        return Ok(text
            .trim_end_matches('\n')
            .split('\n')
            .map(str::to_string)
            .collect());
    }
    Ok(vec![
        "# [{date}] {maintainer} log".to_string(),
        "maintainer: {maintainer}".to_string(),
        "operator: {operator}".to_string(),
    ])
}

/// Append one section to a dated page, creating the page when it is missing.
pub fn append(
    context: &Context,
    kind: &Kind,
    spec: &crate::config::kind::Action,
    options: &crate::execution::engine::Options,
) -> Result<crate::execution::engine::Outcome> {
    let title = options
        .tokens
        .get("title")
        .cloned()
        .or_else(|| options.tokens.get("heading").cloned())
        .ok_or_else(|| Error::usage(format!("`{}` needs --title <text>", spec.name)))?;
    if title.trim().is_empty() || title.contains(['\n', '\r']) {
        return Err(Error::usage("append title must be a non-empty single line"));
    }
    let metadata = crate::collaboration::log_entry::create(
        context,
        spec,
        options.tokens.get("tasks").map(String::as_str),
    )?;
    let identity = crate::config::identity::Identity::resolve(&context.workspace, None)?;
    let maintainer = options
        .tokens
        .get("side")
        .or_else(|| options.tokens.get("maintainer"))
        .cloned()
        .unwrap_or_else(|| identity.display());
    let epoch = crate::foundation::time::now_epoch();
    let offset = context.workspace.policy.offset_hours;
    let date = crate::foundation::time::date(epoch, offset);
    let clock = crate::foundation::time::stamp(epoch, offset);
    let time = clock.rsplit('-').next().unwrap_or("").to_string();

    let pattern = kind
        .document_filename()
        .ok_or_else(|| Error::usage(format!("kind `{}` has no filename pattern", kind.name)))?;
    let mut values: Record = Record::new();
    values.insert("maintainer".to_string(), maintainer.clone());
    values.insert("date".to_string(), date.clone());
    let relative = crate::execution::render::expand(pattern, &values);
    let path = context
        .workspace
        .resolve(kind.document_dir().unwrap_or_default())
        .join(&relative);
    let existed = path.exists();
    let mut page = if existed {
        std::fs::read_to_string(&path)
            .map_err(|error| crate::foundation::error::io_error(&path, error))?
    } else {
        let header = header_lines(context, kind)?;
        let operator = identity
            .operator
            .clone()
            .unwrap_or_else(|| identity.display());
        let mut page = String::new();
        for (index, line) in header.iter().enumerate() {
            let line = line
                .replace("{date}", &date)
                .replace("{maintainer}", &maintainer)
                .replace("{operator}", &operator);
            if index > 0 {
                page.push('\n');
            }
            page.push_str(&line);
        }
        page.push_str("\n\n");
        page
    };
    let heading = crate::execution::render::template_text(
        context,
        &spec
            .heading
            .clone()
            .unwrap_or_else(|| "## {title}".to_string()),
    )?
    .trim_end_matches('\n')
    .replace("{time}", &time)
    .replace("{date}", &date)
    .replace("{title}", &title);
    page.push('\n');
    page.push_str(&heading);
    page.push('\n');
    page.push_str(&metadata);
    let body = options.body.clone().unwrap_or_default();
    let body = body.trim_matches('\n');
    if !body.is_empty() {
        page.push('\n');
        page.push_str(body);
        page.push('\n');
    }
    crate::collaboration::log_entry::validate_page(context, kind, &page)?;
    if let Some(parent) = path.parent() {
        crate::execution::transaction::create_dir_all(parent)?;
    }
    crate::execution::transaction::write(&path, page)?;

    let mut outcome = crate::execution::engine::Outcome::new(kind, &spec.name);
    let mut record: Record = Record::new();
    record.insert("maintainer".to_string(), maintainer);
    record.insert("date".to_string(), date);
    record.insert("title".to_string(), title);
    outcome.records.push(record);
    outcome.affected.push(format!(
        "{}/{}",
        kind.document_dir().unwrap_or_default(),
        relative
    ));
    Ok(outcome)
}
/// Number of documents of a kind, used by diagnostics.
pub fn count(context: &Context, kind: &Kind) -> Result<usize> {
    Ok(pages(context, kind)?.len())
}

/// Path of the index page of a kind.
fn index_path(context: &Context, kind: &Kind) -> Option<PathBuf> {
    kind.index
        .as_ref()
        .map(|spec| context.workspace.resolve(&spec.file))
}

/// Extract the target of a markdown link cell, or the cell itself.
fn link_target(cell: &str) -> String {
    let cell = cell.trim();
    if let Some(open) = cell.find("](")
        && let Some(close) = cell[open + 2..].find(')')
    {
        return cell[open + 2..open + 2 + close].to_string();
    }
    cell.to_string()
}

/// Cells of one table row.
fn cells_of(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

/// Span of the markdown table inside the index page.
fn table_span(lines: &[String]) -> Option<(usize, usize)> {
    let start = lines
        .iter()
        .position(|line| line.trim_start().starts_with('|'))?;
    let mut end = start;
    while end + 1 < lines.len() && lines[end + 1].trim_start().starts_with('|') {
        end += 1;
    }
    Some((start, end))
}

/// Render one index row for a document.
fn index_row(kind: &Kind, filename: &str, record: &Record) -> Result<String> {
    let spec = kind
        .index
        .as_ref()
        .ok_or_else(|| Error::usage(format!("kind `{}` declares no index page", kind.name)))?;
    let mut cells = Vec::new();
    for column in &spec.columns {
        if column == "file" {
            cells.push(format!("[{filename}]({filename})"));
        } else {
            cells.push(record.get(column).cloned().unwrap_or_default());
        }
    }
    Ok(format!("| {} |", cells.join(" | ")))
}

/// Append a row for one document to the index page.
pub fn index_append(
    context: &Context,
    kind: &Kind,
    filename: &str,
    record: &Record,
) -> Result<Option<String>> {
    let Some(path) = index_path(context, kind) else {
        return Ok(None);
    };
    if !path.exists() {
        return Err(Error::usage(format!(
            "index page {} is missing",
            context.workspace.relative(&path)
        )));
    }
    let body = std::fs::read_to_string(&path)
        .map_err(|error| crate::foundation::error::io_error(&path, error))?;
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    let (_, end) = table_span(&lines).ok_or_else(|| {
        Error::usage(format!(
            "index page {} holds no table",
            context.workspace.relative(&path)
        ))
    })?;
    let row = index_row(kind, filename, record)?;
    lines.insert(end + 1, row);
    let mut out = lines.join("\n");
    out.push('\n');
    crate::execution::transaction::write(&path, out)?;
    Ok(Some(context.workspace.relative(&path)))
}

/// Drop the row of one document from the index page.
pub fn index_remove(context: &Context, kind: &Kind, filename: &str) -> Result<Option<String>> {
    let Some(path) = index_path(context, kind) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let body = std::fs::read_to_string(&path)
        .map_err(|error| crate::foundation::error::io_error(&path, error))?;
    let lines: Vec<String> = body.lines().map(str::to_string).collect();
    let Some((start, end)) = table_span(&lines) else {
        return Ok(None);
    };
    let kept: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            if *index < start || *index > end {
                return true;
            }
            let cells = cells_of(line);
            let target = cells
                .first()
                .map(|cell| link_target(cell))
                .unwrap_or_default();
            target != filename
        })
        .map(|(_, line)| line.clone())
        .collect();
    let mut out = kept.join("\n");
    out.push('\n');
    crate::execution::transaction::write(&path, out)?;
    Ok(Some(context.workspace.relative(&path)))
}

/// Index rows of a kind, as link targets.
fn index_targets(context: &Context, kind: &Kind) -> Result<Vec<String>> {
    let Some(path) = index_path(context, kind) else {
        return Ok(Vec::new());
    };
    if !path.exists() {
        return Ok(Vec::new());
    }
    let body = std::fs::read_to_string(&path)
        .map_err(|error| crate::foundation::error::io_error(&path, error))?;
    let lines: Vec<String> = body.lines().map(str::to_string).collect();
    let Some((start, end)) = table_span(&lines) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    // Skip the heading row and the separator row.
    for line in lines.iter().take(end + 1).skip(start + 2) {
        let cells = cells_of(line);
        if let Some(first) = cells.first() {
            out.push(link_target(first));
        }
    }
    Ok(out)
}

/// Compare the index page with the documents on disk.
pub(crate) fn index_problems(context: &Context, kind: &Kind) -> Result<Vec<(String, String)>> {
    if kind.index.is_none() {
        return Ok(Vec::new());
    }
    let spec = kind.index.as_ref().expect("checked above");
    let mut problems = Vec::new();
    let targets = index_targets(context, kind)?;
    let pages = pages(context, kind)?;
    let names: Vec<String> = pages
        .iter()
        .filter_map(|page| {
            page.path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
        })
        .collect();
    for target in &targets {
        // Rows may also point outside the directory, for example at tool
        // sources; only same directory markdown files are documents.
        if !target.ends_with(".md") || target.contains('/') {
            continue;
        }
        if !names.iter().any(|name| name == target) {
            problems.push((
                spec.file.clone(),
                format!("index row points at `{target}`, which is not a document"),
            ));
        }
    }
    for name in &names {
        if !targets.iter().any(|target| target == name) {
            problems.push((spec.file.clone(), format!("`{name}` has no index row")));
        }
    }
    Ok(problems)
}

/// Remove one document permanently.
pub fn delete(
    context: &Context,
    kind: &Kind,
    keys: &[String],
    options: &Options,
) -> Result<Outcome> {
    if !options.force {
        return Err(Error::usage(format!(
            "`{} delete` removes the page; pass --force to confirm",
            kind.name
        )));
    }
    let all = pages(context, kind)?;
    let page = find(kind, &all, keys).ok_or_else(|| missing(kind, keys))?;
    let mut outcome = Outcome::new(kind, "delete");
    outcome.columns = show_columns(kind);
    outcome.records.push(record_of(kind, page));
    outcome.removed.push(context.workspace.relative(&page.path));
    if options.dry_run {
        outcome.dry_run = true;
        return Ok(outcome);
    }
    if let Some(path) = crate::storage::issued::remember(context, kind, &[])? {
        outcome.affected.push(path);
    }
    crate::execution::transaction::remove(&page.path)?;
    if let Some(index) = index_remove(
        context,
        kind,
        &page
            .path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default(),
    )? {
        outcome.affected.push(index);
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_block_metadata() {
        let text = "# 标题\n\n<!-- clerk:meta\nid = CON-01\nstate = draft\n-->\n\n正文。\n";
        let (meta, title, body) = split_block(text);
        assert_eq!(meta.get("id").map(String::as_str), Some("CON-01"));
        assert_eq!(title, "标题");
        assert!(body.contains("正文。"));
        assert!(!body.contains("clerk:meta"));
    }

    #[test]
    fn splits_bullet_metadata() {
        let text = "# 复核\n\n- 日期: [20261027]\n- 状态: 有效\n\n## 一、结论\n\n正文。\n";
        let (meta, title, body) = split_bullets(text);
        assert_eq!(meta.get("日期").map(String::as_str), Some("20261027"));
        assert_eq!(meta.get("状态").map(String::as_str), Some("有效"));
        assert_eq!(title, "复核");
        assert!(body.starts_with("## 一、结论"), "body: {body}");
    }

    #[test]
    fn reads_fields_from_the_file_name() {
        let fields = parse_name("{id}-{slug}.md", "01-archive-prune.md");
        assert_eq!(fields.get("id").map(String::as_str), Some("01"));
        assert_eq!(
            fields.get("slug").map(String::as_str),
            Some("archive-prune")
        );

        let fields = parse_name("{date}-{time}-{slug}.md", "20261027-185600-reservation.md");
        assert_eq!(fields.get("date").map(String::as_str), Some("20261027"));
        assert_eq!(fields.get("time").map(String::as_str), Some("185600"));
        assert_eq!(fields.get("slug").map(String::as_str), Some("reservation"));
        assert!(parse_name("{id}-{slug}.md", "01x.md").is_empty());
    }
}
