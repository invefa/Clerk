//! Commands that act on the workspace rather than on one kind.

use crate::execution::context::Context;
use crate::execution::engine;
use crate::foundation::error::{Error, Result};
use crate::interface::cli::Args;
use crate::terminal::text;

/// Show the resolved identity.
pub fn whoami(context: &mut Context, args: &Args) -> Result<()> {
    let identity = context.identity()?.clone();
    let person = identity.person.clone().unwrap_or_default();
    let operator = identity.operator.clone().unwrap_or_default();
    let headers = vec!["field".to_string(), "value".to_string()];
    let rows = vec![
        vec!["git name".to_string(), identity.git_name.clone()],
        vec!["git email".to_string(), identity.git_email.clone()],
        vec!["person".to_string(), person],
        vec!["operator".to_string(), operator],
        vec!["workspace".to_string(), context.workspace.name.clone()],
    ];
    if args.format() == crate::terminal::output::Format::Json {
        println!("{}", crate::terminal::output::rows_json(&headers, &rows));
        return Ok(());
    }
    crate::terminal::output::print_table(args, &headers, &rows, 0)?;
    Ok(())
}

/// List declared kinds.
pub fn kinds(context: &Context, args: &Args) -> Result<()> {
    let mut rows = Vec::new();
    for kind in context.catalog.kinds() {
        rows.push(vec![
            kind.name.clone(),
            kind.title.clone(),
            kind.fields.len().to_string(),
            kind.transitions.len().to_string(),
            kind.source.clone(),
        ]);
    }
    if args.format() == crate::terminal::output::Format::Json {
        let headers = vec![
            "name".to_string(),
            "title".to_string(),
            "fields".to_string(),
            "transitions".to_string(),
            "declaration".to_string(),
        ];
        println!("{}", crate::terminal::output::rows_json(&headers, &rows));
        return Ok(());
    }
    let headers = vec![
        "kind".to_string(),
        "title".to_string(),
        "field".to_string(),
        "transition".to_string(),
        "declaration".to_string(),
    ];
    crate::terminal::output::print_table(args, &headers, &rows, 0)?;
    Ok(())
}

