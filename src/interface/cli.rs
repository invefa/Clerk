//! Command line parsing and dispatch.
//!
//! Kind commands are resolved from the declaration catalog, so a new kind
//! becomes a command without touching this file.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::kind::Kind;
use crate::execution::context::Context;
use crate::execution::engine::{self, Options, Values};
use crate::execution::pipeline;
use crate::foundation::error::{Error, Result};
use crate::interface::commands;
use crate::terminal::output::{self, Format};

/// Resolve the subcommand in the second position, keeping it empty when the
/// caller gave none so the branch can pick its own default.
fn sub_command(args: &Args, command: &str) -> Result<String> {
    let candidates = crate::interface::command::builtin(command)
        .expect("builtin command")
        .subcommands;
    let value = args.positional.get(1).cloned().unwrap_or_default();
    if value.is_empty() {
        return Ok(value);
    }
    Ok(resolve(&value, candidates)?.to_string())
}
/// Resolve a unique prefix against the candidate names.
pub fn resolve<'a>(input: &str, candidates: &[&'a str]) -> Result<&'a str> {
    if let Some(exact) = candidates.iter().find(|name| **name == input) {
        return Ok(exact);
    }
    let hits: Vec<&str> = candidates
        .iter()
        .copied()
        .filter(|name| name.starts_with(input))
        .collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err(Error::usage(format!(
            "unknown command `{input}`; expected one of {}",
            candidates.join(", ")
        ))),
        _ => Err(Error::usage(format!(
            "`{input}` is ambiguous: {}",
            hits.join(", ")
        ))),
    }
}
/// Whether a table prints its heading row; `--headers hide` turns it off.
pub fn headers_shown(args: &Args) -> Result<bool> {
    match args.value("headers") {
        None | Some("show") => Ok(true),
        Some("hide") => Ok(false),
        Some(other) => Err(Error::usage(format!(
            "`--headers {other}` is not known; use `show` or `hide`"
        ))),
    }
}
/// Parsed command line.
#[derive(Clone, Debug, Default)]
pub struct Args {
    pub positional: Vec<String>,
    pub values: BTreeMap<String, String>,
    pub switches: BTreeSet<String>,
    /// Explicitly disabled options whose configured defaults must not be filled.
    pub off: BTreeSet<String>,
    negations: Vec<(String, usize)>,
    order: BTreeMap<String, usize>,
}

impl Args {
    fn supplied(&mut self, name: &str, value: Option<&str>, at: usize) {
        if name == "no" {
            if let Some(value) = value {
                self.negations.push((value.into(), at));
            } else {
                self.switches.insert(name.into());
            }
            return;
        }
        self.order.insert(name.into(), at);
        self.off.remove(name);
        if let Some(value) = value {
            self.values.insert(name.into(), value.into());
        } else {
            self.switches.insert(name.into());
        }
    }

    fn negate(&mut self, name: &str, at: usize) -> Result<()> {
        use crate::interface::command::option;
        let spec =
            option(name).ok_or_else(|| Error::usage(format!("unknown option `--no-{name}`")))?;
        let behavior = spec.negation.ok_or_else(|| {
            Error::usage(format!("option `--{name}` cannot be disabled or reset"))
        })?;
        self.apply_negation(name, behavior, at);
        Ok(())
    }

    fn apply_negation(
        &mut self,
        name: &str,
        behavior: crate::interface::command::Negation,
        at: usize,
    ) {
        use crate::interface::command::Negation;
        if self.order.get(name).is_some_and(|later| *later > at) {
            return;
        }
        self.values.remove(name);
        self.switches.remove(name);
        self.off.insert(name.into());
        self.order.insert(name.into(), at);
        if let Negation::Value(value) = behavior {
            self.values.insert(name.into(), value.into());
        }
    }

    /// Resolve contextual enum candidates after command discovery, before defaults or capture.
    pub fn resolve_negations(&mut self, context: Option<&Context>) -> Result<()> {
        if self.negations.is_empty() {
            return Ok(());
        }
        let offset = usize::from(
            self.positional.first().is_some_and(|name| name == "help") && self.positional.len() > 1,
        );
        let requested = self
            .positional
            .get(offset)
            .map(String::as_str)
            .unwrap_or("help");
        let mut commands = crate::interface::command::BUILTINS
            .iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        if let Some(context) = context {
            commands.extend(
                context
                    .catalog
                    .kinds()
                    .iter()
                    .map(|kind| kind.name.as_str()),
            );
        }
        let command = resolve(requested, &commands)?;
        let kind = context.and_then(|context| context.catalog.find(command));
        let requested = self
            .positional
            .get(offset + 1)
            .map(String::as_str)
            .unwrap_or("");
        let verbs = kind
            .map(crate::interface::command::verb_names)
            .unwrap_or_else(|| {
                crate::interface::command::builtin(command)
                    .map(|spec| {
                        spec.subcommands
                            .iter()
                            .map(|value| value.to_string())
                            .collect()
                    })
                    .unwrap_or_default()
            });
        let verb = if requested.is_empty() || verbs.is_empty() {
            requested
        } else {
            resolve(
                requested,
                &verbs.iter().map(String::as_str).collect::<Vec<_>>(),
            )?
        };
        let options = crate::interface::choice::negatable(context, command, verb);
        let candidates = options
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        for (value, at) in std::mem::take(&mut self.negations) {
            let name = crate::interface::command::resolve_value("no", &value, &candidates)?;
            let behavior = options
                .iter()
                .find(|(key, _)| *key == name)
                .expect("resolved target")
                .1;
            self.apply_negation(&name, behavior, at);
        }
        Ok(())
    }

