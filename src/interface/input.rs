//! Body text for the verbs that write prose into a file.
//!
//! Three sources, in order: `--body`, `--file <path>` or `--file -`, and when
//! neither is given, the standard input or the editor. Reading from a pipe is
//! only attempted for verbs that take a body, so a listing command never
//! steals the caller's input.

use std::io::IsTerminal;

use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};

/// Body text for one command, if any was supplied.
pub fn body(
    workspace: &Workspace,
    args: &crate::interface::cli::Args,
    interactive: bool,
) -> Result<Option<String>> {
    if let Some(text) = args.value("body") {
        return Ok(Some(text.to_string()));
    }
    match args.value("file") {
        Some("-") => read_stdin().map(Some),
        Some(source) => {
            let text = std::fs::read_to_string(source).map_err(|error| {
                crate::foundation::error::io_error(std::path::Path::new(source), error)
            })?;
            Ok(Some(text))
        }
        None if !interactive => Ok(None),
        None => {
            if std::io::stdin().is_terminal() {
                edit(workspace).map(Some)
            } else {
                read_stdin().map(Some)
            }
        }
    }
}

fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)
        .map_err(|error| Error::io(format!("cannot read standard input: {error}")))?;
    Ok(buffer)
}

/// Open the editor on a scratch file and return what was written.
fn edit(workspace: &Workspace) -> Result<String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_default();
    if editor.is_empty() {
        return Err(Error::usage(
            "no --body, no --file and no editor; set VISUAL or EDITOR, or pipe the text",
        ));
    }
    let path = workspace.root().join(".work").join("body.md");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| crate::foundation::error::io_error(parent, error))?;
    }
    std::fs::write(&path, "").map_err(|error| crate::foundation::error::io_error(&path, error))?;
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|error| Error::io(format!("cannot run {editor}: {error}")))?;
    if !status.success() {
        return Err(Error::io(format!("{editor} exited with {status}")));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| crate::foundation::error::io_error(&path, error))?;
    let _ = std::fs::remove_file(&path);
    Ok(text)
}

/// Lines from a list file, accepting `-` for standard input.
pub fn lines(args: &crate::interface::cli::Args, flag: &str) -> Result<Vec<String>> {
    let Some(source) = args.value(flag) else {
        return Ok(Vec::new());
    };
    let text = if source == "-" {
        read_stdin()?
    } else {
        std::fs::read_to_string(source).map_err(|error| {
            crate::foundation::error::io_error(std::path::Path::new(source), error)
        })?
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}
