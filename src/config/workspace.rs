//! Workspace discovery and declaration loading.
//!
//! A workspace is a directory tree marked by `.clerk/workspace.toml`. Every
//! path in that declaration is relative to the workspace root, so one base
//! covers configuration, data and output files.

use std::path::{Path, PathBuf};

use crate::config::decl::{self, Table};
use crate::foundation::error::{Error, Result};

/// Normalize lexical aliases without following symlinks or changing write boundaries.
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if out.file_name().is_some_and(|part| part != "..") => {
                out.pop();
            }
            std::path::Component::ParentDir if !out.has_root() => out.push(".."),
            std::path::Component::ParentDir => {}
            _ => out.push(part.as_os_str()),
        }
    }
    out
}

pub const MARKER: &str = ".clerk/workspace.toml";
const ENV_ROOT: &str = "CLERK_ROOT";

/// Layout paths used by the tool, all relative to the root.
#[derive(Clone, Debug)]
pub struct Layout {
    pub registry: String,
    pub kinds: String,
    pub data: String,
    pub status: String,
    pub seal: String,
    pub seal_log: String,
    pub identity: String,
    pub requests: String,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            registry: ".clerk/config/registry".to_string(),
            kinds: ".clerk/config/kinds".to_string(),
            data: ".clerk/data".to_string(),
            status: "status.md".to_string(),
            seal: ".clerk/seal/seal.csv".to_string(),
            seal_log: "log/seal-log.md".to_string(),
            identity: ".work/identity".to_string(),
            requests: ".clerk/requests".to_string(),
        }
    }
}

/// Policy switches read from the declaration.
#[derive(Clone, Debug)]
pub struct Policy {
    /// Hours west or east of UTC used for stamps.
    pub offset_hours: i32,
    /// Commit managed changes after a write command.
    pub auto_commit: bool,
    /// Push the commit to the configured remote.
    pub auto_push: bool,
    /// How an omitted commit message is resolved.
    pub commit_mode: CommitMode,
    /// Fixed message used by the default mode.
    pub commit_message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitMode {
    Required,
    Auto,
    Default,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            offset_hours: 0,
            auto_commit: false,
            auto_push: false,
            commit_mode: CommitMode::Auto,
            commit_message: "chore(clerk): update managed state".to_string(),
        }
    }
}

/// Governance strength of a managed path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Written by the tool; a manual edit is a mistake.
    Generated,
    /// Hashed into the seal table; a manual edit needs a reseal.
    Sealed,
    /// Editable by people, checked for structure and values.
    Validated,
    /// Content tracked by hash.
    Tracked,
    /// Only referenced by the ledger.
    Referenced,
    /// Not governed at all.
    Free,
}

impl Level {
    pub fn parse(text: &str, context: &str) -> Result<Self> {
        Ok(match text {
            "generated" => Level::Generated,
            "sealed" => Level::Sealed,
            "validated" => Level::Validated,
            "tracked" => Level::Tracked,
            "referenced" => Level::Referenced,
            "free" => Level::Free,
            other => {
                return Err(Error::usage(format!(
                    "{context}: unknown level `{other}`; expected generated, sealed, validated, tracked, referenced or free"
                )));
            }
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Level::Generated => "generated",
            Level::Sealed => "sealed",
            Level::Validated => "validated",
            Level::Tracked => "tracked",
            Level::Referenced => "referenced",
            Level::Free => "free",
        }
    }
}

/// One managed path declaration.
#[derive(Clone, Debug)]
pub struct PathRule {
    pub path: String,
    pub level: Level,
    /// Named regions inside the file, used instead of the whole file.
    pub regions: Vec<String>,
}

impl PathRule {
    /// Whether the rule covers a workspace relative path.
    pub fn covers(&self, affected: &str) -> bool {
        affected == self.path
            || affected
                .strip_prefix(&self.path)
                .map(|rest| rest.starts_with('/'))
                .unwrap_or(false)
    }
}

/// Sync settings: which remote and branch to compare with.
#[derive(Clone, Debug)]
pub struct SyncPolicy {
    pub enabled: bool,
    pub remote: String,
    pub branch: String,
    /// Refuse a write when the remote branch is not known locally.
    pub fail_closed: bool,
    /// Fetch the remote before a write command checks the branch.
    pub write_fetch: bool,
    /// Fetch the remote before a read command checks the branch.
    pub read_fetch: bool,
    /// Reuse a fetch for this many seconds.
    pub cache_seconds: u64,
    pub write_update: bool,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        SyncPolicy {
            enabled: true,
            remote: "origin".to_string(),
            branch: "main".to_string(),
            fail_closed: false,
            write_fetch: false,
            read_fetch: false,
            cache_seconds: 0,
            write_update: false,
        }
    }
}

