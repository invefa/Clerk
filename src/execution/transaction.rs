//! Local writer exclusion and an undo journal for managed file mutations.
//!
//! The journal is accepted before Git runs: a failed push must never undo a
//! commit. It protects process interruption, not power loss or external editors.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result, io_error};

thread_local! {
    static ACTIVE: RefCell<Option<Rc<RefCell<Journal>>>> = const { RefCell::new(None) };
}

struct Journal {
    root: PathBuf,
    dir: PathBuf,
    entries: BTreeMap<String, Option<String>>,
    initial_clean: bool,
}

/// Process lock, acquired before synchronization and held through Git finalization.
pub struct WriterLock {
    root: PathBuf,
    local: PathBuf,
    _file: File,
}

impl WriterLock {
    pub fn acquire(workspace: &Workspace) -> Result<Self> {
        Self::acquire_mode(workspace, true)
    }

    /// A snapshot never recovers pending mutations or starts an undo journal.
    pub fn snapshot(workspace: &Workspace) -> Result<Self> {
        Self::acquire_mode(workspace, false)
    }

    fn acquire_mode(workspace: &Workspace, recover_pending: bool) -> Result<Self> {
        if ACTIVE.with(|active| active.borrow().is_some()) {
            return Err(Error::conflict("a write transaction is already active"));
        }
        let root = workspace
            .root()
            .canonicalize()
            .map_err(|e| io_error(workspace.root(), e))?;
        let local = local_dir(workspace)?;
        fs::create_dir_all(&local).map_err(|e| io_error(&local, e))?;
        let path = local.join("write.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| io_error(&path, e))?;
        file.try_lock().map_err(|e| {
            Error::conflict(format!(
                "workspace writer lock is unavailable: {e}; retry after the other writer exits"
            ))
        })?;
        if recover_pending {
            recover(&root, &local.join("undo"))?;
            import_receipts(workspace, &local, true)?;
        } else if local.join("undo").exists() {
            return Err(Error::conflict(
                "unfinished write journal exists; run a write command to recover before collecting context",
            ));
        }
        Ok(Self {
            root,
            local,
            _file: file,
        })
    }
}

/// Metadata belongs to this checkout, including when it is a Git worktree.
pub fn local_dir(workspace: &Workspace) -> Result<PathBuf> {
    let root = workspace
        .root()
        .canonicalize()
        .map_err(|e| io_error(workspace.root(), e))?;
    let git = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
        .unwrap_or_else(|| root.join(".work"));
    let local = metadata_dir(&git)?;
    import_identity(workspace, &local)?;
    import_receipts(workspace, &local, false)?;
    Ok(local)
}

/// Move the old clone-local operator without overwriting either side on conflict.
fn import_identity(workspace: &Workspace, local: &Path) -> Result<()> {
    let old = workspace.identity_path();
    let target = local.join("identity");
    if old == target || !old.exists() {
        return Ok(());
    }
    fs::create_dir_all(local).map_err(|e| io_error(local, e))?;
    let _lock = identity_lock(local)?;
    move_metadata(&old, &target)
}

pub(crate) fn identity_lock(local: &Path) -> Result<File> {
    let lock_path = local.join("identity.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| io_error(&lock_path, e))?;
    lock.try_lock().map_err(|e| {
        Error::conflict(format!(
            "identity migration needs an idle operator writer: {e}"
        ))
    })?;
    Ok(lock)
}

/// Upgrade metadata once, then use only the scoped layout. Never overwrite collisions.
pub(crate) fn metadata_dir(git: &Path) -> Result<PathBuf> {
    let local = git.join("clerk");
    let names = [
        ("clerk-operations", "operations"),
        ("clerk-batches", "batches"),
        ("clerk-sessions", "sessions"),
        ("clerk-undo", "undo"),
        ("clerk-sync.stamp", "sync.stamp"),
    ];
    if !names.iter().any(|(old, _)| git.join(old).exists())
        && !git.join("clerk-write.lock").exists()
    {
        return Ok(local);
    }
    fs::create_dir_all(&local).map_err(|e| io_error(&local, e))?;
    let open_lock = |path: &Path| -> Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| io_error(path, e))?;
        file.try_lock().map_err(|e|Error::conflict(format!("metadata migration needs idle old and new writers: {e}; stop old Clerk processes and retry")))?;
        Ok(file)
    };
    let old_path = git.join("clerk-write.lock");
    let old_lock = open_lock(&old_path)?;
    let _new_lock = open_lock(&local.join("write.lock"))?;
    let old_sessions = if git.join("clerk-sessions").exists() {
        Some(open_lock(&git.join("clerk-sessions/lock"))?)
    } else {
        None
    };
    let _new_sessions = if old_sessions.is_some() || local.join("sessions").exists() {
        fs::create_dir_all(local.join("sessions")).map_err(|e| io_error(&local, e))?;
        Some(open_lock(&local.join("sessions/lock"))?)
    } else {
        None
    };
    for (old, new) in names {
        if old == "clerk-sessions" && git.join(old).exists() {
            for entry in fs::read_dir(git.join(old)).map_err(|e| io_error(&git.join(old), e))? {
                let entry = entry.map_err(|e| io_error(&git.join(old), e))?;
                if entry.file_name() != "lock" {
                    move_metadata(&entry.path(), &local.join(new).join(entry.file_name()))?;
                }
            }
        } else {
            move_metadata(&git.join(old), &local.join(new))?;
        }
    }
    drop(old_sessions);
    if git.join("clerk-sessions").exists() {
        let lock = git.join("clerk-sessions/lock");
        fs::remove_file(&lock).map_err(|e| io_error(&lock, e))?;
        fs::remove_dir(git.join("clerk-sessions"))
            .map_err(|e| io_error(&git.join("clerk-sessions"), e))?;
    }
    drop(old_lock);
    fs::remove_file(&old_path).map_err(|e| io_error(&old_path, e))?;
    Ok(local)
}

