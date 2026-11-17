//! Shared workspace diagnostics, independent of presentation.

use crate::execution::context::Context;
use crate::foundation::error::Result;

pub fn problems(context: &Context) -> Result<Vec<(String, usize, String)>> {
    let mut problems: Vec<(String, usize, String)> = Vec::new();
    for kind in context.catalog.kinds() {
        for message in lifecycle(context, kind)? {
            problems.push((kind.source.clone(), 0, message));
        }
        if kind.actions.iter().any(|action| action.handler == "append") {
            let mut ids = std::collections::BTreeSet::new();
            let mut paths = crate::storage::records::markdown_files(
                &context
                    .workspace
                    .resolve(kind.document_dir().unwrap_or_default()),
            )?;
            paths.extend(crate::storage::records::markdown_files(
                &context.workspace.resolve(&kind.archive_dir()),
            )?);
            paths.sort();
            paths.dedup();
            for path in paths {
                let body = std::fs::read_to_string(&path)
                    .map_err(|e| crate::foundation::error::io_error(&path, e))?;
                if let Err(error) =
                    crate::collaboration::log_entry::validate_page(context, kind, &body)
                {
                    problems.push((context.workspace.relative(&path), 0, error.to_string()));
                }
                if let Ok(entries) = crate::collaboration::log_entry::parse(&body) {
                    for id in entries.into_iter().filter_map(|entry| entry.id) {
                        if !ids.insert(id.clone()) {
                            problems.push((
                                context.workspace.relative(&path),
                                0,
                                format!("duplicate log entry id `{id}`"),
                            ));
                        }
                    }
                }
            }
        }
        for problem in crate::execution::validate::problems(context, kind)? {
            problems.push((problem.at, problem.line as usize, problem.message));
        }
    }

    // I9: markdown under a validated path keeps its links and tables. Pages
    // that belong to a document kind are already checked there.
    let covered: Vec<std::path::PathBuf> = context
        .catalog
        .kinds()
        .iter()
        .filter_map(|kind| {
            kind.document_dir()
                .map(|dir| context.workspace.resolve(dir))
        })
        .collect();
    for rule in &context.workspace.paths {
        if rule.level != crate::config::workspace::Level::Validated || !rule.regions.is_empty() {
            continue;
        }
        let target = context.workspace.resolve(&rule.path);
        let files = if target.is_dir() {
            crate::storage::records::markdown_files(&target)?
        } else if target.extension().map(|ext| ext == "md").unwrap_or(false) {
            vec![target]
        } else {
            Vec::new()
        };
        for file in files {
            if covered.iter().any(|dir| file.starts_with(dir)) {
                continue;
            }
            let body = std::fs::read_to_string(&file)
                .map_err(|error| crate::foundation::error::io_error(&file, error))?;
            let relative = context.workspace.relative(&file);
            for link in crate::storage::document::broken_links(&body, file.parent()) {
                problems.push((
                    relative.clone(),
                    0,
                    format!("link `{link}` does not resolve"),
                ));
            }
            for (line, message) in crate::storage::document::table_problems(&body) {
                problems.push((relative.clone(), line, message));
            }
        }
    }
    // I1: enum and state registries are domains; ref fields may use ledgers.
    let mut referenced: Vec<String> = Vec::new();
    for kind in context.catalog.kinds() {
        for field in &kind.fields {
            if field.kind == crate::config::kind::FieldKind::Enum
                && let Some(name) = &field.registry
            {
                referenced.push(name.clone());
            }
        }
        if let Some(state) = &kind.state {
            referenced.push(state.registry.clone());
        }
        // Views may read a plain list or ledger, so their sources only have to
        // exist; only enum fields and states require a domain with display.
    }
    referenced.sort();
    referenced.dedup();
    for name in &referenced {
        match context.registry(name) {
            Ok(registry) if !registry.is_domain() => problems.push((
                format!("registry `{name}`"),
                0,
                "missing `display` column".to_string(),
            )),
            Ok(_) => {}
            Err(error) => problems.push((format!("registry `{name}`"), 0, error.to_string())),
        }
    }

    // A view source only has to exist, whether it is a domain or a plain list.
    for kind in context.catalog.kinds() {
        for view in &kind.views {
            for entry in &view.entries {
                if let Some(name) = &entry.registry
                    && let Err(error) = context.registry(name)
                {
                    problems.push((format!("registry `{name}`"), 0, error.to_string()));
                }
            }
            if view.renderer.as_deref() == Some("registry")
                && let Some(name) = &view.source
                && let Err(error) = context.registry(name)
            {
                problems.push((format!("registry `{name}`"), 0, error.to_string()));
            }
        }
    }
    // I5: every kind path sits under a declared managed path.
    for kind in context.catalog.kinds() {
        let relative = match kind.document_dir() {
            Some(dir) => dir.to_string(),
            None => kind.table_path().to_string(),
        };
        if !context.workspace.covers(&relative) {
            problems.push((
                kind.source.clone(),
                0,
                format!("path `{relative}` is not covered by a [[path]] rule"),
            ));
        }
    }

    // I6: every data table belongs to a kind or to a built in operation.
    // Every data table must belong to a kind; the batch list lives with the
    // other lists under the registry directory, so it is not a data table.
    let mut allowed: Vec<String> = Vec::new();
    for kind in context.catalog.kinds() {
        if kind.document_dir().is_none() {
            allowed.push(kind.table_path().to_string());
            allowed.push(kind.archive_table());
            if let Some(table) = &kind.relations_table {
                allowed.push(table.clone());
            }
        }
    }
    let data_dir = context.workspace.resolve(&context.workspace.layout.data);
    if data_dir.is_dir() {
        let mut names: Vec<String> = std::fs::read_dir(&data_dir)
            .map_err(|error| crate::foundation::error::io_error(&data_dir, error))?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        for name in names {
            let relative = format!("{}/{name}", context.workspace.layout.data);
            if !allowed.iter().any(|item| item == &relative) {
                problems.push((relative, 0, "data file has no declaration".to_string()));
            }
        }
    }

    // I4: an alias must point at a key that exists.
    let aliases = context.workspace.aliases()?;
    if !aliases.is_empty() {
        let mut keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for kind in context.catalog.kinds() {
            if kind.document_dir().is_some() {
                continue;
            }
            let store = crate::storage::store::Store::open(&context.workspace, kind)?;
            for index in 0..store.table.rows.len() {
                let record = store.table.record(index);
                if let Some(first) = kind.key.first()
                    && let Some(value) = record.get(first)
                {
                    keys.insert(value.trim().to_string());
                }
            }
            let archive = context.workspace.resolve(&kind.archive_table());
            if archive.exists() {
                let table = crate::storage::store::Table::load(&archive)?;
                for index in 0..table.rows.len() {
                    let record = table.record(index);
                    if let Some(first) = kind.key.first()
                        && let Some(value) = record.get(first)
                    {
                        keys.insert(value.trim().to_string());
                    }
                }
            }
        }
        for (alias, key) in &aliases {
            if !keys.contains(key) {
                problems.push((
                    "alias.csv".to_string(),
                    0,
                    format!("alias `{alias}` points at unknown key `{key}`"),
                ));
            }
        }
    }
    let seals = crate::storage::seal::Seals::load(&context.workspace)?;
    for problem in seals.verify(&context.workspace) {
        problems.push((problem.key, 0, problem.message));
    }
    for (file, message) in crate::execution::render::missing_regions(context) {
        problems.push((file, 0, message));
    }
    for (file, message) in crate::execution::render::generated_drift(context)? {
        problems.push((file, 0, message));
    }

    Ok(problems)
}