/// An open workspace.
#[derive(Clone, Debug)]
pub struct Workspace {
    root: PathBuf,
    pub layout: Layout,
    pub policy: Policy,
    pub name: String,
    pub paths: Vec<PathRule>,
    pub sync: SyncPolicy,
    /// Per command defaults from `[commands."name"]`.
    pub commands: crate::config::decl::Table,
    pub terminal: crate::config::decl::Table,
}

impl Workspace {
    /// Locate the root and load the declaration.
    pub fn open(explicit: Option<&str>) -> Result<Self> {
        let root = Self::locate(explicit)?;
        Self::at(&root)
    }

    /// Load the declaration from a known root.
    pub fn at(root: &Path) -> Result<Self> {
        let marker = root.join(MARKER);
        if !marker.exists() {
            return Err(Error::usage(format!(
                "no workspace at {}: {} is missing",
                root.display(),
                MARKER
            )));
        }
        let body = std::fs::read_to_string(&marker)
            .map_err(|error| crate::foundation::error::io_error(&marker, error))?;
        let table = decl::parse(&body, MARKER)?;
        decl::only_keys(
            &table,
            &[
                "version", "name", "time", "layout", "policy", "sync", "ids", "fields", "path",
                "commands", "terminal",
            ],
            MARKER,
        )?;
        let version = decl::integer(&table, "version").unwrap_or(1);
        if version != 1 {
            return Err(Error::usage(format!(
                "{MARKER}: unsupported version {version}; expected 1"
            )));
        }

        let mut layout = Layout::default();
        if let Some(section) = decl::sub_table(&table, "layout") {
            decl::only_keys(
                &section,
                &[
                    "registry",
                    "kinds",
                    "data",
                    "status",
                    "seal",
                    "seal_log",
                    "identity",
                    "requests",
                    "log",
                    "log_archive",
                    "log_heading",
                    "log_maintainer",
                    "log_operator",
                ],
                "layout",
            )?;
            if let Some(value) = decl::text(&section, "registry") {
                layout.registry = value;
            }
            if let Some(value) = decl::text(&section, "kinds") {
                layout.kinds = value;
            }
            if let Some(value) = decl::text(&section, "data") {
                layout.data = value;
            }
            if let Some(value) = decl::text(&section, "status") {
                layout.status = value;
            }
            if let Some(value) = decl::text(&section, "seal") {
                layout.seal = value;
            }
            if let Some(value) = decl::text(&section, "seal_log") {
                layout.seal_log = value;
            }
            if let Some(value) = decl::text(&section, "identity") {
                layout.identity = value;
            }
            if let Some(value) = decl::text(&section, "requests") {
                layout.requests = value;
            }
        }

        let mut policy = Policy::default();
        if let Some(section) = decl::sub_table(&table, "time") {
            decl::only_keys(&section, &["offset_hours"], "time")?;
            if let Some(value) = decl::integer(&section, "offset_hours") {
                let hours = i32::try_from(value).map_err(|_| {
                    Error::usage(format!("time.offset_hours: {value} is out of range"))
                })?;
                if !(-12..=14).contains(&hours) {
                    return Err(Error::usage(format!(
                        "time.offset_hours: {hours} is outside -12..=14"
                    )));
                }
                policy.offset_hours = hours;
            }
        }
        let mut sync = SyncPolicy::default();
        let commands = decl::sub_table(&table, "commands").unwrap_or_default();
        if table.get("terminal").is_some_and(|value| !value.is_table()) {
            return Err(Error::usage("terminal must be a table"));
        }
        let terminal = decl::sub_table(&table, "terminal").unwrap_or_default();
        decl::only_keys(&terminal, &["overflow", "paging", "color"], "terminal")?;
        for (name, value) in &terminal {
            let text = value
                .as_str()
                .ok_or_else(|| Error::usage(format!("terminal.{name} must be a string")))?;
            crate::interface::command::option(name)
                .expect("terminal option")
                .validate(text)?;
        }
        let terminal_values = terminal
            .iter()
            .map(|(name, value)| (name.clone(), value.as_str().unwrap_or_default().to_string()))
            .collect();
        crate::interface::command::validate_requirements(&terminal_values)
            .map_err(|error| Error::usage(format!("terminal: {error}")))?;

        if let Some(section) = decl::sub_table(&table, "policy") {
            decl::only_keys(
                &section,
                &[
                    "auto_commit",
                    "auto_push",
                    "commit_message",
                    "commit_mode",
                    "push_remote",
                    "push_branch",
                ],
                "policy",
            )?;
            if let Some(value) = decl::text(&section, "push_remote") {
                sync.remote = value;
            }
            if let Some(value) = decl::text(&section, "push_branch") {
                sync.branch = value;
            }
            policy.auto_commit = decl::flag(&section, "auto_commit").unwrap_or(false);
            policy.auto_push = decl::flag(&section, "auto_push").unwrap_or(false);
            if let Some(value) = decl::text(&section, "commit_message") {
                policy.commit_message = crate::git::commit_message::validate(&value)?;
                policy.commit_mode = CommitMode::Default;
            } else if section.contains_key("commit_message") {
                return Err(Error::usage("policy.commit_message must be a string"));
            }
            if let Some(value) = section.get("commit_mode") {
                policy.commit_mode = match value.as_str() {
                    Some("required") => CommitMode::Required,
                    Some("auto") => CommitMode::Auto,
                    Some("default") => CommitMode::Default,
                    _ => {
                        return Err(Error::usage(
                            "policy.commit_mode needs required, auto or default",
                        ));
                    }
                };
            }
        }
        if let Some(section) = decl::sub_table(&table, "sync") {
            decl::only_keys(
                &section,
                &[
                    "enabled",
                    "remote",
                    "branch",
                    "fail_closed",
                    "write_fetch",
                    "write_update",
                    "read_fetch",
                    "cache_seconds",
                ],
                "sync",
            )?;
            sync.enabled = decl::flag(&section, "enabled").unwrap_or(true);
            sync.fail_closed = decl::flag(&section, "fail_closed").unwrap_or(false);
            sync.write_fetch = decl::flag(&section, "write_fetch").unwrap_or(false);
            if section
                .get("write_update")
                .is_some_and(|value| !value.is_str())
            {
                return Err(Error::usage("sync.write_update must be a string"));
            }
            sync.write_update = match decl::text(&section, "write_update").as_deref() {
                None | Some("refuse") => false,
                Some("ff-only") => true,
                Some(_) => return Err(Error::usage("sync.write_update must be refuse or ff-only")),
            };
            sync.read_fetch = decl::flag(&section, "read_fetch").unwrap_or(false);
            sync.cache_seconds =
                decl::integer(&section, "cache_seconds").unwrap_or(0).max(0) as u64;
            if let Some(value) = decl::text(&section, "remote") {
                sync.remote = value;
            }
            if let Some(value) = decl::text(&section, "branch") {
                sync.branch = value;
            }
        }

        let mut paths = Vec::new();
        for entry in decl::array_of_tables(&table, "path") {
            decl::only_keys(&entry, &["path", "level", "regions"], "path")?;
            let path = decl::required_text(&entry, "path", "path")?;
            if path.starts_with('/') || path.contains(':') || path.contains("..") {
                return Err(Error::usage(format!(
                    "path `{path}` must be relative to the workspace root"
                )));
            }
            let level_text = decl::text(&entry, "level").unwrap_or_else(|| "free".to_string());
            let level = Level::parse(&level_text, &format!("path `{path}`"))?;
            paths.push(PathRule {
                path,
                level,
                regions: decl::strings(&entry, "regions"),
            });
        }

        let name = decl::text(&table, "name").unwrap_or_else(|| {
            root.file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "workspace".to_string())
        });

