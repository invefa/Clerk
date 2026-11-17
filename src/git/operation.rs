//! Local execution receipts and Git-only recovery, separate from business data.

use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    NotRun,
    Pending,
    Skipped,
    Succeeded,
    Failed,
    Unknown,
}
impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotRun => "not_run",
            Self::Pending => "pending",
            Self::Skipped => "skipped",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        match value {
            "not_run" => Ok(Self::NotRun),
            "pending" => Ok(Self::Pending),
            "skipped" => Ok(Self::Skipped),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "unknown" => Ok(Self::Unknown),
            _ => Err(Error::usage("invalid operation stage")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Data {
    NotWritten,
    Pending,
    RolledBack,
    Saved,
    Unknown,
}
impl Data {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotWritten => "not_written",
            Self::Pending => "pending",
            Self::RolledBack => "rolled_back",
            Self::Saved => "saved",
            Self::Unknown => "unknown",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        match value {
            "not_written" => Ok(Self::NotWritten),
            "pending" => Ok(Self::Pending),
            "rolled_back" => Ok(Self::RolledBack),
            "saved" => Ok(Self::Saved),
            "unknown" => Ok(Self::Unknown),
            _ => Err(Error::usage("invalid operation data state")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Report {
    pub id: String,
    pub data: Data,
    pub commit: Stage,
    pub push: Stage,
    pub head: String,
    pub before: String,
    pub verb: String,
    pub message: Option<String>,
    pub actions: Vec<crate::git::commit_message::Change>,
    pub files: BTreeMap<String, String>,
    pub batch: Option<toml::Table>,
}

thread_local! { static CURRENT: RefCell<Option<Report>> = const { RefCell::new(None) }; }

pub fn reset() {
    CURRENT.with(|state| *state.borrow_mut() = None);
}
pub fn current() -> Option<Report> {
    CURRENT.with(|state| state.borrow().clone())
}
pub fn change(update: impl FnOnce(&mut Report)) {
    CURRENT.with(|state| {
        if let Some(report) = state.borrow_mut().as_mut() {
            update(report);
        }
    });
}

pub fn begin(workspace: &Workspace) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let report = Report {
        id: format!("op-{}-{}", base36(nanos), base36(std::process::id().into())),
        data: Data::NotWritten,
        commit: Stage::NotRun,
        push: Stage::NotRun,
        head: String::new(),
        before: crate::git::commit::head_full(workspace).unwrap_or_default(),
        verb: String::new(),
        message: None,
        actions: Vec::new(),
        files: BTreeMap::new(),
        batch: None,
    };
    CURRENT.with(|state| *state.borrow_mut() = Some(report));
}

fn base36(mut number: u128) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(number % 36) as usize] as char);
        number /= 36;
        if number == 0 {
            break;
        }
    }
    digits.into_iter().rev().collect()
}

fn id_time(id: &str) -> Option<u128> {
    if let Some(rest) = id.strip_prefix("op-") {
        let (stamp, process) = rest.split_once('-')?;
        if stamp.is_empty()
            || process.is_empty()
            || !rest
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return None;
        }
        u128::from_str_radix(process, 36).ok()?;
        u128::from_str_radix(stamp, 36).ok()
    } else {
        let (stamp, process) = id.split_once('-')?;
        process.parse::<u32>().ok()?;
        stamp.parse().ok()
    }
}

pub fn prepare(workspace: &Workspace, paths: &[String], verb: &str) -> Result<()> {
    let mut files = BTreeMap::new();
    for path in paths {
        files.insert(path.clone(), digest(workspace, path)?);
    }
    change(|report| {
        report.verb = verb.into();
        report.files = files;
        report.data = Data::Pending;
    });
    save(workspace)
}

pub fn digest(workspace: &Workspace, path: &str) -> Result<String> {
    use sha2::Digest;
    let file = workspace.resolve(path);
    if !file.exists() {
        return Ok("deleted".into());
    }
    let bytes =
        std::fs::read(&file).map_err(|error| crate::foundation::error::io_error(&file, error))?;
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}

pub(crate) fn directory(workspace: &Workspace) -> Result<PathBuf> {
    Ok(crate::execution::transaction::local_dir(workspace)?.join("operations"))
}

pub fn save(workspace: &Workspace) -> Result<()> {
    let Some(report) = current() else {
        return Ok(());
    };
    let dir = directory(workspace)?;
    std::fs::create_dir_all(&dir).map_err(|e| crate::foundation::error::io_error(&dir, e))?;
    let id = report.id.clone();
    crate::execution::transaction::atomic_write(
        &dir.join(format!("{id}.toml")),
        encode(report).as_bytes(),
    )
}