/// Show one kind declaration in full.
pub fn explain(context: &Context, kind_name: &str, args: &Args) -> Result<()> {
    let kind = context.catalog.require(kind_name)?;
    let sort_parts = match &kind.numbering {
        Some(name) => match context.numbering(name) {
            Ok(scheme) => {
                let mut parts = vec![scheme.counter.clone()];
                parts.extend(scheme.tokens.keys().cloned());
                parts
            }
            Err(_) => Vec::new(),
        },
        None => Vec::new(),
    };
    if args.format() == crate::terminal::output::Format::Json {
        println!("{}", crate::config::schema::kind_json(context, kind)?);
        return Ok(());
    }

    // Declaration metadata shares the same layout as record details.
    let mut head: Vec<(&str, String)> = vec![
        ("declaration", kind.source.clone()),
        ("storage", kind.table_path().to_string()),
        ("key", kind.key.join(", ")),
    ];
    if let Some(scheme) = &kind.numbering {
        head.push(("number", scheme.clone()));
        if !sort_parts.is_empty() {
            head.push(("sort parts", sort_parts.join(", ")));
        }
    }
    println!("{} ({})", kind.title, kind.name);
    let head = head
        .into_iter()
        .map(|(label, value)| (format!("{label}:"), value))
        .collect::<Vec<_>>();
    print!("{}", text::pairs_with(&head, 0, args.switch("compact")));
    println!();

    let headers = vec![
        "field".to_string(),
        "type".to_string(),
        "role".to_string(),
        "required".to_string(),
        "registry".to_string(),
        "target".to_string(),
        "display".to_string(),
    ];
    let rows: Vec<Vec<String>> = kind
        .fields
        .iter()
        .map(|field| {
            vec![
                field.name.clone(),
                field.kind.name().to_string(),
                match field.role {
                    crate::config::kind::Role::Spec => "spec".to_string(),
                    crate::config::kind::Role::Status => "tool".to_string(),
                },
                match (&field.required, &field.required_when) {
                    (true, _) => "yes".to_string(),
                    (false, Some(condition)) => {
                        format!("{} == {}", condition.field, condition.value)
                    }
                    _ => String::new(),
                },
                field.registry.clone().unwrap_or_default(),
                field.target.clone().unwrap_or_default(),
                field.display.clone(),
            ]
        })
        .collect();
    crate::terminal::output::print_table(args, &headers, &rows, 0)?;

    if !kind.transitions.is_empty() {
        println!();
        let headers = vec![
            "transition".to_string(),
            "to".to_string(),
            "from".to_string(),
            "requires".to_string(),
        ];
        let rows: Vec<Vec<String>> = kind
            .transitions
            .iter()
            .map(|transition| {
                vec![
                    transition.name.clone(),
                    transition.to.clone(),
                    transition.from.join(", "),
                    transition.requires.join(", "),
                ]
            })
            .collect();
        crate::terminal::output::print_table(args, &headers, &rows, 0)?;
    }

    if !kind.relations.is_empty() {
        println!();
        let headers = vec!["relation".to_string(), "target".to_string()];
        let rows: Vec<Vec<String>> = kind
            .relations
            .iter()
            .map(|relation| vec![relation.name.clone(), relation.target.clone()])
            .collect();
        crate::terminal::output::print_table(args, &headers, &rows, 0)?;
    }

    if !kind.views.is_empty() {
        println!();
        let headers = vec!["view".to_string(), "columns".to_string()];
        let rows: Vec<Vec<String>> = kind
            .views
            .iter()
            .map(|view| vec![view.name.clone(), view.columns.join(", ")])
            .collect();
        crate::terminal::output::print_table(args, &headers, &rows, 0)?;
    }
    Ok(())
}

/// Verify every table against its declaration.
pub fn check(context: &mut Context, args: &Args) -> Result<i32> {
    let problems = crate::execution::checks::problems(context)?;

    if args.format() == crate::terminal::output::Format::Json {
        let items: Vec<crate::terminal::output::ReportItem> = problems
            .iter()
            .map(
                |(file, line, message)| crate::terminal::output::ReportItem {
                    name: file.clone(),
                    ok: false,
                    line: *line as u32,
                    detail: message.clone(),
                },
            )
            .collect();
        println!(
            "{}",
            crate::terminal::output::report_json(problems.is_empty(), &items)
        );
    } else if problems.is_empty() {
        if !args.switch("quiet") {
            println!("check passed");
        }
    } else {
        for (file, line, message) in &problems {
            println!("{file}:{line}: {message}");
        }
        println!("{} problem(s)", problems.len());
    }
    Ok(if problems.is_empty() { 0 } else { 1 })
}

