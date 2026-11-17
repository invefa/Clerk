//! Requests: a write captured as a manifest and replayed later.
//!
//! A request is not a second implementation of the verbs. It stores the same
//! arguments the command would have used, plus who asked and which commit the
//! ask was based on, so applying it runs the ordinary path.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::decl::{self, Table};
use crate::config::identity::Identity;
use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};
use crate::foundation::time;

/// One captured write.
#[derive(Clone, Debug)]
pub struct Manifest {
    pub path: PathBuf,
    pub name: String,
    pub kind: String,
    pub verb: String,
    pub keys: Vec<String>,
    pub values: BTreeMap<String, String>,
    pub tokens: BTreeMap<String, String>,
    pub requester: String,
    pub email: String,
    pub baseline: String,
    pub created: String,
    pub body: Option<String>,
    pub force: bool,
    pub digest: String,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let body = std::fs::read_to_string(path)
            .map_err(|error| crate::foundation::error::io_error(path, error))?;
        let context = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        let table = decl::parse(&body, &context)?;
        for key in ["body", "requester", "email", "baseline", "created"] {
            if table.get(key).is_some_and(|value| !value.is_str()) {
                return Err(Error::usage(format!("{context}: `{key}` must be a string")));
            }
        }
        if table.get("force").is_some_and(|value| !value.is_bool()) {
            return Err(Error::usage(format!(
                "{context}: `force` must be a boolean"
            )));
        }
        if table
            .get("version")
            .is_some_and(|value| !value.is_integer())
        {
            return Err(Error::usage(format!(
                "{context}: `version` must be an integer"
            )));
        }
        if let Some(keys) = table.get("keys") {
            let valid = keys
                .as_array()
                .is_some_and(|keys| keys.iter().all(|key| key.is_str()));
            if !valid {
                return Err(Error::usage(format!(
                    "{context}: `keys` must be an array of strings"
                )));
            }
        }
        for key in ["values", "tokens"] {
            if let Some(values) = table.get(key) {
                let valid = values
                    .as_table()
                    .is_some_and(|values| values.values().all(|value| value.is_str()));
                if !valid {
                    return Err(Error::usage(format!(
                        "{context}: `{key}` must contain string values"
                    )));
                }
            }
        }
        decl::only_keys(
            &table,
            &[
                "version",
                "kind",
                "verb",
                "keys",
                "values",
                "tokens",
                "requester",
                "email",
                "baseline",
                "created",
                "body",
                "force",
            ],
            &context,
        )?;
        let version = decl::integer(&table, "version").unwrap_or(1);
        if version != 1 && version != 2 {
            return Err(Error::usage(format!(
                "{context}: unsupported request version {version}"
            )));
        }
        let stem = path
            .file_stem()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        let name = stem
            .split_once('-')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_else(|| stem.clone());
        Ok(Manifest {
            path: path.to_path_buf(),
            name,
            kind: decl::required_text(&table, "kind", &context)?,
            verb: decl::required_text(&table, "verb", &context)?,
            keys: decl::strings(&table, "keys"),
            values: pairs(&table, "values"),
            tokens: pairs(&table, "tokens"),
            requester: decl::text(&table, "requester").unwrap_or_default(),
            email: decl::text(&table, "email").unwrap_or_default(),
            baseline: decl::text(&table, "baseline").unwrap_or_default(),
            created: decl::text(&table, "created").unwrap_or_default(),
            body: decl::text(&table, "body"),
            force: table
                .get("force")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            digest: {
                use sha2::Digest;
                format!("{:x}", sha2::Sha256::digest(body.as_bytes()))
            },
        })
    }
}

fn pairs(table: &Table, key: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(section) = decl::sub_table(table, key) {
        for (name, value) in &section {
            if let Some(text) = value.as_str() {
                out.insert(name.clone(), text.to_string());
            }
        }
    }
    out
}

fn pending_dir(workspace: &Workspace) -> PathBuf {
    workspace
        .resolve(&workspace.layout.requests)
        .join("pending")
}

/// The write a request stands for.
pub struct NewRequest<'a> {
    pub kind: &'a str,
    pub verb: &'a str,
    pub keys: &'a [String],
    pub values: &'a BTreeMap<String, String>,
    pub tokens: &'a BTreeMap<String, String>,
    pub name: &'a str,
    pub body: Option<&'a str>,
    pub force: bool,
}