fn encode(report: Report) -> String {
    let mut table = toml::Table::new();
    table.insert("version".into(), toml::Value::Integer(1));
    for (key, value) in [
        ("id", report.id),
        ("data", report.data.name().into()),
        ("commit", report.commit.name().into()),
        ("push", report.push.name().into()),
        ("head", report.head),
        ("before", report.before),
        ("verb", report.verb),
    ] {
        table.insert(key.into(), toml::Value::String(value));
    }
    table.insert(
        "message".into(),
        toml::Value::String(report.message.unwrap_or_default()),
    );
    table.insert(
        "files".into(),
        toml::Value::Table(
            report
                .files
                .into_iter()
                .map(|(k, v)| (k, toml::Value::String(v)))
                .collect(),
        ),
    );
    if let Some(batch) = report.batch {
        table.insert("batch".into(), toml::Value::Table(batch));
    }
    table.to_string()
}

pub(crate) fn accepted_receipt() -> Result<(String, String)> {
    let mut report = current().ok_or_else(|| Error::io("missing operation receipt"))?;
    report.data = Data::Saved;
    Ok((report.id.clone(), encode(report)))
}

pub fn load(workspace: &Workspace, id: Option<&str>) -> Result<Report> {
    let dir = directory(workspace)?;
    let path = match id {
        Some(id) if id_time(id).is_some() => dir.join(format!("{id}.toml")),
        Some(_) => {
            return Err(Error::usage(
                "invalid operation id; use the id from operation status",
            ));
        }
        None => {
            let mut paths: Vec<_> = std::fs::read_dir(&dir)
                .map_err(|e| crate::foundation::error::io_error(&dir, e))?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
                .collect();
            paths.sort_by_key(|path| {
                path.file_stem()
                    .and_then(|id| id.to_str())
                    .and_then(id_time)
                    .unwrap_or(0)
            });
            let mut latest = None;
            while let Some(path) = paths.pop() {
                let table = crate::config::decl::parse(
                    &std::fs::read_to_string(&path)
                        .map_err(|e| crate::foundation::error::io_error(&path, e))?,
                    "local operation",
                )?;
                if table.get("historical").and_then(toml::Value::as_bool) != Some(true) {
                    latest = Some(path);
                    break;
                }
            }
            latest.ok_or_else(|| Error::usage("no local operation receipt"))?
        }
    };
    let table = crate::config::decl::parse(
        &std::fs::read_to_string(&path)
            .map_err(|e| crate::foundation::error::io_error(&path, e))?,
        "operation",
    )?;
    if table.get("version").and_then(toml::Value::as_integer) != Some(1) {
        return Err(Error::usage("unsupported operation receipt version"));
    }
    let text = |key| crate::config::decl::required_text(&table, key, "operation");
    Ok(Report {
        id: text("id")?,
        data: Data::parse(&text("data")?)?,
        commit: Stage::parse(&text("commit")?)?,
        push: Stage::parse(&text("push")?)?,
        head: crate::config::decl::text(&table, "head").unwrap_or_default(),
        before: crate::config::decl::text(&table, "before").unwrap_or_default(),
        verb: text("verb")?,
        message: crate::config::decl::text(&table, "message").filter(|message| !message.is_empty()),
        actions: Vec::new(),
        batch: crate::config::decl::sub_table(&table, "batch"),
        files: crate::config::decl::sub_table(&table, "files")
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|v| (k, v.to_string()))
                    .ok_or_else(|| Error::usage("invalid operation file digest"))
            })
            .collect::<Result<_>>()?,
    })
}

pub fn set(report: Report) {
    CURRENT.with(|state| *state.borrow_mut() = Some(report));
}

impl Report {
    pub fn json(&self) -> String {
        format!(
            "{{\"id\":\"{}\",\"data\":\"{}\",\"commit\":\"{}\",\"push\":\"{}\",\"head\":\"{}\",\"message\":{},\"batch\":{}}}",
            crate::terminal::output::escape(&self.id),
            self.data.name(),
            self.commit.name(),
            self.push.name(),
            crate::terminal::output::escape(&self.head),
            self.message
                .as_deref()
                .map(crate::config::schema::quote)
                .unwrap_or_else(|| "null".into()),
            self.batch
                .as_ref()
                .map(|batch| serde_json::to_string(batch).expect("batch receipt JSON"))
                .unwrap_or_else(|| "null".into())
        )
    }
    pub fn summary(&self) -> String {
        format!(
            "operation {}: data={}, commit={}, push={}, head={}",
            self.id,
            self.data.name(),
            self.commit.name(),
            self.push.name(),
            self.head
        )
    }
}