/// Verify the workspace and its inputs.
pub fn doctor(context: &mut Context, args: &Args) -> Result<i32> {
    let mut checks: Vec<(String, bool, String)> = Vec::new();

    checks.push((
        "workspace".to_string(),
        true,
        context.workspace.root().display().to_string(),
    ));
    checks.push((
        "kinds".to_string(),
        !context.catalog.kinds().is_empty(),
        format!("{} item(s)", context.catalog.kinds().len()),
    ));
    checks.push((
        "registry".to_string(),
        true,
        format!("{} item(s)", context.registries.len()),
    ));
    let git = crate::config::identity::git_config(context.workspace.root(), "user.email");
    checks.push((
        "git identity".to_string(),
        git.is_some(),
        git.unwrap_or_else(|| "user.email is not set".to_string()),
    ));
    let repo = std::process::Command::new("git")
        .arg("-C")
        .arg(context.workspace.root())
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    checks.push(("repository".to_string(), repo, {
        if repo {
            "initialised".to_string()
        } else {
            "not a git repository".to_string()
        }
    }));

    for kind in context.catalog.kinds() {
        if kind.document_dir().is_some() {
            let count = crate::storage::document::count(context, kind).unwrap_or(0);
            checks.push((
                format!("documents {}", kind.name),
                true,
                format!("{count} page(s) in {}", kind.table_path()),
            ));
            continue;
        }
        let path = context.workspace.resolve(kind.table_path());
        let exists = path.exists();
        checks.push((
            format!("table {}", kind.name),
            true,
            if exists {
                kind.table_path().to_string()
            } else {
                format!("{} is created on first write", kind.table_path())
            },
        ));
    }

    let failed = checks.iter().filter(|(_, ok, _)| !ok).count();
    if args.format() == crate::terminal::output::Format::Json {
        let items: Vec<crate::terminal::output::ReportItem> = checks
            .iter()
            .map(|(name, ok, detail)| crate::terminal::output::ReportItem {
                name: name.clone(),
                ok: *ok,
                line: 0,
                detail: detail.clone(),
            })
            .collect();
        println!(
            "{}",
            crate::terminal::output::report_json(failed == 0, &items)
        );
    } else {
        let headers = vec![
            "check".to_string(),
            "result".to_string(),
            "note".to_string(),
        ];
        let rows: Vec<Vec<String>> = checks
            .iter()
            .map(|(name, ok, detail)| {
                vec![
                    name.clone(),
                    if *ok {
                        "ok".to_string()
                    } else {
                        "failed".to_string()
                    },
                    detail.clone(),
                ]
            })
            .collect();
        crate::terminal::output::print_table(args, &headers, &rows, 0)?;
        if failed > 0 {
            println!("{failed} check(s) failed");
        }
    }
    Ok(if failed == 0 { 0 } else { 1 })
}

/// List the managed paths and their governance strength.
pub fn scope(context: &Context, args: &Args) -> Result<()> {
    let mut rows = Vec::new();
    for rule in &context.workspace.paths {
        rows.push(vec![
            rule.path.clone(),
            rule.level.name().to_string(),
            rule.regions.join(", "),
        ]);
    }
    if args.format() == crate::terminal::output::Format::Json {
        let items: Vec<String> = rows
            .iter()
            .map(|row| {
                format!(
                    "{{\"path\":\"{}\",\"level\":\"{}\",\"regions\":\"{}\"}}",
                    row[0], row[1], row[2]
                )
            })
            .collect();
        println!(
            "{{\"count\":{},\"data\":[{}]}}",
            items.len(),
            items.join(",")
        );
        return Ok(());
    }
    let headers = vec![
        "path".to_string(),
        "level".to_string(),
        "blocks".to_string(),
    ];
    crate::terminal::output::print_table(args, &headers, &rows, 0)?;
    Ok(())
}

/// Guard for commands that need a kind name.
pub fn require_kind<'a>(context: &'a Context, name: &str) -> Result<&'a crate::config::kind::Kind> {
    context
        .catalog
        .require(name)
        .map_err(|error| Error::usage(format!("{error}")))
}

/// List pending requests.
pub fn request_list(context: &Context, args: &Args) -> Result<()> {
    let manifests = crate::collaboration::request::list(&context.workspace)?;
    if args.format() == crate::terminal::output::Format::Json {
        let items: Vec<String> = manifests
            .iter()
            .map(|manifest| {
                format!(
                    "{{\"name\":\"{}\",\"kind\":\"{}\",\"verb\":\"{}\",\"requester\":\"{}\"}}",
                    manifest.name, manifest.kind, manifest.verb, manifest.requester
                )
            })
            .collect();
        println!(
            "{{\"count\":{},\"data\":[{}]}}",
            items.len(),
            items.join(",")
        );
        return Ok(());
    }
    let headers = vec![
        "request".to_string(),
        "kind".to_string(),
        "verbs".to_string(),
        "requester".to_string(),
        "baseline".to_string(),
    ];
    let rows: Vec<Vec<String>> = manifests
        .iter()
        .map(|manifest| {
            vec![
                manifest.name.clone(),
                manifest.kind.clone(),
                manifest.verb.clone(),
                manifest.requester.clone(),
                manifest.baseline.clone(),
            ]
        })
        .collect();
    crate::terminal::output::print_table(args, &headers, &rows, 0)?;
    Ok(())
}