fn move_metadata(from: &Path, to: &Path) -> Result<()> {
    if [from, to]
        .iter()
        .any(|path| fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()))
    {
        return Err(Error::usage(
            "metadata migration does not follow symbolic links",
        ));
    }
    if !from.exists() {
        return Ok(());
    }
    if !to.exists() {
        return fs::rename(from, to).map_err(|e| io_error(from, e));
    }
    if from.is_dir() && to.is_dir() {
        for entry in fs::read_dir(from).map_err(|e| io_error(from, e))? {
            let entry = entry.map_err(|e| io_error(from, e))?;
            move_metadata(&entry.path(), &to.join(entry.file_name()))?;
        }
        return fs::remove_dir(from).map_err(|e| io_error(from, e));
    }
    // An upgrade may resume after some entries were already copied.
    if from.is_file()
        && to.is_file()
        && fs::read(from).map_err(|e| io_error(from, e))?
            == fs::read(to).map_err(|e| io_error(to, e))?
    {
        return fs::remove_file(from).map_err(|e| io_error(from, e));
    }
    Err(Error::conflict(format!(
        "metadata migration collision at {}; inspect both files before retrying",
        to.display()
    )))
}

/// Import historical shared batch receipts without changing tracked files.
fn import_receipts(workspace: &Workspace, local: &Path, locked: bool) -> Result<()> {
    let old = workspace.resolve(".clerk/operations");
    if !old.exists() || local.join("undo").exists() {
        return Ok(());
    }
    let dir = local.join("operations");
    for entry in fs::read_dir(&old).map_err(|e| io_error(&old, e))? {
        let path = entry.map_err(|e| io_error(&old, e))?.path();
        if path.extension().is_none_or(|ext| ext != "toml") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| io_error(&path, e))?;
        let batch = crate::config::decl::parse(&text, "historical batch receipt")?;
        let id = crate::config::decl::required_text(&batch, "id", "batch receipt")?;
        if !safe_receipt_name(&format!("{id}.toml")) {
            return Err(Error::usage("invalid historical receipt id"));
        }
        let target = dir.join(format!("{id}.toml"));
        let existing = if target.exists() {
            Some(fs::read_to_string(&target).map_err(|e| io_error(&target, e))?)
        } else {
            None
        };
        let mut report = existing
            .as_deref()
            .map(|text| crate::config::decl::parse(text, "local receipt"))
            .transpose()?
            .unwrap_or_else(|| {
                let mut report = toml::Table::new();
                report.insert("version".into(), toml::Value::Integer(1));
                report.insert("historical".into(), toml::Value::Boolean(true));
                for (key, value) in [
                    ("id", id.as_str()),
                    ("data", "saved"),
                    ("commit", "skipped"),
                    ("push", "unknown"),
                    ("verb", "workflow"),
                    ("message", ""),
                ] {
                    report.insert(key.into(), toml::Value::String(value.into()));
                }
                report
            });
        if report.contains_key("batch") {
            continue;
        }
        // Serialize imports with every ordinary writer before re-reading its latest report.
        let lock_path = local.join("write.lock");
        fs::create_dir_all(local).map_err(|e| io_error(local, e))?;
        let lock = if !locked {
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(&lock_path)
                .map_err(|e| io_error(&lock_path, e))?;
            file.try_lock().map_err(|e| {
                Error::conflict(format!("receipt migration needs an idle writer: {e}"))
            })?;
            Some(file)
        } else {
            None
        };
        if local.join("undo").exists() {
            return Ok(());
        }
        if target.exists() {
            report = crate::config::decl::parse(
                &fs::read_to_string(&target).map_err(|e| io_error(&target, e))?,
                "local receipt",
            )?;
        }
        if !report.contains_key("batch") {
            report.insert("data".into(), toml::Value::String("saved".into()));
            report.insert("batch".into(), toml::Value::Table(batch));
            atomic_write(&target, report.to_string().as_bytes())?;
        }
        drop(lock);
    }
    Ok(())
}