    pub fn parse(argv: &[String]) -> Result<Self> {
        let mut args = Args::default();
        let mut index = 0;
        let mut literal = false;
        while index < argv.len() {
            let token = &argv[index];
            index += 1;
            if literal {
                args.positional.push(token.clone());
                continue;
            }
            if token == "--" {
                literal = true;
                continue;
            }
            if let Some(body) = token.strip_prefix("--") {
                if body.is_empty() {
                    literal = true;
                    continue;
                }
                if let Some((name, value)) = body.split_once('=') {
                    args.supplied(name, Some(value), index);
                    continue;
                }
                if let Some(name) = body.strip_prefix("no-") {
                    args.negate(name, index)?;
                    continue;
                }
                if crate::interface::command::is_switch(body) {
                    args.supplied(body, None, index);
                    continue;
                }
                let next = argv.get(index);
                match next {
                    Some(value) if !value.starts_with("--") => {
                        args.supplied(body, Some(value), index);
                        index += 1;
                    }
                    _ => {
                        args.supplied(body, None, index);
                    }
                }
                continue;
            }
            if let Some(body) = token.strip_prefix('-')
                && !body.is_empty()
                && !body.starts_with('-')
            {
                for (long, inline) in crate::interface::command::short_options(body)? {
                    if crate::interface::command::is_switch(long) {
                        args.supplied(long, None, index);
                    } else {
                        let value = match inline {
                            Some(value) => value.to_string(),
                            None => {
                                let next = argv.get(index).filter(|value| !value.starts_with("--"));
                                match next {
                                    Some(value) => {
                                        index += 1;
                                        value.clone()
                                    }
                                    None => {
                                        return Err(Error::usage(format!(
                                            "--{long} needs a value"
                                        )));
                                    }
                                }
                            }
                        };
                        args.supplied(long, Some(&value), index);
                    }
                }
                continue;
            }
            args.positional.push(token.clone());
        }
        for name in &args.switches {
            if !crate::interface::command::is_switch(name) {
                if crate::interface::command::option(name).is_some() {
                    return Err(Error::usage(format!("option `--{name}` needs a value")));
                }
                return Err(Error::usage(format!("unknown option `--{name}`")));
            }
        }
        for (name, value) in &mut args.values {
            if !crate::interface::command::choices(name).is_empty() {
                *value = crate::interface::command::resolve_choice(name, value)?;
            }
            if let Some(spec) = crate::interface::command::option(name) {
                if spec.switch() {
                    return Err(Error::usage(format!(
                        "`--{name}` takes no value; use --no-{name} to turn it off"
                    )));
                }
                spec.validate(value)?;
            }
        }
        Ok(args)
    }

    pub fn switch(&self, name: &str) -> bool {
        self.switches.contains(name) && !self.off.contains(name)
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    pub fn format(&self) -> Format {
        if self.switch("json") {
            Format::Json
        } else if self.switch("tsv") {
            Format::Tsv
        } else if self.switch("csv") {
            Format::Csv
        } else if self.switch("md") {
            Format::Markdown
        } else {
            Format::Table
        }
    }

    pub fn max_width(&self) -> usize {
        self.value("max-width")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0)
    }