/// Show one pending request.
pub fn request_show(context: &Context, name: &str, args: &Args) -> Result<()> {
    let manifest = crate::collaboration::request::list(&context.workspace)?
        .into_iter()
        .find(|item| item.name == name)
        .ok_or_else(|| Error::usage(format!("no pending request `{name}`")))?;
    let mut rows = vec![
        ("request:".into(), manifest.name),
        ("kind:".into(), manifest.kind),
        ("verbs:".into(), manifest.verb),
        (
            "requester:".into(),
            format!("{} <{}>", manifest.requester, manifest.email),
        ),
        ("baseline:".into(), manifest.baseline),
        ("time:".into(), manifest.created),
    ];
    if !manifest.keys.is_empty() {
        rows.push(("key:".into(), manifest.keys.join(" ")));
    }
    for (key, value) in &manifest.values {
        rows.push((format!("{key} ="), value.clone()));
    }
    print!("{}", text::pairs_with(&rows, 0, args.switch("compact")));
    Ok(())
}

/// Apply pending requests through the ordinary verb path.
pub fn request_apply(context: &mut Context, args: &Args) -> Result<i32> {
    let wanted: Vec<String> = args.positional.iter().skip(2).cloned().collect();
    let baseline = crate::execution::pipeline::begin(context)?;
    let mut selected = crate::collaboration::request::list(&context.workspace)?;
    if !wanted.is_empty() {
        selected.retain(|manifest| wanted.iter().any(|name| name == &manifest.name));
        if selected.len() != wanted.len() {
            return Err(Error::usage(format!(
                "unknown request among: {}",
                wanted.join(", ")
            )));
        }
    }
    if selected.is_empty() {
        println!("no pending request");
        return Ok(0);
    }

    let dry_run = args.switch("dry-run");
    // Check every target against the same initial tree, before replay changes it.
    let mut digests = std::collections::BTreeSet::new();
    for manifest in &selected {
        if !digests.insert(&manifest.digest) {
            return Err(Error::conflict(
                "the selected focus contains duplicate request content",
            ));
        }
        crate::collaboration::request::validate_apply(context, manifest)?;
    }
    let mut affected = Vec::new();
    let mut removed = Vec::new();
    let mut output = String::new();
    let mut applied = 0;
    for manifest in selected {
        let kind = context.catalog.require(&manifest.kind)?.clone();
        let options = engine::Options {
            dry_run: false,
            tokens: manifest.tokens.clone(),
            filters: Vec::new(),
            filter: None,
            case: None,
            body: manifest.body.clone(),
            scope: "current".into(),
            display: false,
            force: manifest.force,
            long: false,
            mine: false,
            claimed: false,
        };
        let outcome = crate::execution::application::execute(
            context,
            &kind,
            &manifest.verb,
            &manifest.keys,
            &manifest.values,
            &options,
        )?;
        affected.extend(outcome.affected.clone());
        removed.extend(outcome.removed.clone());
        let print = crate::terminal::output::Print {
            format: args.format(),
            max_width: args.max_width(),
            compact: args.switch("compact"),
            ..Default::default()
        };
        let text = crate::terminal::output::render(context, &kind, &outcome, &print)?;
        output.push_str(&text);
        if !dry_run {
            crate::collaboration::request::remove(&context.workspace, &manifest.path)?;
            crate::collaboration::request::append_log(
                &context.workspace,
                "applied",
                &manifest,
                "ok",
            )?;
            removed.push(context.workspace.relative(&manifest.path));
            affected.push(format!("{}/applied.log", context.workspace.layout.requests));
            applied += 1;
        }
    }
    if dry_run {
        drop(baseline);
        print!("{output}");
        println!("dry run: nothing applied");
    } else {
        let notes = crate::execution::pipeline::finish_paths(
            context,
            "request apply",
            &mut affected,
            &removed,
            baseline,
        )?;
        print!("{output}");
        for note in notes {
            println!("{note}");
        }
        println!("{applied} request(s) applied");
    }
    Ok(0)
}