/// Held through the entire read/modify/write and Git sequence.
pub struct Transaction {
    journal: Rc<RefCell<Journal>>,
    _lock: WriterLock,
    accepted: bool,
}

impl Transaction {
    pub fn begin(workspace: &Workspace) -> Result<Self> {
        Self::with_lock(WriterLock::acquire(workspace)?)
    }

    pub fn with_lock(lock: WriterLock) -> Result<Self> {
        let dir = lock.local.join("undo");
        fs::create_dir(&dir).map_err(|e| io_error(&dir, e))?;
        let journal = Rc::new(RefCell::new(Journal {
            root: lock.root.clone(),
            dir,
            entries: BTreeMap::new(),
            initial_clean: std::process::Command::new("git")
                .arg("-C")
                .arg(&lock.root)
                .args(["status", "--porcelain", "--untracked-files=all"])
                .output()
                .is_ok_and(|out| out.status.success() && out.stdout.is_empty()),
        }));
        ACTIVE.with(|active| *active.borrow_mut() = Some(journal.clone()));
        Ok(Self {
            journal,
            _lock: lock,
            accepted: false,
        })
    }

    /// Accept local data, but keep the writer lock until this guard is dropped.
    pub fn accept(&mut self) -> Result<()> {
        self.accept_with_receipt(None)
    }

