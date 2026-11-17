//! One span-preserving parser and conservative Git-history repair planner.

use crate::config::schema::{object, quote};
use crate::config::workspace::normalize;
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result, io_error};
use crate::interface::cli::Args;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct Reference {
    pub start: usize,
    pub end: usize,
    pub target: String,
    pub inline: bool,
}

fn local(target: &str) -> bool {
    !target.is_empty()
        && !target.starts_with(['#', '?'])
        && !target.contains("://")
        && !target.starts_with("//")
        && !target.starts_with("mailto:")
        && !target.starts_with("data:")
}

fn destination(line: &str, mut at: usize) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    if bytes.get(at) == Some(&b'<') {
        let end = line[at + 1..].find('>')? + at + 1;
        return Some((at + 1, end));
    }
    let start = at;
    let mut depth = 0;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'\\' if at + 1 < bytes.len() => {
                at += 2;
                continue;
            }
            b'(' => depth += 1,
            b')' if depth == 0 => break,
            b')' => depth -= 1,
            b if b.is_ascii_whitespace() && depth == 0 => break,
            _ => {}
        }
        at += 1;
    }
    (at > start).then_some((start, at))
}

/// Code fences, comments and code spans never contribute Markdown links.
pub fn parse(body: &str, inline: bool) -> Vec<Reference> {
    let mut out = Vec::new();
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    let mut comment = false;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let bytes = trimmed.as_bytes();
        let marker = bytes.first().copied().unwrap_or(0);
        let width = bytes.iter().take_while(|b| **b == marker).count();
        if matches!(marker, b'`' | b'~') && width >= 3 {
            if let Some((active, n)) = fence {
                if active == marker && width >= n && trimmed[width..].trim().is_empty() {
                    fence = None;
                }
            } else {
                fence = Some((marker, width));
            }
            offset += line.len();
            continue;
        }
        if fence.is_some() || line.starts_with("    ") || line.starts_with('\t') {
            offset += line.len();
            continue;
        }
        let mut at = 0;
        let bytes = line.as_bytes();
        while at < line.len() {
            if comment {
                if let Some(end) = line[at..].find("-->") {
                    at += end + 3;
                    comment = false;
                } else {
                    break;
                }
            } else if line[at..].starts_with("<!--") {
                at += 4;
                comment = true;
            } else if bytes[at] == b'\\' {
                at += 1;
                at += line[at..].chars().next().map(char::len_utf8).unwrap_or(0);
            } else if bytes[at] == b'`' {
                let count = bytes[at..].iter().take_while(|b| **b == b'`').count();
                let delimiter = "`".repeat(count);
                if let Some(end) = line[at + count..].find(&delimiter) {
                    let end = at + count + end;
                    let target = &line[at + count..end];
                    if inline
                        && local(target)
                        && !target.chars().any(char::is_whitespace)
                        && !target.contains(['*', '{', '}', '[', ']'])
                        && !target.contains("...")
                        && Path::new(split(target).0).extension().is_some()
                        && (target.contains(['/', '\\', '.']))
                    {
                        out.push(Reference {
                            start: offset + at,
                            end: offset + end + count,
                            target: target.into(),
                            inline: true,
                        });
                    }
                    at = end + count;
                } else {
                    at += count;
                }
            } else if bytes[at] == b'[' {
                // Consume the entire label, including inline code, before its destination.
                let mut end = at + 1;
                let mut depth = 1;
                while end < line.len() && depth > 0 {
                    if bytes[end] == b'\\' {
                        end += 2;
                        continue;
                    }
                    if bytes[end] == b'[' {
                        depth += 1;
                    }
                    if bytes[end] == b']' {
                        depth -= 1;
                    }
                    end += 1;
                }
                let definition =
                    at == line.len() - line.trim_start().len() && bytes.get(end) == Some(&b':');
                if (bytes.get(end) == Some(&b'(') || definition)
                    && let Some((start, finish)) = destination(line, end + 1)
                {
                    let target = &line[start..finish];
                    if local(target) {
                        out.push(Reference {
                            start: offset + start,
                            end: offset + finish,
                            target: target.into(),
                            inline: false,
                        });
                    }
                    at = finish;
                    continue;
                }
                at = end.min(line.len());
            } else {
                at += line[at..].chars().next().map(char::len_utf8).unwrap_or(1);
            }
        }
        offset += line.len();
    }
    out
}