/// Drop a pending request and record why.
pub fn request_reject(context: &mut Context, args: &Args) -> Result<i32> {
    let name = args
        .positional
        .get(2)
        .ok_or_else(|| Error::usage("usage: clerk request reject <name> --reason <text>"))?;
    let reason = args.value("reason").unwrap_or("no reason given");
    let baseline = crate::execution::pipeline::begin(context)?;
    let manifest = crate::collaboration::request::list(&context.workspace)?
        .into_iter()
        .find(|item| item.name == *name)
        .ok_or_else(|| Error::usage(format!("no pending request `{name}`")))?;
    crate::collaboration::request::remove(&context.workspace, &manifest.path)?;
    crate::collaboration::request::append_log(&context.workspace, "rejected", &manifest, reason)?;
    let mut affected = vec![format!(
        "{}/rejected.log",
        context.workspace.layout.requests
    )];
    let removed = vec![context.workspace.relative(&manifest.path)];
    let notes = crate::execution::pipeline::finish_paths(
        context,
        "request reject",
        &mut affected,
        &removed,
        baseline,
    )?;
    for note in notes {
        println!("{note}");
    }
    println!("rejected {}", manifest.name);
    Ok(0)
}

/// Reseal every declared sealed path and record why.
pub fn seal(context: &mut Context, args: &Args) -> Result<i32> {
    let reason = args
        .value("note")
        .ok_or_else(|| Error::usage("usage: clerk seal --note <reason>"))?
        .to_string();
    let baseline = crate::execution::pipeline::begin_reseal(context)?;
    let identity = context.identity()?.clone();
    let targets = crate::storage::seal::targets(&context.workspace)?;
    if targets.is_empty() {
        println!("nothing is declared sealed");
        return Ok(0);
    }
    let stamp = crate::foundation::time::stamp(
        crate::foundation::time::now_epoch(),
        context.workspace.policy.offset_hours,
    );
    // Resealing is a full rebuild: keys the rules no longer cover are dropped.
    let existing = crate::storage::seal::Seals::load(&context.workspace)?;
    let stale: Vec<String> = existing
        .entries()
        .keys()
        .filter(|key| !targets.contains(key))
        .cloned()
        .collect();
    crate::storage::seal::Seals::update(&context.workspace, &targets, &stale, &identity, &stamp)?;
    crate::storage::seal::note(&context.workspace, &identity, &stamp, &reason)?;
    let mut affected = targets.clone();
    affected.push(context.workspace.layout.seal.clone());
    affected.push(context.workspace.layout.seal_log.clone());
    let notes =
        crate::execution::pipeline::finish_paths(context, "seal", &mut affected, &[], baseline)?;
    if !args.switch("quiet") {
        println!("sealed {} key(s) with note: {reason}", targets.len());
    }
    for note in notes {
        println!("{note}");
    }
    Ok(0)
}

/// Create the files a new workspace needs.
pub fn init(context: &Context, args: &Args) -> Result<i32> {
    let root = context.workspace.root();
    let marker = root.join(crate::config::workspace::MARKER);
    // init fills in whatever is missing, so running it twice is harmless.
    let _ = args;
    for dir in [
        &context.workspace.layout.registry,
        &context.workspace.layout.kinds,
        &context.workspace.layout.data,
    ] {
        let path = context.workspace.resolve(dir);
        std::fs::create_dir_all(&path)
            .map_err(|error| crate::foundation::error::io_error(&path, error))?;
    }
    if !marker.exists() {
        if let Some(parent) = marker.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| crate::foundation::error::io_error(parent, error))?;
        }
        let name = root
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "workspace".to_string());
        std::fs::write(&marker, format!("version = 1\nname = \"{name}\"\n"))
            .map_err(|error| crate::foundation::error::io_error(&marker, error))?;
    }
    let status = context.workspace.resolve(&context.workspace.layout.status);
    if !status.exists() {
        std::fs::write(&status, "# Status\n")
            .map_err(|error| crate::foundation::error::io_error(&status, error))?;
    }
    println!("{}", root.display());
    Ok(0)
}