    pub(crate) fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub(crate) fn accept_with_receipt(&mut self, receipt: Option<(&str, &str)>) -> Result<()> {
        let journal = self.journal.borrow();
        if let Some((id, body)) = receipt {
            let name = format!("{id}.toml");
            if !safe_receipt_name(&name) {
                return Err(Error::usage("invalid operation receipt id"));
            }
            let table: toml::Table = [
                ("name".into(), toml::Value::String(name)),
                ("body".into(), toml::Value::String(body.into())),
            ]
            .into_iter()
            .collect();
            durable_write(
                &journal.dir.join("receipt.toml"),
                table.to_string().as_bytes(),
            )?;
        }
        atomic_write(&journal.dir.join("accepted"), b"accepted\n")?;
        self.accepted = true;
        ACTIVE.with(|active| *active.borrow_mut() = None);
        publish_receipt(&journal.dir)?;
        // Once accepted, cleanup failure must not cause a later rollback.
        fs::remove_dir_all(&journal.dir).map_err(|e| io_error(&journal.dir, e))
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        ACTIVE.with(|active| *active.borrow_mut() = None);
        if !self.accepted {
            let journal = self.journal.borrow();
            if let Err(error) = recover(&journal.root, &journal.dir) {
                crate::git::operation::change(|report| {
                    report.data = crate::git::operation::Data::Unknown
                });
                eprintln!(
                    "clerk: rollback incomplete: {error}; the next write will retry recovery"
                );
            }
        }
    }
}

/// Resolve a mutation path without permitting parent traversal or symlink escapes.
fn checked(root: &Path, path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::io(e.to_string()))?
            .join(path)
    };
    if absolute
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(Error::usage(
            "write paths must not contain parent traversal",
        ));
    }
    for ancestor in absolute.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(Error::usage("write paths must not contain symbolic links"));
        }
    }
    // Canonicalize the existing prefix as well as its Windows drive spelling;
    // newly created files do not themselves have a canonical path yet.
    let mut prefix = absolute.as_path();
    let mut tail = Vec::new();
    let absolute = loop {
        match prefix.canonicalize() {
            Ok(mut canonical) => {
                for part in tail.iter().rev() {
                    canonical.push(part);
                }
                break canonical;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tail.push(
                    prefix
                        .file_name()
                        .ok_or_else(|| Error::usage("invalid write path"))?
                        .to_os_string(),
                );
                prefix = prefix
                    .parent()
                    .ok_or_else(|| Error::usage("invalid write path"))?;
            }
            Err(error) => return Err(io_error(prefix, error)),
        }
    };
    let relative = absolute.strip_prefix(root).map_err(|_| {
        Error::usage(format!(
            "write path {} is outside the workspace",
            path.display()
        ))
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(name) if !name.to_string_lossy().eq_ignore_ascii_case(".git") => {
                current.push(name)
            }
            _ => {
                return Err(Error::usage(
                    "write paths must stay below the workspace and outside .git",
                ));
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::usage(format!(
                    "write path {} contains a symbolic link",
                    path.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&current, error)),
        }
    }
    Ok(current)
}

fn remember(path: &Path) -> Result<()> {
    ACTIVE.with(|active| {
        let active = active.borrow();
        let Some(journal) = active.as_ref() else {
            return Ok(());
        };
        let mut journal = journal.borrow_mut();
        let path = checked(&journal.root, path)?;
        if path.starts_with(journal.dir.parent().expect("metadata directory")) {
            return Err(Error::usage(
                "managed writes must not change transaction metadata",
            ));
        }
        let relative = path
            .strip_prefix(&journal.root)
            .expect("checked path")
            .to_string_lossy()
            .replace('\\', "/");
        if journal.entries.contains_key(&relative) {
            return Ok(());
        }
        let backup = match fs::read(&path) {
            Ok(bytes) => {
                let name = format!("{}.bin", journal.entries.len());
                durable_write(&journal.dir.join(&name), &bytes)?;
                Some(name)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error(&path, error)),
        };
        journal.entries.insert(relative, backup);
        let entries: toml::Table = journal
            .entries
            .iter()
            .map(|(path, backup)| {
                (
                    path.clone(),
                    toml::Value::String(backup.clone().unwrap_or_default()),
                )
            })
            .collect();
        let body = toml::to_string(&entries).map_err(|e| Error::io(e.to_string()))?;
        atomic_write(&journal.dir.join("manifest.toml"), body.as_bytes())
    })
}