/// Keep editor line suffixes and Markdown fragments intact when relocating a file.
fn split(target: &str) -> (&str, &str) {
    let mut end = target.find(['#', '?']).unwrap_or(target.len());
    if let Some(at) = target[..end].rfind(':') {
        let tail = &target[at + 1..end];
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            end = at;
        }
    }
    (&target[..end], &target[end..])
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && at + 2 < bytes.len()
            && let Ok(n) = u8::from_str_radix(
                std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap_or(""),
                16,
            )
        {
            out.push(n);
            at += 3;
            continue;
        }
        if bytes[at] == b'\\' && bytes.get(at + 1).is_some_and(|b| b"()<> ".contains(b)) {
            at += 1;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).replace('\\', "/")
}

fn resolve(doc: &Path, target: &str) -> PathBuf {
    normalize(
        &doc.parent()
            .unwrap_or(Path::new("."))
            .join(decode(split(target).0)),
    )
}

pub fn broken(body: &str, directory: Option<&Path>) -> Vec<String> {
    let doc = directory.unwrap_or(Path::new(".")).join("document.md");
    parse(body, false)
        .into_iter()
        .filter(|reference| !resolve(&doc, &reference.target).exists())
        .map(|r| r.target)
        .collect()
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| Error::io(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::usage(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

type Renames = Vec<(PathBuf, PathBuf)>;
fn renames(root: &Path) -> Result<Renames> {
    let mut bytes = git(
        root,
        &[
            "log",
            "HEAD",
            "--format=",
            "--name-status",
            "-z",
            "--diff-filter=R",
            "-M",
        ],
    )?;
    bytes.extend(git(
        root,
        &[
            "diff",
            "HEAD",
            "--name-status",
            "-z",
            "--diff-filter=R",
            "-M",
        ],
    )?);
    let parts = bytes
        .split(|b| *b == 0)
        .map(|b| {
            String::from_utf8_lossy(b)
                .trim_start_matches('\n')
                .to_string()
        })
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut at = 0;
    while at < parts.len() {
        if parts[at].starts_with('R') && at + 2 < parts.len() {
            out.push((
                normalize(&root.join(&parts[at + 1])),
                normalize(&root.join(&parts[at + 2])),
            ));
            at += 3;
        } else {
            at += 1;
        }
    }
    Ok(out)
}

fn destinations(path: &Path, edges: &Renames) -> (BTreeSet<PathBuf>, bool) {
    let mut stack = vec![path.to_path_buf()];
    let mut seen = BTreeSet::new();
    let mut found = BTreeSet::new();
    let mut ambiguous = false;
    while let Some(path) = stack.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let next = edges
            .iter()
            .filter(|(old, _)| old == &path)
            .map(|(_, new)| new.clone())
            .collect::<BTreeSet<_>>();
        if next.len() > 1 {
            ambiguous = true;
        }
        if path.exists() {
            found.insert(path);
        }
        stack.extend(next);
    }
    (found, ambiguous)
}

fn relative(from: &Path, to: &Path) -> Option<String> {
    let a = from.components().collect::<Vec<_>>();
    let b = to.components().collect::<Vec<_>>();
    let common = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return None;
    }
    let mut parts = vec!["..".to_string(); a.len() - common];
    parts.extend(
        b[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    Some(parts.join("/"))
}

fn collect(directory: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(directory).map_err(|e| io_error(directory, e))? {
        let entry = entry.map_err(|e| io_error(directory, e))?;
        let ty = entry.file_type().map_err(|e| io_error(&entry.path(), e))?;
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            if [".git", "target", ".work", "node_modules"]
                .contains(&entry.file_name().to_string_lossy().as_ref())
                || entry.path().join(".git").exists()
            {
                continue;
            }
            collect(&entry.path(), out)?;
        } else if entry.path().extension().is_some_and(|ext| ext == "md") {
            out.push(entry.path());
        }
    }
    Ok(())
}

fn files(context: &Context, args: &Args) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if args.positional.len() > 2 {
        for item in &args.positional[2..] {
            let path = normalize(&context.workspace.resolve(item));
            if path.is_dir() {
                collect(&path, &mut files)?;
            } else if path.is_file() {
                files.push(path);
            } else {
                return Err(Error::usage(format!(
                    "document path `{item}` does not exist"
                )));
            }
        }
    } else if let Ok(paths) = git(context.workspace.root(), &["ls-files", "-z"]) {
        for name in paths.split(|b| *b == 0).filter(|b| !b.is_empty()) {
            let path = context.workspace.resolve(&String::from_utf8_lossy(name));
            if path.extension().is_some_and(|ext| ext == "md")
                && path.is_file()
                && !std::fs::symlink_metadata(&path)
                    .map_err(|e| io_error(&path, e))?
                    .file_type()
                    .is_symlink()
            {
                files.push(path);
            }
        }
    } else {
        for rule in &context.workspace.paths {
            let path = context.workspace.resolve(&rule.path);
            if path.is_dir() {
                collect(&path, &mut files)?;
            } else if path.extension().is_some_and(|e| e == "md") && path.exists() {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// Shared Markdown output for migrated prose and generated path cells.
pub fn markdown_link(label: &str, target: &str) -> String {
    let label = label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('|', "\\|");
    let target = target
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('|', "%7C");
    format!("[{label}](<{target}>)")
}

/// Paths stay raw in storage and machine output. Only rendered documents get links.
pub fn path_cell(context: &Context, target: &Path, raw: &str) -> String {
    raw.split('|')
        .map(|item| {
            let item = item.trim();
            let path = normalize(&context.workspace.resolve(split(item).0));
            if !item.is_empty()
                && path.exists()
                && let Some(relative) =
                    relative(target.parent().unwrap_or(context.workspace.root()), &path)
            {
                return markdown_link(item, &(relative + split(item).1));
            }
            item.to_string()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

struct Change {
    reference: Reference,
    line: usize,
    status: &'static str,
    replacement: Option<String>,
    candidates: Vec<String>,
}

fn plan(
    doc: &Path,
    body: &str,
    edges: &Renames,
    linkify: bool,
    repo: Option<&Path>,
) -> Vec<Change> {
    let mut changes = Vec::new();
    for reference in parse(body, linkify) {
        let mut path = resolve(doc, &reference.target);
        if reference.inline
            && reference.target.contains(['/', '\\'])
            && !path.exists()
            && let Some(repo) = repo
        {
            let from_root = normalize(&repo.join(decode(split(&reference.target).0)));
            if from_root.exists() {
                path = from_root;
            }
        }
        let exists = path.exists();
        let mut candidates = BTreeSet::new();
        let mut ambiguous = false;
        if !exists {
            if reference.inline
                && reference.target.contains(['/', '\\'])
                && let Some(repo) = repo
            {
                let root_path = normalize(&repo.join(decode(split(&reference.target).0)));
                let (found, branching) = destinations(&root_path, edges);
                candidates.extend(found);
                ambiguous |= branching;
            }
            let (found, branching) = destinations(&path, edges);
            candidates.extend(found);
            ambiguous |= branching;
            // Moving a document also changes the meaning of unchanged relative links.
            let mut old_docs = vec![doc.to_path_buf()];
            let mut seen = BTreeSet::new();
            while let Some(current) = old_docs.pop() {
                if !seen.insert(current.clone()) {
                    continue;
                }
                for (old, new) in edges.iter().filter(|(_, new)| new == &current) {
                    let old_target = resolve(old, &reference.target);
                    let (found, branching) = destinations(&old_target, edges);
                    candidates.extend(found);
                    ambiguous |= branching;
                    let _ = new;
                    old_docs.push(old.clone());
                }
            }
        }
        let destination = if exists {
            Some(path)
        } else if !ambiguous && candidates.len() == 1 {
            candidates.iter().next().cloned()
        } else {
            None
        };
        let replacement = destination
            .as_ref()
            .and_then(|to| relative(doc.parent()?, to))
            .and_then(|target| {
                let target = target + split(&reference.target).1;
                if reference.inline {
                    Some(markdown_link(&reference.target, &target))
                } else if !exists {
                    Some(
                        target
                            .replace(' ', "%20")
                            .replace('(', "%28")
                            .replace(')', "%29"),
                    )
                } else {
                    None
                }
            });
        // Do not report ordinary inline identifiers which were never filesystem paths.
        if reference.inline && !exists && candidates.is_empty() {
            continue;
        }
        let status = if exists {
            if reference.inline { "linkify" } else { "ok" }
        } else if replacement.is_some() {
            "rename"
        } else if ambiguous || candidates.len() > 1 {
            "ambiguous"
        } else {
            "missing"
        };
        changes.push(Change {
            line: body[..reference.start]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                + 1,
            reference,
            status,
            replacement,
            candidates: candidates
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        });
    }
    changes
}

pub fn command(root: Option<&str>, args: &Args) -> Result<i32> {
    let mut context = Context::open(root)?;
    let verb = args
        .positional
        .get(1)
        .map(String::as_str)
        .unwrap_or("check");
    if args.switch("apply") && verb != "repair" {
        return Err(Error::usage("--apply requires links repair"));
    }
    if args.switch("apply") && args.switch("dry-run") {
        return Err(Error::usage("--apply and --dry-run cannot be combined"));
    }
    if args.value("request").is_some() {
        return Err(Error::usage("reference repair does not support --request"));
    }
    let baseline = if args.switch("apply") {
        Some(crate::execution::pipeline::begin(&mut context)?)
    } else {
        None
    };
    let _snapshot = if baseline.is_none() {
        Some(crate::execution::transaction::WriterLock::snapshot(
            &context.workspace,
        )?)
    } else {
        None
    };
    let files = files(&context, args)?;
    let mut histories = BTreeMap::new();
    let mut output = Vec::new();
    let mut rows = Vec::new();
    let mut updates = Vec::new();
    let mut unresolved = 0;
    for file in files {
        let body = std::fs::read_to_string(&file).map_err(|e| io_error(&file, e))?;
        let directory = file.parent().unwrap_or(context.workspace.root());
        let repo = git(directory, &["rev-parse", "--show-toplevel"])
            .ok()
            .map(|bytes| PathBuf::from(String::from_utf8_lossy(&bytes).trim()));
        let edges = if verb != "check" {
            if let Some(repo) = &repo {
                if !histories.contains_key(repo) {
                    // An unborn repository has no rename history.
                    let history = if git(repo, &["rev-parse", "--verify", "HEAD"]).is_ok() {
                        renames(repo)?
                    } else {
                        Vec::new()
                    };
                    histories.insert(repo.clone(), history);
                }
                histories.get(repo).cloned().unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let mut changes = plan(
            &file,
            &body,
            &edges,
            args.switch("linkify") && verb != "check",
            repo.as_deref(),
        );
        let relative_path = context.workspace.relative(&file);
        let generated = context.workspace.paths.iter().any(|rule| {
            rule.level == crate::config::workspace::Level::Generated
                && rule.regions.is_empty()
                && (relative_path == rule.path
                    || relative_path.starts_with(&format!("{}/", rule.path)))
        });
        let mut ranges = Vec::new();
        let mut cursor = 0;
        while let Some(start) = body[cursor..].find("<!-- clerk:begin ") {
            let start = cursor + start;
            let Some(end) = body[start..].find("<!-- clerk:end ") else {
                ranges.push(start..body.len());
                break;
            };
            let end = start + end;
            ranges.push(start..end);
            cursor = end + 1;
        }
        changes.retain(|change| {
            !change.reference.inline
                || !(generated
                    || ranges
                        .iter()
                        .any(|range| range.contains(&change.reference.start)))
        });
        for change in &mut changes {
            if change.replacement.is_some()
                && (generated
                    || ranges
                        .iter()
                        .any(|range| range.contains(&change.reference.start)))
            {
                change.replacement = None;
                change.status = "generated";
            }
        }
        let mut result = body.clone();
        for change in changes.iter().rev() {
            if let Some(replacement) = &change.replacement {
                result.replace_range(change.reference.start..change.reference.end, replacement);
            }
        }
        if result != body {
            if baseline.is_some() && !file.starts_with(context.workspace.root()) {
                return Err(Error::usage(
                    "repair can only write documents inside its workspace",
                ));
            }
            updates.push((file.clone(), result));
        }
        for change in changes {
            if matches!(change.status, "missing" | "ambiguous" | "generated") {
                unresolved += 1;
            }
            if change.status == "ok" && verb == "check" {
                continue;
            }
            let relative = context.workspace.relative(&file);
            rows.push(vec![
                relative.clone(),
                change.line.to_string(),
                change.status.into(),
                change.reference.target.clone(),
                change.replacement.clone().unwrap_or_default(),
            ]);
            output.push(object([
                ("path", quote(&relative)),
                ("line", change.line.to_string()),
                ("start", change.reference.start.to_string()),
                ("end", change.reference.end.to_string()),
                ("target", quote(&change.reference.target)),
                ("status", quote(change.status)),
                (
                    "replacement",
                    change
                        .replacement
                        .as_deref()
                        .map(quote)
                        .unwrap_or_else(|| "null".into()),
                ),
                (
                    "candidates",
                    crate::config::schema::strings_json(&change.candidates),
                ),
            ]));
        }
    }
    let mut affected = updates
        .iter()
        .map(|(path, _)| context.workspace.relative(path))
        .collect::<Vec<_>>();
    if let Some(baseline) = baseline {
        for (path, body) in &updates {
            crate::execution::transaction::write(path, body)?;
        }
        let notes = crate::execution::pipeline::finish_paths(
            &mut context,
            "links repair",
            &mut affected,
            &[],
            baseline,
        )?;
        for note in notes {
            eprintln!("{note}");
        }
    }
    if args.switch("json") {
        println!(
            "{}",
            object([
                ("version", "1".into()),
                ("applied", args.switch("apply").to_string()),
                ("files_changed", updates.len().to_string()),
                ("unresolved", unresolved.to_string()),
                ("references", format!("[{}]", output.join(",")))
            ])
        );
    } else {
        let headers = ["path", "line", "status", "target", "replacement"].map(String::from);
        print!(
            "{}",
            crate::terminal::text::table(
                Some(&headers),
                &rows,
                args.max_width(),
                args.switch("compact")
            )
        );
        println!(
            "{} file(s) {}; {unresolved} unresolved reference(s)",
            updates.len(),
            if args.switch("apply") {
                "updated"
            } else {
                "planned"
            }
        );
    }
    Ok(if unresolved > 0 { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_balanced_encoded_and_reference_links_without_code_or_comments() {
        let body = "[one](a(b).md#x) [two](<a b.md> \"title\")\n[ref]: old.md\n`[fake](no.md)`\n```md\n[fake](no.md)\n```\n<!-- [fake](no.md) -->\n";
        let refs = parse(body, false);
        assert_eq!(
            refs.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(),
            ["a(b).md#x", "a b.md", "old.md"]
        );
        assert_eq!(decode("a%20b%28x%29.md"), "a b(x).md");
        assert_eq!(split("old.md:165#section"), ("old.md", ":165#section"));
    }
}