/// Print the directory a document category lives in.
pub fn place(context: &Context, name: &str) -> Result<i32> {
    let registry = crate::config::registry::Registry::load(&context.workspace, "category")?;
    let Some(entry) = registry
        .key_of(name)
        .and_then(|key| registry.find(&key).cloned())
    else {
        let known: Vec<String> = registry
            .entries()
            .iter()
            .map(|entry| {
                format!(
                    "{} -> {}",
                    entry.key,
                    entry.cells.get("path").cloned().unwrap_or_default()
                )
            })
            .collect();
        return Err(Error::usage(format!(
            "unknown category `{name}`; known: {}",
            known.join(", ")
        )));
    };
    println!(
        "{}",
        entry
            .cells
            .get("path")
            .cloned()
            .unwrap_or_else(|| entry.key.clone())
    );
    Ok(0)
}

/// Print a shell completion script.
pub fn completions(context: &Context, shell: &str) -> Result<i32> {
    let mut commands: Vec<&str> = crate::interface::command::BUILTINS
        .iter()
        .map(|spec| spec.name)
        .collect();
    for kind in context.catalog.kinds() {
        commands.push(&kind.name);
    }
    commands.sort();
    commands.dedup();
    let words = commands.join(" ");
    match shell {
        "bash" => println!("complete -W \"{words}\" clerk"),
        "zsh" => println!("#compdef clerk\n_arguments '1:command:({words})'"),
        "powershell" => println!(
            "Register-ArgumentCompleter -CommandName clerk -ScriptBlock {{ param($w,$c,$p,$t) \"{words}\".Split(' ') | Where-Object {{ $_ -like \"$t*\" }} | ForEach-Object {{ [System.Management.Automation.CompletionResult]::new($_,$_,'ParameterValue',$_) }} }}"
        ),
        other => {
            return Err(Error::usage(format!(
                "unknown shell `{other}`; expected bash, zsh or powershell"
            )));
        }
    }
    Ok(0)
}

/// List or edit the people registry.
pub fn people(context: &mut Context, args: &Args) -> Result<i32> {
    let sub = args
        .positional
        .get(1)
        .cloned()
        .unwrap_or_else(|| "list".to_string());
    let mut baseline = if sub == "add" || sub == "remove" {
        Some(crate::execution::pipeline::begin(context)?)
    } else {
        None
    };
    let path = context.workspace.registry_dir().join("person.csv");
    let mut table = if path.exists() {
        crate::storage::store::Table::load(&path)?
    } else {
        crate::storage::store::Table {
            headers: vec![
                "person".to_string(),
                "email".to_string(),
                "name".to_string(),
            ],
            rows: Vec::new(),
        }
    };
    match sub.as_str() {
        "list" => {
            if args.format() == crate::terminal::output::Format::Json {
                println!(
                    "{}",
                    crate::terminal::output::rows_json(&table.headers, &table.rows)
                );
                return Ok(0);
            }
            crate::terminal::output::print_table(args, &table.headers, &table.rows, 0)?;
            Ok(0)
        }
        "add" => {
            let person = args.value("person").ok_or_else(|| {
                Error::usage("usage: clerk people add --person <name> --email <mail>")
            })?;
            let email = args.value("email").ok_or_else(|| {
                Error::usage("usage: clerk people add --person <name> --email <mail>")
            })?;
            let name = args.value("name").unwrap_or(person);
            if table
                .rows
                .iter()
                .any(|row| row.get(1).map(String::as_str) == Some(email))
            {
                return Err(Error::conflict(format!("`{email}` is already registered")));
            }
            table.rows.push(vec![
                person.to_string(),
                email.to_string(),
                name.to_string(),
            ]);
            table.save_to(&path)?;
            let mut affected = vec![context.workspace.relative(&path)];
            let notes = crate::execution::pipeline::finish_paths(
                context,
                "people",
                &mut affected,
                &[],
                baseline.take().expect("write guard"),
            )?;
            for note in notes {
                println!("{note}");
            }
            println!("added {email}");
            Ok(0)
        }
        "remove" => {
            let email = args
                .value("email")
                .ok_or_else(|| Error::usage("usage: clerk people remove --email <mail>"))?;
            let before = table.rows.len();
            table
                .rows
                .retain(|row| row.get(1).map(String::as_str) != Some(email));
            if table.rows.len() == before {
                return Err(Error::usage(format!("`{email}` is not registered")));
            }
            table.save_to(&path)?;
            let mut affected = vec![context.workspace.relative(&path)];
            let notes = crate::execution::pipeline::finish_paths(
                context,
                "people",
                &mut affected,
                &[],
                baseline.take().expect("write guard"),
            )?;
            for note in notes {
                println!("{note}");
            }
            println!("removed {email}");
            Ok(0)
        }
        other => Err(Error::usage(format!(
            "unknown people subcommand `{other}`; expected list, add or remove"
        ))),
    }
}