fn recover(root: &Path, dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    if dir.join("accepted").exists() {
        publish_receipt(dir)?;
    } else if dir.join("manifest.toml").exists() {
        let manifest = dir.join("manifest.toml");
        let text = fs::read_to_string(&manifest).map_err(|e| io_error(&manifest, e))?;
        let entries: toml::Table =
            toml::from_str(&text).map_err(|e| Error::io(format!("invalid undo manifest: {e}")))?;
        for (relative, backup) in entries {
            let path = checked(root, &root.join(relative))?;
            let backup = backup
                .as_str()
                .ok_or_else(|| Error::io("invalid undo entry"))?;
            if backup.is_empty() {
                match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(io_error(&path, e)),
                }
            } else {
                if Path::new(backup).components().count() != 1 || !backup.ends_with(".bin") {
                    return Err(Error::io("invalid undo backup name"));
                }
                let source = dir.join(backup);
                let bytes = fs::read(&source).map_err(|e| io_error(&source, e))?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
                }
                durable_write(&path, &bytes)?;
            }
        }
    }
    fs::remove_dir_all(dir).map_err(|e| io_error(dir, e))
}

fn safe_receipt_name(name: &str) -> bool {
    name.strip_suffix(".toml").is_some_and(|id| {
        !id.is_empty()
            && id.len() <= 100
            && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    })
}

/// The accepted journal survives until its recovery receipt is published durably.
fn publish_receipt(dir: &Path) -> Result<()> {
    let source = dir.join("receipt.toml");
    if !source.exists() {
        return Ok(());
    }
    let table = crate::config::decl::parse(
        &fs::read_to_string(&source).map_err(|e| io_error(&source, e))?,
        "accepted receipt",
    )?;
    let name = crate::config::decl::required_text(&table, "name", "accepted receipt")?;
    if !safe_receipt_name(&name) {
        return Err(Error::usage("invalid accepted receipt filename"));
    }
    let body = crate::config::decl::required_text(&table, "body", "accepted receipt")?;
    atomic_write(
        &dir.parent()
            .expect("journal parent")
            .join("operations")
            .join(name),
        body.as_bytes(),
    )
}

fn durable_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(path).map_err(|e| io_error(path, e))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| io_error(path, e))
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::usage("write path has no parent"))?;
    fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    let temporary = parent.join(format!(".clerk-{}-write.tmp", std::process::id()));
    let result = durable_write(&temporary, bytes)
        .and_then(|()| fs::rename(&temporary, path).map_err(|e| io_error(path, e)));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    remember(path)?;
    // Managed writes already have a durable before-image. Avoid temporary
    // files in managed directories, including after a process interruption.
    if ACTIVE.with(|active| active.borrow().is_some()) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
        }
        durable_write(path, bytes.as_ref())
    } else {
        atomic_write(path, bytes.as_ref())
    }
}

pub fn remove(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    remember(path)?;
    fs::remove_file(path).map_err(|e| io_error(path, e))
}

pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    if to.exists() {
        return Err(Error::conflict(format!("{} already exists", to.display())));
    }
    remember(from)?;
    remember(to)?;
    fs::rename(from, to).map_err(|e| io_error(to, e))
}

pub fn append(path: &Path, text: &str) -> Result<()> {
    let mut bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(io_error(path, e)),
    };
    bytes.extend_from_slice(text.as_bytes());
    write(path, bytes)
}

/// Directory creation checks the same boundary before making any filesystem change.
pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    ACTIVE.with(|active| {
        if let Some(journal) = active.borrow().as_ref() {
            checked(&journal.borrow().root, path)?;
        }
        Ok::<(), Error>(())
    })?;
    fs::create_dir_all(path).map_err(|e| io_error(path, e))
}

/// An initially clean transaction may account for its own pending edits.
pub(crate) fn owns_changes(paths: &[String]) -> bool {
    ACTIVE.with(|active| {
        active.borrow().as_ref().is_some_and(|journal| {
            let journal = journal.borrow();
            journal.initial_clean && paths.iter().all(|path| journal.entries.contains_key(path))
        })
    })
}