/// Optional terminal metadata turns transition declarations into a checked graph.
fn lifecycle(context: &Context, kind: &crate::config::kind::Kind) -> Result<Vec<String>> {
    use std::collections::BTreeSet;
    let Some(state) = &kind.state else {
        return Ok(Vec::new());
    };
    if kind.transitions.is_empty() {
        return Ok(Vec::new());
    }
    let registry = context.registry(&state.registry)?;
    if !registry
        .entries()
        .iter()
        .any(|entry| entry.cells.contains_key("terminal"))
    {
        return Ok(Vec::new());
    }
    let mut messages = Vec::new();
    let mut reachable = BTreeSet::new();
    for entry in registry.entries() {
        match entry.cells.get("terminal").map(String::as_str) {
            Some("true") => {
                reachable.insert(entry.key.clone());
            }
            Some("false") => {}
            _ => messages.push(format!(
                "state `{}` needs terminal=true or false",
                entry.key
            )),
        }
    }
    let mut used = BTreeSet::new();
    if let Some(default) = &state.default {
        used.insert(default.clone());
    }
    for transition in &kind.transitions {
        used.insert(transition.to.clone());
        used.extend(transition.from.iter().cloned());
    }
    for record in crate::storage::records::including_archived(context, kind)? {
        if let Some(value) = record.get(&state.field)
            && !value.is_empty()
        {
            used.insert(value.clone());
        }
    }
    loop {
        let before = reachable.len();
        for transition in &kind.transitions {
            if reachable.contains(&transition.to) {
                reachable.extend(transition.from.iter().cloned());
            }
        }
        if before == reachable.len() {
            break;
        }
    }
    for node in used.difference(&reachable) {
        messages.push(format!(
            "state `{node}` cannot reach a terminal state; add a recovery transition"
        ));
    }
    Ok(messages)
}