/// Show the repository registry and, with `--check`, the local checkouts.
pub fn repos(context: &Context, args: &Args) -> Result<i32> {
    let registry = crate::config::registry::Registry::load(&context.workspace, "repo")?;
    let mut rows = Vec::new();
    for entry in registry.entries() {
        let remotes = entry.cells.get("remotes").cloned().unwrap_or_default();
        let mut checkout = String::new();
        if args.switch("check") {
            checkout = match find_checkout(context, &remotes) {
                Some(path) => path,
                None => "missing".to_string(),
            };
        }
        rows.push(vec![
            entry.key.clone(),
            entry.display.clone(),
            remotes,
            entry.cells.get("visibility").cloned().unwrap_or_default(),
            checkout,
        ]);
    }
    let headers = if args.switch("check") {
        vec![
            "key".to_string(),
            "display".to_string(),
            "remotes".to_string(),
            "visibility".to_string(),
            "local".to_string(),
        ]
    } else {
        vec![
            "key".to_string(),
            "display".to_string(),
            "remotes".to_string(),
            "visibility".to_string(),
            String::new(),
        ]
    };
    if args.switch("check") {
        crate::terminal::output::print_table(args, &headers, &rows, 0)?;
        let missing = rows.iter().filter(|row| row[4] == "missing").count();
        return Ok(if missing == 0 { 0 } else { 1 });
    }
    let short: Vec<Vec<String>> = rows.iter().map(|row| row[..4].to_vec()).collect();
    crate::terminal::output::print_table(args, &headers[..4], &short, 0)?;
    Ok(0)
}

pub(crate) fn find_checkout(context: &Context, remotes: &str) -> Option<String> {
    let parent = context.workspace.root().parent()?;
    let wanted: Vec<String> = remotes
        .split(';')
        .flat_map(|item| item.split('|'))
        .map(str::trim)
        .map(crate::git::sync::normalise)
        .collect();
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(["remote", "get-url", "origin"])
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if wanted
            .iter()
            .any(|item| *item == crate::git::sync::normalise(&remote))
        {
            return Some(path.display().to_string());
        }
    }
    None
}