        Ok(Workspace {
            root: root.to_path_buf(),
            layout,
            policy,
            name,
            paths,
            sync,
            commands,
            terminal,
        })
    }

    /// The most specific rule covering a path.
    pub fn rule_for(&self, affected: &str) -> Option<&PathRule> {
        self.paths
            .iter()
            .filter(|rule| rule.covers(affected))
            .max_by_key(|rule| rule.path.len())
    }

    pub fn rules_at(&self, level: Level) -> Vec<&PathRule> {
        self.paths
            .iter()
            .filter(|rule| rule.level == level)
            .collect()
    }

    fn locate(explicit: Option<&str>) -> Result<PathBuf> {
        if let Some(value) = explicit {
            return Self::verify(Path::new(value));
        }
        if let Ok(value) = std::env::var(ENV_ROOT)
            && !value.trim().is_empty()
        {
            return Self::verify(Path::new(value.trim()));
        }
        let current = std::env::current_dir()
            .map_err(|error| Error::io(format!("cannot read the current directory: {error}")))?;
        let mut probe: Option<&Path> = Some(&current);
        while let Some(dir) = probe {
            if dir.join(MARKER).exists() {
                return Ok(dir.to_path_buf());
            }
            probe = dir.parent();
        }
        Err(Error::usage(format!(
            "no workspace found from {} upward; run inside a workspace or pass --root",
            current.display()
        )))
    }

    fn verify(path: &Path) -> Result<PathBuf> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|error| Error::io(format!("cannot read the current directory: {error}")))?
                .join(path)
        };
        if !absolute.join(MARKER).exists() {
            return Err(Error::usage(format!(
                "{} does not hold {}",
                absolute.display(),
                MARKER
            )));
        }
        let normalized = normalize(&absolute);
        if normalized != absolute {
            let actual = absolute
                .canonicalize()
                .map_err(|e| crate::foundation::error::io_error(&absolute, e))?;
            let candidate = normalized
                .canonicalize()
                .map_err(|e| crate::foundation::error::io_error(&normalized, e))?;
            if actual != candidate {
                return Err(Error::usage(
                    "workspace root traversal crosses a symbolic link; use its resolved path",
                ));
            }
        }
        Ok(normalized)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Whether a relative path falls under one of the declared managed paths.
    pub fn covers(&self, relative: &str) -> bool {
        self.paths.iter().any(|rule| {
            relative == rule.path
                || relative.starts_with(&format!("{}/", rule.path))
                || rule.path.starts_with(&format!("{relative}/"))
        })
    }
    /// Legacy identifiers mapped to the key they stand for.
    pub fn aliases(&self) -> Result<std::collections::BTreeMap<String, String>> {
        let mut out = std::collections::BTreeMap::new();
        let path = self.registry_dir().join("alias.csv");
        if !path.exists() {
            return Ok(out);
        }
        let table = crate::storage::store::Table::load(&path)?;
        let Some(alias_at) = table.column("alias") else {
            return Ok(out);
        };
        let key_at = table.column("key").unwrap_or(1);
        for row in &table.rows {
            let alias = row.get(alias_at).map(|value| value.trim()).unwrap_or("");
            let key = row.get(key_at).map(|value| value.trim()).unwrap_or("");
            if !alias.is_empty() && !key.is_empty() {
                out.insert(alias.to_string(), key.to_string());
            }
        }
        Ok(out)
    }
    pub fn registry_dir(&self) -> PathBuf {
        self.resolve(&self.layout.registry)
    }

    pub fn kinds_dir(&self) -> PathBuf {
        self.resolve(&self.layout.kinds)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.resolve(&self.layout.data)
    }

    pub fn identity_path(&self) -> PathBuf {
        self.resolve(&self.layout.identity)
    }

    /// Numbering schemes declared under `[ids.<name>]`.
    pub fn numbering(&self) -> Result<Vec<(String, Table)>> {
        let marker = self.root.join(MARKER);
        let body = std::fs::read_to_string(&marker)
            .map_err(|error| crate::foundation::error::io_error(&marker, error))?;
        let table = decl::parse(&body, MARKER)?;
        let mut out = Vec::new();
        if let Some(section) = decl::sub_table(&table, "ids") {
            for (name, value) in &section {
                let inner = value.as_table().cloned().ok_or_else(|| {
                    Error::usage(format!("ids.{name}: expected a table of settings"))
                })?;
                out.push((name.clone(), inner));
            }
        }
        out.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(out)
    }

    /// Field defaults declared under `[fields.<name>]`.
    pub fn field_options(&self) -> Result<Vec<(String, Table)>> {
        let marker = self.root.join(MARKER);
        let body = std::fs::read_to_string(&marker)
            .map_err(|error| crate::foundation::error::io_error(&marker, error))?;
        let table = decl::parse(&body, MARKER)?;
        let mut out = Vec::new();
        if let Some(section) = decl::sub_table(&table, "fields") {
            for (name, value) in &section {
                let inner = value.as_table().cloned().ok_or_else(|| {
                    Error::usage(format!("fields.{name}: expected a table of settings"))
                })?;
                out.push((name.clone(), inner));
            }
        }
        out.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(out)
    }

    /// The workspace relative form of a path, using forward slashes.
    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|value| value.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"))
    }
}