/// Capture a write instead of applying it.
pub fn capture(
    workspace: &Workspace,
    identity: &Identity,
    request: NewRequest<'_>,
) -> Result<PathBuf> {
    let NewRequest {
        kind,
        verb,
        keys,
        values,
        tokens,
        name,
        body: prose,
        force,
    } = request;
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') {
        return Err(Error::usage(
            "request name must be a plain name without separators",
        ));
    }
    let dir = pending_dir(workspace);
    crate::execution::transaction::create_dir_all(&dir)?;
    let path = dir.join(format!("{kind}-{name}.toml"));
    if path.exists() {
        return Err(Error::conflict(format!(
            "request {} already exists",
            workspace.relative(&path)
        )));
    }
    let baseline = crate::git::commit::head(workspace).unwrap_or_default();
    let created = time::stamp(time::now_epoch(), workspace.policy.offset_hours);
    let mut body = String::new();
    body.push_str("version = 2\n");
    body.push_str(&format!("kind = {}\n", quote(kind)));
    body.push_str(&format!("verb = {}\n", quote(verb)));
    body.push_str(&format!("keys = {}\n", array(keys)));
    body.push_str(&format!("requester = {}\n", quote(&identity.git_name)));
    body.push_str(&format!("email = {}\n", quote(&identity.git_email)));
    body.push_str(&format!("baseline = {}\n", quote(&baseline)));
    body.push_str(&format!("created = {}\n", quote(&created)));
    body.push_str(&format!("force = {force}\n"));
    if let Some(prose) = prose {
        body.push_str(&format!("body = {}\n", quote(prose)));
    }
    if !values.is_empty() {
        body.push_str("\n[values]\n");
        for (key, value) in values {
            body.push_str(&format!("{} = {}\n", quote_key(key), quote(value)));
        }
    }
    if !tokens.is_empty() {
        body.push_str("\n[tokens]\n");
        for (key, value) in tokens {
            body.push_str(&format!("{} = {}\n", quote_key(key), quote(value)));
        }
    }
    crate::execution::transaction::write(&path, body)?;
    Ok(path)
}

fn quote_key(key: &str) -> String {
    if key
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        key.to_string()
    } else {
        quote(key)
    }
}

fn array(items: &[String]) -> String {
    let parts: Vec<String> = items.iter().map(|item| quote(item)).collect();
    format!("[{}]", parts.join(", "))
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Every pending request.
pub fn list(workspace: &Workspace) -> Result<Vec<Manifest>> {
    let dir = pending_dir(workspace);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|error| crate::foundation::error::io_error(&dir, error))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|ext| ext == "toml").unwrap_or(false))
        .collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        out.push(Manifest::load(&path)?);
    }
    Ok(out)
}

/// Remove a request file once it has been handled.
pub fn remove(workspace: &Workspace, path: &Path) -> Result<()> {
    if path.exists() {
        crate::execution::transaction::remove(path)?;
    }
    let _ = workspace;
    Ok(())
}

/// Append a receipt line to one of the request logs.
pub fn append_log(workspace: &Workspace, log: &str, manifest: &Manifest, note: &str) -> Result<()> {
    let dir = workspace.resolve(&workspace.layout.requests);
    crate::execution::transaction::create_dir_all(&dir)?;
    let path = dir.join(format!("{log}.log"));
    let stamp = time::stamp(time::now_epoch(), workspace.policy.offset_hours);
    let line = format!(
        "{stamp} {} {} {} {} sha256={} {note}\n",
        manifest.name, manifest.kind, manifest.verb, manifest.requester, manifest.digest
    );
    crate::execution::transaction::append(&path, &line)?;
    Ok(())
}

/// Whether the baseline commit still exists in the repository.
pub fn baseline_known(workspace: &Workspace, baseline: &str) -> bool {
    if baseline.len() < 7
        || baseline.len() > 64
        || !baseline.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return false;
    }
    std::process::Command::new("git")
        .arg("-C")
        .arg(workspace.root())
        .args(["cat-file", "-e", &format!("{baseline}^{{commit}}")])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Reject replay of a receipt, a divergent baseline, or a changed target.