    pub fn columns(&self) -> Option<Vec<String>> {
        self.value("columns").map(|value| {
            value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
    }

    /// Field values for a write command, with numbering tokens taken out.
    pub fn payload(&self, kind: &Kind) -> (Values, BTreeMap<String, String>) {
        let mut values = Values::new();
        let mut tokens = BTreeMap::new();
        for (name, value) in &self.values {
            if crate::interface::command::reserved(name) {
                continue;
            }
            if kind.field(name).is_some() {
                values.insert(name.clone(), value.clone());
            } else {
                tokens.insert(name.clone(), value.clone());
            }
        }
        (values, tokens)
    }

    /// Filters for a list command.
    pub fn filters(&self, kind: &Kind) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (name, value) in &self.values {
            if crate::interface::command::reserved(name) {
                continue;
            }
            if kind.field(name).is_some() {
                out.push((name.clone(), value.clone()));
            }
        }
        out
    }
}

/// Entry point: returns the process exit code.
pub fn run(argv: Vec<String>) -> i32 {
    crate::git::operation::reset();
    let parsed = Args::parse(&argv).ok();
    let json = parsed.as_ref().is_some_and(|args| args.switch("json"));
    let root = parsed
        .as_ref()
        .and_then(|args| args.value("root"))
        .map(str::to_string);
    match dispatch(argv) {
        Ok(code) => code,
        Err(error) => {
            let mut error_args = parsed.clone().unwrap_or_default();
            if let Ok(context) = Context::open(root.as_deref()) {
                let command = error_args.positional.first().cloned().unwrap_or_default();
                let verb = error_args.positional.get(1).cloned().unwrap_or_default();
                apply_defaults(&mut error_args, &context.command_defaults(&command, &verb));
            }
            eprintln!(
                "{}",
                crate::terminal::style::error(Some(&error_args), &format!("clerk: {error}"))
            );
            if let Some(report) = crate::git::operation::current() {
                if let Ok(workspace) = crate::config::workspace::Workspace::open(root.as_deref())
                    && let Err(error) = crate::git::operation::save(&workspace)
                {
                    eprintln!("cannot save operation receipt: {error}");
                }
                eprintln!("{}", report.summary());
                if json {
                    println!("{{\"ok\":false,\"execution\":{}}}", report.json());
                }
            }
            error.code()
        }
    }
}

/// Read caller intent exactly once, before synchronization or body input.
fn configure_commit_message(context: &Context, args: &Args, writes: bool) -> Result<()> {
    let supplied = args.value("message").is_some() || args.value("message-file").is_some();
    let commits = writes
        && args.value("request").is_none()
        && args.value("batch").is_none()
        && !args.switch("preview");
    if supplied && !commits {
        return Err(Error::usage(
            "commit messages require a Git-managed write; operation retry reuses its original message",
        ));
    }
    if args.value("message").is_some() && args.value("message-file").is_some() {
        return Err(Error::usage(
            "--message and --message-file are mutually exclusive",
        ));
    }
    if !commits {
        return Ok(());
    }
    let message = match (args.value("message"), args.value("message-file")) {
        (Some(text), _) => Some(crate::git::commit_message::validate(text)?),
        (_, Some(source)) => {
            if source == "-" {
                let verb = args.positional.get(1).map(String::as_str).unwrap_or("");
                let command = args.positional[0].as_str();
                let implicit_body = context.catalog.find(command).is_some_and(|kind| {
                    kind.action(verb).is_some_and(|action| {
                        matches!(action.handler.as_str(), "attach" | "append")
                    }) || (verb == "new"
                        && kind.document_dir().is_some()
                        && kind.template.is_none())
                }) && args.value("body").is_none()
                    && args.value("file").is_none();
                let implicit_workflow = command == "batch"
                    && args.value("template").is_none()
                    && args.value("file").is_none();
                if implicit_body
                    || implicit_workflow
                    || ["file", "from"]
                        .iter()
                        .any(|flag| args.value(flag) == Some("-"))
                {
                    return Err(Error::usage(
                        "--message-file - cannot share stdin with body, workflow, CSV or list input; use separate files",
                    ));
                }
            };
            let mut input = Args::default();
            input.values.insert("file".into(), source.into());
            let text = crate::interface::input::body(&context.workspace, &input, false)?
                .expect("explicit message file");
            Some(crate::git::commit_message::validate(&text)?)
        }
        _ => None,
    };
    crate::git::operation::begin(&context.workspace);
    crate::git::operation::change(|report| {
        report.message = message;
        report.verb = args.positional.join(" ");
    });
    crate::git::commit_message::require(&context.workspace)
}

fn dispatch(argv: Vec<String>) -> Result<i32> {
    let mut args = Args::parse(&argv)?;
    if !args.negations.is_empty() {
        let context = Context::open(args.value("root")).ok();
        args.resolve_negations(context.as_ref())?;
    }
    if args.positional.is_empty()
        || args.switch("version")
        || args
            .positional
            .first()
            .is_some_and(|command| command == "version")
    {
        if args.value("message").is_some() || args.value("message-file").is_some() {
            return Err(Error::usage("commit messages require a Git-managed write"));
        }
        for name in args
            .values
            .keys()
            .chain(args.switches.iter())
            .chain(args.off.iter())
        {
            if !crate::interface::command::option(name).is_some_and(|spec| spec.global) {
                return Err(Error::usage(format!("unknown option `--{name}`")));
            }
        }
    }
    if args.switch("version") {
        if !args.positional.is_empty() {
            return Err(Error::usage("--version takes no positional arguments"));
        }
        print!(
            "{}",
            crate::foundation::version::render(args.switch("json"))
        );
        return Ok(0);
    }
    let Some(command) = args.positional.first().cloned() else {
        if let Ok(context) = Context::open(args.value("root")) {
            crate::terminal::reader::help(
                Some(&context),
                &args,
                &crate::interface::help::overview(&context),
            )?;
        } else {
            crate::terminal::reader::help(None, &args, &crate::interface::help::fallback())?;
        }
        return Ok(0);
    };
    if command == "completion" && !args.switch("help") {
        return crate::interface::completion::command(&args);
    }
    if command == "version" {
        if args.positional.len() != 1 {
            return Err(Error::usage("version takes no positional arguments"));
        }
        print!(
            "{}",
            crate::foundation::version::render(args.switch("json"))
        );
        return Ok(0);
    }
    // `clerk help <name>` and `clerk <name> [verb] --help` reach the same page:
    // the page belongs to the command, a verb has no page of its own.
    if command == "help" || args.switch("help") {
        let target = if command == "help" {
            args.positional.get(1).cloned()
        } else {
            Some(command.clone())
        };
        if let Some(name) = target {
            let root = args.value("root").map(str::to_string);
            let context = Context::open(root.as_deref())?;
            crate::terminal::reader::help(
                Some(&context),
                &args,
                &crate::interface::help::selected_page(
                    &context,
                    &name,
                    args.positional
                        .get(if command == "help" { 2 } else { 1 })
                        .map(String::as_str),
                )?,
            )?;
            return Ok(0);
        }
        if let Ok(context) = Context::open(args.value("root")) {
            crate::terminal::reader::help(
                Some(&context),
                &args,
                &crate::interface::help::overview(&context),
            )?;
        } else {
            crate::terminal::reader::help(None, &args, &crate::interface::help::fallback())?;
        }
        return Ok(0);
    }
    let root = args.value("root").map(str::to_string);
    if let Some(mode) = args.value("sync") {
        if mode != "ff-only" {
            return Err(Error::usage("--sync needs ff-only"));
        }
        if args.switch("dry-run") {
            return Err(Error::usage(
                "--sync cannot be combined with --dry-run; run clerk sync update separately",
            ));
        }
    }
    // A unique prefix of a command resolves to it; an ambiguous one lists the
    // candidates, which keeps the short forms usable without a lookup table.
    let mut opened = Context::open(root.as_deref());
    let kinds: Vec<String> = match &opened {
        Ok(context) => context
            .catalog
            .kinds()
            .iter()
            .map(|kind| kind.name.clone())
            .collect(),
        Err(_) => Vec::new(),
    };
    let mut candidates: Vec<&str> = crate::interface::command::BUILTINS
        .iter()
        .map(|spec| spec.name)
        .collect();
    candidates.extend(kinds.iter().map(String::as_str));
    let command_name = match crate::interface::cli::resolve(&command, &candidates) {
        Ok(command) => command,
        Err(error) => match &opened {
            // A broken declaration hides the kinds, so the name looks unknown.
            // Report what actually failed instead of the symptom.
            Err(open_error) => {
                return Err(Error::usage(format!(
                    "the workspace did not load: {open_error}"
                )));
            }
            Ok(_) => return Err(error),
        },
    }
    .to_string();
    let command = command_name.as_str();
    args.positional[0] = command_name.clone();
    if let Some(requested) = args.positional.get(1).cloned() {
        if let Some(builtin) = crate::interface::command::builtin(command) {
            if !builtin.subcommands.is_empty() {
                args.positional[1] = resolve(&requested, builtin.subcommands)?.to_string();
            }
        } else if let Ok(context) = &opened
            && let Some(kind) = context.catalog.find(command)
        {
            let names = crate::interface::command::verb_names(kind);
            let candidates: Vec<&str> = names.iter().map(String::as_str).collect();
            if let Ok(verb) = resolve(&requested, &candidates) {
                args.positional[1] = verb.to_string();
            }
        }
    }
    if (matches!(command, "context" | "workflow" | "session")
        || (command == "links" && !args.switch("apply")))
        && args.value("sync").is_some()
    {
        return Err(Error::usage(
            "read-only context and workflow inspection never synchronize; run sync update explicitly",
        ));
    }
    if let Ok(context) = &opened {
        validate_options(context, command, &args)?;
        if command == "batch" {
            crate::execution::batch::preflight(context, &args)?;
        }
        let requested = args.positional.get(1).map(String::as_str).unwrap_or("");
        if args.value("step-name").is_some() && args.value("batch").is_none() {
            return Err(Error::usage("--step-name requires --batch"));
        }
        if args.value("batch").is_some()
            && (args.value("sync").is_some()
                || args.value("request").is_some()
                || args.value("from").is_some()
                || args.switch("dry-run"))
        {
            return Err(Error::usage(
                "--batch captures only; it cannot combine with --sync, --request, --from or --dry-run",
            ));
        }
        if command == "batch"
            && args.switch("preview")
            && (args.switch("dry-run") || args.value("sync").is_some())
        {
            return Err(Error::usage(
                "--preview cannot combine with --dry-run or --sync",
            ));
        }
        if command == "batch" && !matches!(requested, "" | "apply") && args.value("sync").is_some()
        {
            return Err(Error::usage(
                "local batch inspection never synchronizes; use sync update separately",
            ));
        }
        let writes = context
            .catalog
            .find(command)
            .is_some_and(|kind| crate::interface::command::writes(kind, requested))
            || matches!(command, "render" | "seal")
            || (command == "batch"
                && (requested.is_empty() || requested == "apply")
                && !args.switch("preview"))
            || (command == "links" && args.switch("apply"))
            || matches!(
                (command, requested),
                ("request", "apply" | "reject")
                    | ("people", "add" | "remove")
                    | ("focus", "set" | "add" | "remove")
            );
        configure_commit_message(context, &args, writes)?;
        if args.value("sync").is_some()
            || (writes
                && context.workspace.sync.write_update
                && args.value("request").is_none()
                && args.value("batch").is_none()
                && !args.switch("dry-run"))
        {
            crate::git::sync::update(&context.workspace)?;
            opened = Ok(Context::open(root.as_deref()).map_err(|error| Error::usage(format!("workspace failed to load after synchronization; business data was not written: {error}")))?);
        }
    }
    // Defaults fill only what the command line left out.
    if let Ok(context) = &opened {
        let verb = args.positional.get(1).cloned().unwrap_or_default();
        crate::interface::choice::normalize(context, command, &mut args)?;
        let mut defaults = context.command_defaults(command, verb.as_str());
        let json = args.switch("json")
            || (!args.off.contains("json") && defaults.iter().any(|(name, _)| name == "json"));
        if json || (command == "workflow" && verb == "preview") {
            defaults.retain(|(name, _)| !matches!(name.as_str(), "plain" | "width"));
        }
        crate::interface::cli::apply_defaults(&mut args, &defaults);
        validate_options(context, command, &args)?;
        crate::terminal::reader::validate(&args)?;
    }
    match command {
        "completion" => crate::interface::completion::command(&args),
        "context" => crate::collaboration::handoff::command(root.as_deref(), &args),
        "workflow" => crate::collaboration::workflow::templates_command(
            &Context::open(root.as_deref())?,
            &args,
        ),
        "batch" => {
            if args.value("request").is_some() {
                return Err(Error::usage(
                    "workflow does not support --request; use individual kind requests",
                ));
            }
            let mut context = Context::open(root.as_deref())?;
            crate::execution::batch::command(&mut context, &args)
        }
        "operation" => {
            let context = Context::open(root.as_deref())?;
            let id = args.positional.get(2).map(String::as_str);
            let report = match sub_command(&args, "operation")?.as_str() {
                "status" => crate::git::operation::load(&context.workspace, id)?,
                "retry" => {
                    if args.switch("dry-run") {
                        return Err(Error::usage(
                            "operation retry does not support --dry-run; use operation status",
                        ));
                    }
                    let report = crate::git::commit::retry(&context.workspace, id)?;
                    crate::execution::batch::cleanup(&context, &report.id);
                    report
                }
                _ => return Err(Error::usage("usage: clerk operation status|retry [id]")),
            };
            if args.switch("json") {
                println!("{}", report.json());
            } else {
                println!("{}", report.summary());
            }
            Ok(0)
        }
        "help" => unreachable!("help is dispatched before writes and synchronization"),
        "whoami" => {
            let mut context = Context::open(root.as_deref())?;
            commands::whoami(&mut context, &args)?;
            Ok(0)
        }
        "id" => {
            let context = Context::open(root.as_deref())?;
            let name = args
                .positional
                .get(1)
                .ok_or_else(|| Error::usage("usage: clerk id <name>"))?;
            crate::config::identity::write_operator(&context.workspace, name)?;
            println!("operator set to {name}");
            Ok(0)
        }
        "kinds" => {
            let context = Context::open(root.as_deref())?;
            commands::kinds(&context, &args)?;
            Ok(0)
        }
        "explain" => {
            let context = Context::open(root.as_deref())?;
            let kind = args
                .positional
                .get(1)
                .ok_or_else(|| Error::usage("usage: clerk explain <kind>"))?;
            commands::explain(&context, kind, &args)?;
            Ok(0)
        }
        "check" => {
            let mut context = Context::open(root.as_deref())?;
            commands::check(&mut context, &args)
        }
        "doctor" => {
            let mut context = Context::open(root.as_deref())?;
            commands::doctor(&mut context, &args)
        }
        "request" => {
            let mut context = Context::open(root.as_deref())?;
            let sub = sub_command(&args, "request")?;
            match sub.as_str() {
                "list" => {
                    commands::request_list(&context, &args)?;
                    Ok(0)
                }
                "show" => {
                    let name = args
                        .positional
                        .get(2)
                        .ok_or_else(|| Error::usage("usage: clerk request show <name>"))?;
                    commands::request_show(&context, name, &args)?;
                    Ok(0)
                }
                "apply" => commands::request_apply(&mut context, &args),
                "reject" => commands::request_reject(&mut context, &args),
                other => Err(Error::usage(format!(
                    "unknown request subcommand `{other}`; expected list, show, apply or reject"
                ))),
            }
        }
        "sync" => {
            let context = Context::open(root.as_deref())?;
            let sub = {
                let value = sub_command(&args, "sync")?;
                if value.is_empty() {
                    "check".to_string()
                } else {
                    value
                }
            };
            match sub.as_str() {
                "check" => {
                    let report = crate::git::sync::check(&context.workspace)?;
                    if report.unknown {
                        println!("{} is unknown; run clerk sync fetch", report.reference);
                    } else {
                        println!(
                            "{} behind {}, ahead {}",
                            report.reference, report.behind, report.ahead
                        );
                    }
                    Ok(0)
                }
                "update" => {
                    if args.switch("dry-run") {
                        return Err(Error::usage(
                            "sync update does not support --dry-run; use sync check",
                        ));
                    }
                    crate::git::sync::update(&context.workspace)?;
                    Context::open(root.as_deref())?;
                    let report = crate::git::sync::check(&context.workspace)?;
                    println!(
                        "workspace synchronized; local branch is {} commit(s) ahead; no push performed; business data not written",
                        report.ahead
                    );
                    Ok(0)
                }
                "fetch" => {
                    crate::git::sync::fetch(&context.workspace)?;
                    println!("updated {}", context.workspace.sync.remote);
                    Ok(0)
                }
                other => Err(Error::usage(format!(
                    "unknown sync subcommand `{other}`; expected check or fetch"
                ))),
            }
        }

        "seal" => {
            let mut context = Context::open(root.as_deref())?;
            commands::seal(&mut context, &args)
        }
        "init" => {
            let context = Context::open(root.as_deref())?;
            commands::init(&context, &args)
        }
        "place" => {
            let context = Context::open(root.as_deref())?;
            let name = args
                .positional
                .get(1)
                .ok_or_else(|| Error::usage("usage: clerk place <category>"))?;
            commands::place(&context, name)
        }
        "completions" => {
            let context = Context::open(root.as_deref())?;
            let shell = args
                .positional
                .get(1)
                .cloned()
                .unwrap_or_else(|| "bash".to_string());
            commands::completions(&context, &shell)
        }
        "people" => {
            let mut context = Context::open(root.as_deref())?;
            commands::people(&mut context, &args)
        }
        "repos" => {
            let context = Context::open(root.as_deref())?;
            commands::repos(&context, &args)
        }
        "brief" => {
            let context = Context::open(root.as_deref())?;
            commands::brief(&context, &args)
        }
        "focus" => {
            let mut context = Context::open(root.as_deref())?;
            commands::focus(&mut context, &args)
        }
        "links" => crate::collaboration::references::command(root.as_deref(), &args),
        "session" => {
            crate::collaboration::session::command(&Context::open(root.as_deref())?, &args)
        }
        "scope" => {
            let context = Context::open(root.as_deref())?;
            commands::scope(&context, &args)?;
            Ok(0)
        }
        "render" => {
            let mut context = Context::open(root.as_deref())?;
            if args.value("batch").is_some() {
                return crate::execution::batch::capture(
                    &mut context,
                    &args,
                    crate::collaboration::workflow::Step {
                        verb: "render".into(),
                        ..Default::default()
                    },
                );
            }
            let baseline = pipeline::begin(&mut context)?;
            let report = crate::execution::render::render(&context)?;
            let mut affected = report.changed.clone();
            let notes = if args.switch("dry-run") {
                drop(baseline);
                Vec::new()
            } else if affected.is_empty() {
                Vec::new()
            } else {
                pipeline::finish_paths(&mut context, "render", &mut affected, &[], baseline)?
            };
            if args.switch("json") {
                println!(
                    "{{\"blocks\":{},\"changed\":{},\"execution\":{}}}",
                    report.blocks,
                    report.changed.len(),
                    crate::git::operation::current()
                        .map(|report| report.json())
                        .unwrap_or_else(|| "null".into())
                );
                return Ok(0);
            }
            if !args.switch("quiet") {
                println!(
                    "{} block(s), {} file(s) changed",
                    report.blocks,
                    report.changed.len()
                );
            }
            for note in notes {
                println!("{note}");
            }
            Ok(0)
        }
        other => run_kind(root.as_deref(), other, &args),
    }
}

fn validate_options(context: &Context, command: &str, args: &Args) -> Result<()> {
    if args.switch("long") && args.switch("details") {
        return Err(Error::usage("--long and --details are mutually exclusive"));
    }
    let reading_options = args.value("width").is_some() || args.switch("plain");
    if reading_options {
        if args.switch("plain") && args.value("width").is_some() {
            return Err(Error::usage("--plain and --width are mutually exclusive"));
        }
        if args.switch("json")
            || (command == "workflow"
                && args.positional.get(1).is_some_and(|verb| verb == "preview"))
        {
            return Err(Error::usage(
                "terminal reading options cannot be combined with JSON or workflow preview",
            ));
        }
    }
    let kind = context.catalog.find(command);
    let scheme = kind
        .and_then(|kind| kind.numbering.as_ref())
        .map(|name| context.numbering(name))
        .transpose()?;
    for name in args
        .values
        .keys()
        .chain(args.switches.iter())
        .chain(args.off.iter())
    {
        let template_param =
            crate::collaboration::workflow::accepts_option(context, command, args, name)?;
        let allowed = template_param
            || match kind {
                Some(kind) => crate::interface::command::kind_argument(kind, scheme, name),
                None => crate::interface::command::builtin(command).is_some_and(|builtin| {
                    crate::interface::command::builtin_option(builtin, name)
                }),
            };
        if !allowed {
            return Err(Error::usage(format!(
                "unknown option `--{name}` for `{command}`; run `clerk {command} --help`"
            )));
        }
        if !if let Some(kind) = kind {
            crate::interface::command::kind_applies(
                kind,
                args.positional.get(1).map(String::as_str).unwrap_or(""),
                name,
            )
        } else {
            crate::interface::command::applies(
                name,
                command,
                args.positional.get(1).map(String::as_str).unwrap_or(""),
                false,
            )
        } {
            return Err(Error::usage(format!(
                "--{name} does not apply to `{}`",
                args.positional.join(" ")
            )));
        }
    }
    Ok(())
}

fn run_kind(root: Option<&str>, kind_name: &str, args: &Args) -> Result<i32> {
    let mut context = Context::open(root)?;
    let kind = context.catalog.require(kind_name)?.clone();
    let Some(verb) = args.positional.get(1).cloned() else {
        crate::terminal::reader::help(
            Some(&context),
            args,
            &crate::interface::help::usage_page(&kind),
        )?;
        return Ok(0);
    };
    // Kind verbs take the same prefix rule as commands.
    let verb_list = crate::interface::command::verb_names(&kind).join(", ");
    let verb_candidates: Vec<&str> = verb_list.split(", ").collect();
    let verb = match crate::interface::cli::resolve(&verb, &verb_candidates) {
        Ok(verb) => verb.to_string(),
        Err(error) if error.message().starts_with("unknown command") => {
            return Err(Error::usage(format!(
                "kind `{}` has no verb `{verb}`; run `clerk {}` for usage",
                kind.name, kind.name
            )));
        }
        Err(error) => return Err(error),
    };
    let aliases = context.workspace.aliases()?;
    let mut rest: Vec<String> = args
        .positional
        .iter()
        .skip(2)
        .map(|item| aliases.get(item).cloned().unwrap_or_else(|| item.clone()))
        .collect();
    let format = args.format();
    let columns = args.columns();
    let max_width = args.max_width();

    // Only verbs that write prose pull text from a pipe or the editor.
    let interactive = kind
        .action(&verb)
        .map(|action| action.handler == "attach" || action.handler == "append")
        .unwrap_or(false)
        || (verb == "new" && kind.document_dir().is_some() && kind.template.is_none());
    let (values, tokens) = args.payload(&kind);
    let options = Options {
        dry_run: args.switch("dry-run"),
        tokens: tokens.clone(),
        filters: args.filters(&kind),
        filter: args.value("filter").map(str::to_string),
        case: args.value("case").map(str::to_string),
        body: crate::interface::input::body(&context.workspace, args, interactive)?,
        scope: args.value("data-scope").unwrap_or("current").into(),
        display: args.switch("display"),
        force: args.switch("force"),
        long: args.switch("long"),
        mine: args.switch("mine"),
        claimed: args.switch("claimed"),
    };

    if verb == "show" {
        rest = crate::query::selection::resolve(&context, &kind, &rest, args, false)?;
    } else if args.value("match").is_some() {
        return Err(Error::usage("--match requires show or context"));
    }
    let writes = crate::interface::command::writes(&kind, &verb);
    if args.value("batch").is_some() {
        return crate::execution::batch::capture_kind(
            &mut context,
            args,
            &kind,
            &verb,
            rest,
            values,
            options,
        );
    }
    if let Some(name) = args.value("request") {
        if !writes {
            return Err(Error::usage(format!(
                "`--request` needs a write verb; `{verb}` does not write"
            )));
        }
        if args.value("from").is_some() || options.dry_run {
            return Err(Error::usage(
                "--request cannot be combined with --from or --dry-run",
            ));
        }
        let mut transaction =
            crate::execution::transaction::Transaction::begin(&context.workspace)?;
        engine::apply_provided(
            &context,
            &kind,
            &mut crate::storage::store::Record::new(),
            &values,
            verb == "new",
        )?;
        let identity = context.identity()?.clone();
        let path = crate::collaboration::request::capture(
            &context.workspace,
            &identity,
            crate::collaboration::request::NewRequest {
                kind: &kind.name,
                verb: &verb,
                keys: &rest,
                values: &values,
                tokens: &tokens,
                name,
                body: options.body.as_deref(),
                force: options.force,
            },
        )?;
        transaction.accept()?;
        println!("captured {}", context.workspace.relative(&path));
        return Ok(0);
    }
    // `new --from` creates one instance per row, in a single commit.
    if verb == "new"
        && let Some(source) = args.value("from")
    {
        return create_from(&mut context, &kind, source, &options, args);
    }
    let baseline = if writes {
        Some(pipeline::begin(&mut context)?)
    } else {
        None
    };

    let mut outcome = if verb == "verify" {
        engine::verify(&context, &kind.name, &options)?
    } else {
        crate::execution::application::execute(
            &mut context,
            &kind,
            &verb,
            &rest,
            &values,
            &options,
        )?
    };

    let view_sort = kind
        .views
        .iter()
        .find(|view| view.name == verb)
        .map(|view| view.sort.clone())
        .unwrap_or_default();
    let sort_keys: Vec<String> = args
        .value("sort")
        .map(|value| {
            value
                .split(',')
                .map(|key| key.trim().to_string())
                .filter(|key| !key.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let print = output::Print {
        format,
        columns,
        max_width,
        display: options.display,
        long: options.long,
        view_sort,
        sort: sort_keys,
        desc: args.switch("desc"),
        headers: headers_shown(args)?,
        compact: args.switch("compact"),
        width: args.value("width").and_then(|value| value.parse().ok()),
        plain: args.switch("plain"),
        details: args.switch("details"),
    };
    // Formatting errors must be found while rollback is still possible.
    output::render(&context, &kind, &outcome, &print)?;
    if let Some(baseline) = baseline {
        if options.dry_run {
            drop(baseline);
            outcome.dry_run = true;
        } else {
            outcome = pipeline::finish(&mut context, outcome, baseline)?;
        }
    }
    if !args.switch("quiet") {
        if matches!(outcome.verb.as_str(), "list" | "show") {
            let sorting = if outcome.verb == "list" && format == output::Format::Table {
                let mut candidates = kind
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect::<Vec<_>>();
                if let Some(scheme) = kind
                    .numbering
                    .as_deref()
                    .map(|name| context.numbering(name))
                    .transpose()?
                {
                    for field in &kind.key {
                        candidates.extend(
                            scheme
                                .part_names()
                                .iter()
                                .map(|part| format!("{field}:{part}")),
                        );
                    }
                }
                Some(crate::terminal::reader::Sorting {
                    keys: output::sort_keys(&context, &kind, &print)?,
                    desc: print.desc,
                    candidates,
                })
            } else {
                None
            };
            crate::terminal::reader::controlled(
                args,
                format != output::Format::Table,
                sorting,
                |width, sorting| {
                    let mut print = print.clone();
                    print.width = width;
                    if let Some(sorting) = sorting {
                        print.sort = sorting.keys.clone();
                        print.desc = sorting.desc;
                    }
                    output::document(&context, &kind, &outcome, &print)
                },
            )?;
        } else {
            print!("{}", output::render(&context, &kind, &outcome, &print)?);
        }
    }
    if !outcome.problems.is_empty() {
        for problem in &outcome.problems {
            println!("{problem}");
        }
        return Ok(1);
    }
    Ok(0)
}

/// Merge command defaults into the parsed flags, keeping what the user typed.
pub fn apply_defaults(args: &mut Args, defaults: &[(String, String)]) {
    for (name, value) in defaults {
        if args.off.contains(name) {
            continue;
        }
        if crate::interface::command::is_switch(name) {
            args.switches.insert(name.clone());
        } else if !args.values.contains_key(name) {
            args.values.insert(name.clone(), value.clone());
        }
    }
}

/// Create one instance per row of a csv whose first row names the fields.
fn create_from(
    context: &mut Context,
    kind: &Kind,
    source: &str,
    options: &engine::Options,
    args: &Args,
) -> Result<i32> {
    let body = if source == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .map_err(|error| Error::usage(format!("cannot read stdin: {error}")))?;
        text
    } else {
        let path = context.workspace.resolve(source);
        std::fs::read_to_string(&path)
            .map_err(|error| crate::foundation::error::io_error(&path, error))?
    };
    let rows = crate::foundation::csv::parse(&body)?;
    let Some(headers) = rows.first().cloned() else {
        return Err(Error::usage("--from needs a header row"));
    };
    for name in &headers {
        if kind.field(name).is_none() {
            return Err(Error::usage(format!(
                "--from: kind `{}` has no field `{name}`",
                kind.name
            )));
        }
    }
    let baseline = if options.dry_run {
        None
    } else {
        Some(pipeline::begin(context)?)
    };
    let mut outcome = engine::Outcome::new(kind, "new");
    let mut taken: Vec<String> = Vec::new();
    for (number, row) in rows.iter().skip(1).enumerate() {
        let mut values = engine::Values::new();
        for (index, name) in headers.iter().enumerate() {
            // Status fields are maintained by the tool; a column for one is
            // ignored rather than rejected, so an exported table can be reused.
            let status = kind
                .field(name)
                .map(|field| field.role == crate::config::kind::Role::Status)
                .unwrap_or(false);
            if status {
                continue;
            }
            values.insert(name.clone(), row.get(index).cloned().unwrap_or_default());
        }
        let created = engine::create_with(context, &kind.name, &values, options, &taken).map_err(
            |error| {
                Error::new(
                    error.failure(),
                    format!("--from row {}: {error}", number + 2),
                )
            },
        )?;
        if let Some(key) = created
            .records
            .first()
            .and_then(|record| kind.key.first().and_then(|name| record.get(name)).cloned())
        {
            taken.push(key);
        }
        outcome.records.extend(created.records);
        outcome.affected.extend(created.affected);
        outcome.notes.extend(created.notes);
    }
    let view_sort = kind
        .views
        .iter()
        .find(|view| view.name == "list")
        .map(|view| view.sort.clone())
        .unwrap_or_default();
    let print = output::Print {
        format: args.format(),
        columns: None,
        max_width: args.max_width(),
        display: options.display,
        long: false,
        view_sort,
        sort: Vec::new(),
        desc: false,
        headers: headers_shown(args)?,
        compact: args.switch("compact"),
        width: args.value("width").and_then(|value| value.parse().ok()),
        plain: args.switch("plain"),
        details: args.switch("details"),
    };
    output::render(context, kind, &outcome, &print)?;
    if let Some(baseline) = baseline {
        outcome = pipeline::finish(context, outcome, baseline)?;
    }
    let text = output::render(context, kind, &outcome, &print)?;
    if !args.switch("quiet") {
        print!("{text}");
    }
    Ok(0)
}