/// Summarise the ledger and the current focus.
pub fn brief(context: &Context, args: &Args) -> Result<i32> {
    let mut rows = Vec::new();
    for kind in context.catalog.kinds() {
        let Some(state) = &kind.state else {
            continue;
        };
        let registry = context.registry(&state.registry)?;
        let records = crate::storage::records::all(context, kind)?;
        for entry in registry.entries() {
            let count = records
                .iter()
                .filter(|record| {
                    record
                        .get(&state.field)
                        .map(|value| value.trim() == entry.key)
                        .unwrap_or(false)
                })
                .count();
            if count > 0 {
                rows.push(vec![
                    kind.name.clone(),
                    entry.display.clone(),
                    count.to_string(),
                ]);
            }
        }
    }
    let batch_path = context.workspace.registry_dir().join("focus.csv");
    let mut focus = Vec::new();
    if batch_path.exists() {
        let table = crate::storage::store::Table::load(&batch_path)?;
        let at = table.column("key").unwrap_or(0);
        for row in &table.rows {
            if let Some(key) = row.get(at)
                && !key.trim().is_empty()
            {
                focus.push(key.trim().to_string());
            }
        }
    }
    if args.format() == crate::terminal::output::Format::Json {
        let items: Vec<String> = rows
            .iter()
            .map(|row| {
                format!(
                    "{{\"kind\":\"{}\",\"state\":\"{}\",\"count\":{}}}",
                    row[0], row[1], row[2]
                )
            })
            .collect();
        println!(
            "{{\"states\":[{}],\"focus\":[{}]}}",
            items.join(","),
            focus
                .iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        return Ok(0);
    }
    let headers = vec!["kind".to_string(), "state".to_string(), "count".to_string()];
    crate::terminal::output::print_table(args, &headers, &rows, 0)?;
    if !focus.is_empty() {
        println!("current focus: {}", focus.join(", "));
    }
    Ok(0)
}

/// Read or replace the current focus.
pub fn focus(context: &mut Context, args: &Args) -> Result<i32> {
    if args.value("request").is_some() {
        return Err(Error::usage(
            "focus does not support --request; use a workflow step for transactional changes",
        ));
    }
    let sub = args.positional.get(1).map(String::as_str).unwrap_or("show");
    if sub == "show" {
        if args.value("batch").is_some() {
            return Err(Error::usage("--batch requires focus set, add or remove"));
        }
        if args.positional.len() > 2 {
            return Err(Error::usage("focus show takes no keys"));
        }
        let keys = crate::collaboration::focus::keys(context)?;
        if args.switch("json") {
            println!("{}", crate::config::schema::strings_json(&keys));
        } else {
            for key in keys {
                println!("{key}");
            }
        }
        return Ok(0);
    }
    let change = crate::collaboration::focus::Change::parse(sub)?;
    let ids: Vec<String> = args.positional.iter().skip(2).cloned().collect();
    if ids.is_empty() && sub != "set" {
        return Err(Error::usage("focus add/remove needs at least one key"));
    }
    if args.value("batch").is_some() {
        return crate::execution::batch::capture(
            context,
            args,
            crate::collaboration::workflow::Step {
                kind: Some("focus".into()),
                verb: sub.into(),
                keys: ids,
                ..Default::default()
            },
        );
    }
    let write = crate::execution::pipeline::begin(context)?;
    let outcome = crate::collaboration::focus::apply(context, change, &ids)?;
    let mut affected = outcome.affected;
    if args.switch("dry-run") {
        drop(write);
    } else {
        let notes =
            crate::execution::pipeline::finish_paths(context, "focus", &mut affected, &[], write)?;
        if !args.switch("json") {
            for note in notes {
                println!("{note}");
            }
        }
    }
    if args.switch("json") {
        println!(
            "{{\"keys\":{},\"added\":{},\"removed\":{},\"unchanged\":{},\"dry_run\":{},\"execution\":{}}}",
            crate::config::schema::strings_json(&outcome.keys),
            crate::config::schema::strings_json(&outcome.added),
            crate::config::schema::strings_json(&outcome.removed),
            crate::config::schema::strings_json(&outcome.unchanged),
            args.switch("dry-run"),
            crate::git::operation::current().expect("operation").json()
        );
    } else {
        println!("focus: {}", outcome.keys.join(", "));
    }
    Ok(0)
}