pub fn validate_apply(
    context: &crate::execution::context::Context,
    manifest: &Manifest,
) -> Result<()> {
    use crate::foundation::error::Failure;
    let stale = |message: String| Error::new(Failure::Stale, message);
    let workspace = &context.workspace;
    let log = workspace
        .resolve(&workspace.layout.requests)
        .join("applied.log");
    let text = match std::fs::read_to_string(&log) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(crate::foundation::error::io_error(&log, error)),
    };
    {
        let digest = format!("sha256={}", manifest.digest);
        if text
            .lines()
            .any(|line| line.split_whitespace().any(|part| part == digest))
        {
            return Err(Error::conflict(format!(
                "request {} was already applied",
                manifest.name
            )));
        }
    }
    if !baseline_known(workspace, &manifest.baseline) {
        return Err(stale(format!(
            "request {} is based on unknown commit {}",
            manifest.name, manifest.baseline
        )));
    }
    let git = |arguments: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(workspace.root())
            .args(arguments)
            .output()
            .map_err(|error| Error::io(format!("cannot run git: {error}")))
    };
    if !git(&["merge-base", "--is-ancestor", &manifest.baseline, "HEAD"])?
        .status
        .success()
    {
        return Err(stale(format!(
            "request {} baseline is not an ancestor of HEAD",
            manifest.name
        )));
    }
    let config = git(&[
        "diff",
        "--quiet",
        &manifest.baseline,
        "--",
        crate::config::workspace::MARKER,
        &workspace.layout.kinds,
        &workspace.layout.registry,
    ])?;
    if !config.status.success() {
        return Err(stale(format!(
            "request {} declarations changed since its baseline",
            manifest.name
        )));
    }
    let kind = context.catalog.require(&manifest.kind)?;
    if !crate::interface::command::verbs(kind)
        .iter()
        .any(|verb| verb.name == manifest.verb && verb.writes)
    {
        return Err(Error::usage(format!(
            "request {} needs a declared write verb; `{}` is not one of {}",
            manifest.name,
            manifest.verb,
            crate::interface::command::verbs(kind)
                .into_iter()
                .filter(|verb| verb.writes)
                .map(|verb| verb.name)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if matches!(manifest.verb.as_str(), "new" | "create") {
        return Ok(());
    }
    let archived = manifest.verb == "restore";
    if kind.document_dir().is_some() {
        // A document has its own file; refusing changes anywhere in this kind
        // is conservative for operations such as append or prune without keys.
        let dir = if archived {
            kind.archive_dir()
        } else {
            kind.document_dir().unwrap().to_string()
        };
        let difference = git(&["diff", "--quiet", &manifest.baseline, "--", &dir])?;
        let untracked = git(&["ls-files", "--others", "--exclude-standard", "--", &dir])?;
        if !difference.status.success()
            || !untracked.status.success()
            || !untracked.stdout.is_empty()
        {
            return Err(stale(format!(
                "request {} document target changed since its baseline",
                manifest.name
            )));
        }
    } else {
        let relative = if archived {
            kind.archive_table()
        } else {
            kind.table_path().to_string()
        };
        let old = git(&["show", &format!("{}:{relative}", manifest.baseline)])?;
        if !old.status.success() {
            return Err(stale(format!(
                "request {} target was absent at its baseline",
                manifest.name
            )));
        }
        let before =
            crate::storage::store::Table::from_text(&String::from_utf8_lossy(&old.stdout))?;
        let current = crate::storage::records::table(context, kind, archived)?;
        let keys: Vec<String> = manifest.keys.iter().take(kind.key.len()).cloned().collect();
        let before = before
            .find(&kind.key, &keys)
            .map(|index| before.record(index));
        let current = current
            .find(&kind.key, &keys)
            .map(|index| current.record(index));
        let unchanged = match (before, current) {
            (Some(before), Some(current)) => kind.fields.iter().all(|field| {
                before.get(&field.name).map(String::as_str).unwrap_or("")
                    == current.get(&field.name).map(String::as_str).unwrap_or("")
            }),
            _ => false,
        };
        if !unchanged {
            return Err(stale(format!(
                "request {} target changed since its baseline",
                manifest.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_strings_for_toml() {
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_key("title"), "title");
        assert_eq!(quote_key("odd key"), "\"odd key\"");
    }
}
