//! Sourced, bounded handoff projections; no fetch, recovery or business writes.

use crate::config::kind::Kind;
use crate::config::schema::{object, quote, strings_json};
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::interface::cli::Args;
use crate::storage::store::Record;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

struct Source {
    path: String,
    digest: String,
}

struct Part {
    role: String,
    path: String,
    line: usize,
    text: String,
    truncated: bool,
}

fn bounded(text: &str, limit: usize) -> (String, bool) {
    (
        text.chars().take(limit).collect(),
        text.chars().count() > limit,
    )
}

fn record_json(record: &Record) -> String {
    object(
        record
            .iter()
            .map(|(key, value)| (key.as_str(), quote(value))),
    )
}

/// Read through a single provenance cache so excerpts and digests describe the same bytes.
struct Bundle<'a> {
    context: &'a Context,
    full: bool,
    contents: BTreeMap<String, String>,
    diagnostics: Vec<String>,
    parts: Vec<Part>,
}

impl Bundle<'_> {
    fn read(&mut self, path: &str) -> Option<String> {
        if let Some(text) = self.contents.get(path) {
            return Some(text.clone());
        }
        let absolute = self.context.workspace.resolve(path);
        match std::fs::read_to_string(&absolute) {
            Ok(text) => {
                self.contents.insert(path.into(), text.clone());
                Some(text)
            }
            Err(error) => {
                self.diagnostics.push(format!("{path}: {error}"));
                None
            }
        }
    }

    fn part(&mut self, role: &str, path: &str, line: usize, text: &str, limit: usize) {
        let (text, truncated) = if self.full {
            (text.to_string(), false)
        } else {
            bounded(text, limit)
        };
        self.parts.push(Part {
            role: role.into(),
            path: path.into(),
            line,
            text,
            truncated,
        });
    }

    fn find(&mut self, kind: &Kind, key: &str) -> Result<Option<(Record, String, bool)>> {
        if kind.document_dir().is_some() {
            for (dir, archived) in [
                (kind.document_dir().expect("document").to_string(), false),
                (kind.archive_dir(), true),
            ] {
                for page in crate::storage::document::pages_in(self.context, kind, &dir)? {
                    if crate::storage::document::key_of(kind, &page) == key {
                        let path = self.context.workspace.relative(&page.path);
                        self.read(&path);
                        return Ok(Some((
                            crate::storage::document::record_of(kind, &page),
                            path,
                            archived,
                        )));
                    }
                }
            }
        } else {
            for (path, archived) in [
                (kind.table_path().to_string(), false),
                (kind.archive_table(), true),
            ] {
                if !self.context.workspace.resolve(&path).exists() {
                    continue;
                }
                if let Some(text) = self.read(&path) {
                    let table = crate::storage::store::Table::from_text(&text)?;
                    if let Some(index) = table.find(
                        &kind.key,
                        &key.split(' ').map(str::to_string).collect::<Vec<_>>(),
                    ) {
                        return Ok(Some((table.record(index), path, archived)));
                    }
                }
            }
        }
        Ok(None)
    }
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
}

struct GitObservation {
    checkout: String,
    branch: Option<String>,
    head: Option<String>,
    status: Option<String>,
}

impl GitObservation {
    fn read(path: &Path) -> Self {
        Self {
            checkout: path.display().to_string(),
            branch: git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"]),
            head: git(path, &["rev-parse", "HEAD"]),
            status: git(path, &["status", "--porcelain", "--untracked-files=all"]),
        }
    }
    fn json(&self) -> String {
        object([
            ("checkout", quote(&self.checkout)),
            (
                "branch",
                self.branch
                    .as_deref()
                    .map(quote)
                    .unwrap_or_else(|| "null".into()),
            ),
            (
                "head",
                self.head
                    .as_deref()
                    .map(quote)
                    .unwrap_or_else(|| "null".into()),
            ),
            (
                "status",
                self.status
                    .as_deref()
                    .map(quote)
                    .unwrap_or_else(|| "null".into()),
            ),
        ])
    }
    fn fields(&self) -> Vec<(String, String)> {
        vec![
            (
                "branch:".into(),
                self.branch.as_deref().unwrap_or("unknown/detached").into(),
            ),
            (
                "head:".into(),
                self.head.as_deref().unwrap_or("unknown").into(),
            ),
            (
                "status:".into(),
                self.status
                    .as_deref()
                    .map(|s| if s.is_empty() { "clean" } else { s })
                    .unwrap_or("unknown")
                    .into(),
            ),
        ]
    }
    fn text(&self, label: &str, compact: bool) -> String {
        let details = crate::terminal::text::pairs_with(&self.fields(), 0, compact);
        crate::terminal::text::group(label, &self.checkout, &details)
    }
}

pub fn command(root: Option<&str>, args: &Args) -> Result<i32> {
    if args.value("sync").is_some() || args.switch("dry-run") || args.value("request").is_some() {
        return Err(Error::usage(
            "context is read-only; --sync, --dry-run and --request are unavailable",
        ));
    }
    let initial = Context::open(root)?;
    let _lock = crate::execution::transaction::WriterLock::snapshot(&initial.workspace)?;
    let context = Context::open(root)?;
    let name = args
        .positional
        .get(1)
        .ok_or_else(|| Error::usage("usage: clerk context <kind> <key>"))?;
    let kind = context.catalog.require(name)?;
    let name = &kind.name;
    if args.positional.len() != kind.key.len() + 2 {
        return Err(Error::usage("context requires all declared key components"));
    }
    let key = crate::query::selection::resolve(&context, kind, &args.positional[2..], args, true)?
        .join(" ");
    let mut bundle = Bundle {
        context: &context,
        full: args.switch("full"),
        contents: BTreeMap::new(),
        diagnostics: Vec::new(),
        parts: Vec::new(),
    };
    bundle.read(
        &context
            .workspace
            .relative(&context.workspace.root().join(".clerk/workspace.toml")),
    );
    bundle.read(&kind.source);
    let (record, source, archived) = bundle
        .find(kind, &key)?
        .ok_or_else(|| Error::validation(format!("unknown `{name}` key `{key}`")))?;
    let spec = kind.context.as_ref();
    if spec.is_none() {
        bundle.diagnostics.push(format!(
            "kind `{name}` has no [context] declaration; record and relations only"
        ));
    }
    if let Some(spec) = spec {
        for field in &spec.attachments {
            if let Some(path) = record.get(field).filter(|v| !v.is_empty())
                && let Some(text) = bundle.read(path)
            {
                bundle.part(field, path, 1, &text, 4000);
            }
        }
    }
    let mut related = Vec::<(String, String, String, String)>::new();
    // Source kind + declared relation name disambiguate the unqualified relation table.
    for owner in context.catalog.kinds() {
        let Some(path) = &owner.relations_table else {
            continue;
        };
        if !context.workspace.resolve(path).exists() {
            continue;
        }
        let Some(text) = bundle.read(path) else {
            continue;
        };
        let table = crate::storage::store::Table::from_text(&text)?;
        for index in 0..table.rows.len() {
            let edge = table.record(index);
            let from = edge.get("from").map(String::as_str).unwrap_or("");
            let to = edge.get("to").map(String::as_str).unwrap_or("");
            let relation = edge.get("kind").map(String::as_str).unwrap_or("");
            let Some(declaration) = owner.relations.iter().find(|r| r.name == relation) else {
                continue;
            };
            if owner.name == *name && from == key {
                related.push((
                    "outgoing".into(),
                    relation.into(),
                    declaration.target.clone(),
                    to.into(),
                ));
            }
            if declaration.target == *name && to == key {
                related.push((
                    "incoming".into(),
                    relation.into(),
                    owner.name.clone(),
                    from.into(),
                ));
            }
        }
    }
    for field in &kind.fields {
        if let Some(target) = &field.target {
            for key in record
                .get(&field.name)
                .map(String::as_str)
                .unwrap_or("")
                .split('|')
                .filter(|key| !key.is_empty())
            {
                related.push((
                    "reference".into(),
                    field.name.clone(),
                    target.clone(),
                    key.into(),
                ));
            }
        }
    }
    if related.len() > 50 {
        bundle.diagnostics.push(format!(
            "relations truncated: {} total, 50 included",
            related.len()
        ));
        related.truncate(50);
    }
    let mut relation_json = Vec::new();
    let mut relation_lines = Vec::new();
    for (direction, relation, target, key) in related {
        let target_kind = context.catalog.require(&target)?;
        bundle.read(&target_kind.source);
        let result = bundle.find(target_kind, &key)?;
        let (data, path, archived) = if let Some((record, path, archived)) = result {
            if target_kind.document_dir().is_some()
                && let Some(text) = bundle.read(&path)
            {
                bundle.part("related document", &path, 1, &text, 2000);
            }
            (record_json(&record), quote(&path), archived.to_string())
        } else {
            bundle
                .diagnostics
                .push(format!("unresolved relation {relation}: {target} {key}"));
            ("null".into(), "null".into(), "null".into())
        };
        relation_lines.push(vec![
            direction.clone(),
            relation.clone(),
            target.clone(),
            key.clone(),
        ]);
        relation_json.push(object([
            ("direction", quote(&direction)),
            ("relation", quote(&relation)),
            ("kind", quote(&target)),
            ("key", quote(&key)),
            ("source", path),
            ("archived", archived),
            ("record", data),
        ]));
    }
    if let Some(logs) = spec.and_then(|s| s.logs.as_deref()) {
        let logs_kind = context.catalog.require(logs)?;
        bundle.read(&logs_kind.source);
        let mut entries = Vec::new();
        for (path, text) in crate::storage::records::mentioned_logs(&context, logs_kind, &key)? {
            for entry in crate::collaboration::log_entry::parse(&text)? {
                if entry.matches(Some(&kind.name), &key) {
                    let date = Path::new(&path)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy();
                    let role = if entry.explicit(Some(&kind.name), &key) {
                        "log explicit reference"
                    } else {
                        "log text mention"
                    };
                    entries.push((
                        format!("{date} {} {path}", entry.text.lines().next().unwrap_or("")),
                        path.clone(),
                        entry.line,
                        entry.text,
                        role,
                    ));
                }
            }
            bundle.contents.insert(path, text);
        }
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        if entries.len() > 5 {
            bundle.diagnostics.push(format!(
                "logs truncated: {} entries, 5 included",
                entries.len()
            ));
        }
        for (_, path, line, text, role) in entries.into_iter().take(5) {
            bundle.part(role, &path, line, &text, 1500);
        }
    }
    let sessions = if kind.name == "tasks" {
        match crate::collaboration::session::snapshot(&context, &key) {
            Ok(rows) => rows,
            Err(error) => {
                bundle
                    .diagnostics
                    .push(format!("session snapshot unavailable: {error}"));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let mut repositories = Vec::new();
    let mut repository_lines = Vec::new();
    if let Some(field) = spec.and_then(|s| s.repos.as_deref()) {
        let registry_name = kind
            .field(field)
            .and_then(|f| f.registry.as_deref())
            .expect("validated repos registry");
        bundle.read(
            &context.workspace.relative(
                &context
                    .workspace
                    .registry_dir()
                    .join(format!("{registry_name}.csv")),
            ),
        );
        let registry = context.registry(registry_name)?;
        for key in record
            .get(field)
            .map(String::as_str)
            .unwrap_or("")
            .split('|')
            .filter(|key| !key.is_empty())
        {
            let Some(entry) = registry.entries().iter().find(|entry| entry.key == key) else {
                bundle
                    .diagnostics
                    .push(format!("unknown repository `{key}`"));
                continue;
            };
            let remotes = entry.cells.get("remotes").cloned().unwrap_or_default();
            let checkout = crate::interface::commands::find_checkout(&context, &remotes)
                .map(std::path::PathBuf::from);
            if checkout.is_none() {
                bundle
                    .diagnostics
                    .push(format!("repository `{key}` has no matching local checkout"));
            }
            let observation = checkout.as_deref().map(GitObservation::read);
            repository_lines.push(
                observation
                    .as_ref()
                    .map(|observation| observation.text(&format!("{key}:"), args.switch("compact")))
                    .unwrap_or_else(|| {
                        crate::terminal::text::group(&format!("{key}:"), "missing", "")
                    }),
            );
            repositories.push(object([
                ("key", quote(key)),
                ("registry", record_json(&entry.cells)),
                (
                    "observation",
                    observation
                        .as_ref()
                        .map(GitObservation::json)
                        .unwrap_or_else(|| "null".into()),
                ),
            ]));
        }
    }
    let sources = bundle
        .contents
        .iter()
        .map(|(path, text)| Source {
            path: path.clone(),
            digest: format!("{:x}", Sha256::digest(text.as_bytes())),
        })
        .collect::<Vec<_>>();
    let parts = bundle
        .parts
        .iter()
        .map(|part| {
            object([
                ("role", quote(&part.role)),
                ("path", quote(&part.path)),
                ("line", part.line.to_string()),
                ("text", quote(&part.text)),
                ("truncated", part.truncated.to_string()),
            ])
        })
        .collect::<Vec<_>>();
    let observed = crate::foundation::time::stamp(
        crate::foundation::time::now_epoch(),
        context.workspace.policy.offset_hours,
    );
    let upstream = format!(
        "refs/remotes/{}/{}",
        context.workspace.sync.remote, context.workspace.sync.branch
    );
    let workspace_observation = GitObservation::read(context.workspace.root());
    let data = object([
        ("version", "1".into()),
        ("observed_at", quote(&observed)),
        ("kind", quote(name)),
        ("key", quote(&key)),
        ("source", quote(&source)),
        ("archived", archived.to_string()),
        ("record", record_json(&record)),
        ("workspace", workspace_observation.json()),
        (
            "remote_observation",
            object([
                ("cached", "true".into()),
                ("ref", quote(&upstream)),
                (
                    "head",
                    git(
                        context.workspace.root(),
                        &["rev-parse", "--verify", &upstream],
                    )
                    .as_deref()
                    .map(quote)
                    .unwrap_or_else(|| "null".into()),
                ),
            ]),
        ),
        ("relations", format!("[{}]", relation_json.join(","))),
        ("repositories", format!("[{}]", repositories.join(","))),
        (
            "sessions",
            format!(
                "[{}]",
                sessions
                    .iter()
                    .map(|row| crate::config::schema::json(&toml::Value::Table(row.clone())))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
        ("parts", format!("[{}]", parts.join(","))),
        (
            "sources",
            format!(
                "[{}]",
                sources
                    .iter()
                    .map(|s| object([("path", quote(&s.path)), ("sha256", quote(&s.digest))]))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
        ("diagnostics", strings_json(&bundle.diagnostics)),
    ]);
    drop(_lock);
    if args.switch("json") {
        println!("{data}");
    } else {
        let compact = args.switch("compact");
        let reading = crate::terminal::size::reading(
            args.switch("plain"),
            args.value("width").and_then(|value| value.parse().ok()),
        );
        let mut output = String::new();
        output.push_str(&format!("Context: {name} {key}\n"));
        output.push('\n');
        output.push_str(&crate::terminal::text::pairs_with(
            &[
                ("Observed:".into(), observed),
                ("Source:".into(), source),
                ("Archived:".into(), archived.to_string()),
            ],
            0,
            compact,
        ));
        let fields = record
            .iter()
            .map(|(key, value)| (format!("{key}:"), value.clone()))
            .collect::<Vec<_>>();
        let field_start = output.lines().count() + 1;
        let mut field_document: crate::terminal::reader::Document =
            crate::terminal::text::pairs_with(&fields, 0, compact).into();
        field_document.detail_styles(
            &context,
            kind,
            std::slice::from_ref(&record),
            &record.keys().cloned().collect::<Vec<_>>(),
            false,
        )?;
        output.push('\n');
        output.push_str(&field_document.text);
        let remote_head = git(
            context.workspace.root(),
            &["rev-parse", "--verify", &upstream],
        )
        .unwrap_or_else(|| "unknown".into());
        let remote_fields = crate::terminal::text::pairs_with(
            &[("ref:".into(), upstream), ("head:".into(), remote_head)],
            0,
            compact,
        );
        let mut details =
            crate::terminal::text::pairs_with(&workspace_observation.fields(), 0, compact);
        details.push_str(&crate::terminal::text::group(
            "remote (cached):",
            "",
            &remote_fields,
        ));
        output.push_str(&format!(
            "\n{}",
            crate::terminal::text::group("Workspace:", &workspace_observation.checkout, &details)
        ));
        output.push_str(&format!(
            "\n{}",
            crate::terminal::text::group("Repositories:", "", &repository_lines.concat())
        ));
        let mut session_lines = String::new();
        for row in &sessions {
            let details = row
                .iter()
                .filter(|(key, _)| key.as_str() != "id")
                .map(|(key, value)| {
                    (
                        format!("{key}:"),
                        value
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| value.to_string()),
                    )
                })
                .collect::<Vec<_>>();
            session_lines.push_str(&crate::terminal::text::group(
                "Session:",
                row.get("id").and_then(toml::Value::as_str).unwrap_or(""),
                &crate::terminal::text::pairs_with(&details, 0, compact),
            ));
        }
        if !session_lines.is_empty() {
            output.push_str(&format!(
                "\n{}",
                crate::terminal::text::group("Sessions:", "", &session_lines)
            ));
        }
        let headers = ["direction", "relation", "kind", "key"].map(String::from);
        let relations = crate::terminal::text::table(
            crate::interface::cli::headers_shown(args)?.then_some(headers.as_slice()),
            &relation_lines,
            args.max_width(),
            compact,
        );
        output.push_str(&format!(
            "\n{}",
            crate::terminal::text::group("Relations:", "", &relations)
        ));
        let mut excerpts = String::new();
        for part in &bundle.parts {
            let mut metadata = vec![("Source:".into(), format!("{}:{}", part.path, part.line))];
            if part.truncated {
                metadata.push(("Truncated:".into(), "true".into()));
                metadata.push((
                    "Hint:".into(),
                    "use --full for complete excerpt bodies".into(),
                ));
            }
            let mut children = crate::terminal::text::pairs_with(&metadata, 0, compact);
            children.push_str(&crate::terminal::text::group(
                "Content:",
                "",
                &reading.literal(&part.text, part.line),
            ));
            excerpts.push_str(&crate::terminal::text::group(
                &format!("{}:", part.role),
                "",
                &children,
            ));
        }
        output.push_str(&format!(
            "\n{}",
            crate::terminal::text::group("Excerpts:", "", &excerpts)
        ));
        let source_rows = sources
            .iter()
            .map(|source| (source.path.clone(), format!("sha256={}", source.digest)))
            .collect::<Vec<_>>();
        let sources = crate::terminal::text::pairs_with(&source_rows, 0, compact);
        let source_start = output.lines().count() + 2;
        output.push_str(&format!(
            "\n{}",
            crate::terminal::text::group("Sources:", "", &sources)
        ));
        let source_end = output.lines().count();
        for diagnostic in &bundle.diagnostics {
            output.push_str(&format!("diagnostic: {diagnostic}"));
            output.push('\n');
        }
        let mut document: crate::terminal::reader::Document = output.into();
        let field_end = field_start + field_document.styles.len();
        for (line, styles) in field_document.styles.into_iter().enumerate() {
            document.styles[field_start + line] = styles;
        }
        for (line, text) in document.logical.iter().enumerate() {
            if (source_start..source_end).contains(&line) {
                document.styles[line].push(crate::terminal::style::Span {
                    range: 0..text.len(),
                    role: "muted".into(),
                });
                continue;
            }
            if (field_start..field_end).contains(&line)
                || crate::terminal::style::gutter(text).is_some()
            {
                continue;
            }
            let Some((label, body)) = text.split_once(": ") else {
                continue;
            };
            // Roles belong to generated metadata, never to words in excerpt bodies.
            let role = match label.trim() {
                "Source" | "Observed" => "muted",
                "Truncated" | "diagnostic" => "warning",
                "Hint" => "info",
                "Context" => "identifier",
                _ => continue,
            };
            let start = if label.trim() == "diagnostic" {
                0
            } else {
                label.len() + 2 + body.len() - body.trim_start().len()
            };
            document.styles[line].push(crate::terminal::style::Span {
                range: start..text.len(),
                role: role.into(),
            });
        }
        crate::terminal::reader::present(args, false, |width| {
            Ok(match width {
                Some(width) => document.reading_width(width),
                None => document.clone(),
            })
        })?;
    }
    Ok(0)
}
