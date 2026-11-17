//! Shared command metadata for parsing, help, defaults and completion.

use crate::config::kind::Kind;
use crate::foundation::error::{Error, Result};

/// One option, including its spelling, type and default eligibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Text,
    Width,
    TerminalWidth,
    Headers,
    Overflow,
    List,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Negation {
    Reset,
    Value(&'static str),
}

pub struct OptionSpec {
    pub name: &'static str,
    pub value: Option<ValueKind>,
    pub short: Option<char>,
    pub suffix: &'static str,
    pub about: &'static str,
    pub global: bool,
    pub default: bool,
    pub reserved: bool,
    pub negation: Option<Negation>,
}

impl OptionSpec {
    pub fn switch(&self) -> bool {
        self.value.is_none()
    }

    pub fn label(&self) -> String {
        match self.short {
            Some(short) => format!("-{short}, --{}{}", self.name, self.suffix),
            None => format!("--{}{}", self.name, self.suffix),
        }
    }

    /// A compact placeholder; candidates are separate help and schema data.
    pub fn argument(&self) -> String {
        if self.switch() {
            return String::new();
        }
        if choices(self.name).is_empty() {
            return self.suffix.trim().to_string();
        }
        format!(
            "<{}>",
            match self.name {
                "data-scope" => "scope",
                "input-format" => "format",
                "color" => "when",
                _ => "mode",
            }
        )
    }

    pub fn validate(&self, text: &str) -> Result<()> {
        let choices = choices(self.name);
        if !choices.is_empty() && !choices.contains(&text) {
            return Err(Error::usage(format!(
                "--{} needs {}",
                self.name,
                choices.join(", ")
            )));
        }
        match self.value {
            Some(ValueKind::Overflow)
                if !matches!(text, "truncate" | "more" | "pager" | "full") =>
            {
                Err(Error::usage(
                    "--overflow needs truncate, more, pager or full",
                ))
            }
            Some(ValueKind::Headers) if !matches!(text, "show" | "hide") => {
                Err(Error::usage("--headers needs show or hide"))
            }
            Some(ValueKind::Width) if text.parse::<usize>().is_err() => {
                Err(Error::usage("--max-width needs a non-negative integer"))
            }
            Some(ValueKind::TerminalWidth)
                if !text
                    .parse::<usize>()
                    .is_ok_and(|width| (20..=10000).contains(&width)) =>
            {
                Err(Error::usage(
                    "--width needs an integer from 20 to 10000 terminal columns",
                ))
            }
            _ => Ok(()),
        }
    }
}

/// Enumerated inputs are shared with completion and configuration validation.
pub fn choices(name: &str) -> &'static [&'static str] {
    match name {
        "paging" => &["auto", "always", "never"],
        "color" => &["auto", "always", "never"],
        "case" => &["ignore", "sensitive"],
        "search-mode" => &["literal", "regex"],
        "match" => &["exact", "prefix", "contains", "regex"],
        "overflow" => &["full", "truncate", "pager", "more"],
        "headers" => &["show", "hide"],
        "data-scope" => &["current", "archived", "all"],
        "input-format" => &["toml", "json"],
        "sync" => &["ff-only"],
        _ => &[],
    }
}

pub fn resolve_choice(name: &str, value: &str) -> Result<String> {
    resolve_value(name, value, choices(name))
}

/// Exact keys win; abbreviations must identify exactly one declared candidate.
pub fn resolve_value(name: &str, value: &str, candidates: &[&str]) -> Result<String> {
    if value.is_empty() {
        return Err(Error::usage(format!(
            "--{name} needs a non-empty value; expected {}",
            candidates.join(", ")
        )));
    }
    if candidates.contains(&value) {
        return Ok(value.into());
    }
    let hits = candidates
        .iter()
        .filter(|candidate| candidate.starts_with(value))
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    match hits.as_slice() {
        [only] => Ok((*only).into()),
        [] => Err(Error::usage(format!(
            "unknown --{name} value `{value}`; expected {}",
            candidates.join(", ")
        ))),
        _ => Err(Error::usage(format!(
            "--{name} value `{value}` is ambiguous: {}",
            hits.join(", ")
        ))),
    }
}

/// Canonical initial values shared with the help projection.
pub fn initial(name: &str) -> Option<&'static str> {
    match name {
        "paging" | "color" => Some("auto"),
        "overflow" => Some("full"),
        "search-mode" => Some("literal"),
        "match" => Some("exact"),
        "headers" => Some("show"),
        "data-scope" => Some("current"),
        "input-format" => Some("toml"),
        _ => None,
    }
}

pub fn about(command: &str, name: &str) -> &'static str {
    match (command, name) {
        ("batch", "file") => "read a workflow from a file, or from stdin",
        _ => option(name).map(|option| option.about).unwrap_or(""),
    }
}

/// Reading order within a help group; unknown declaration inputs follow builtins.
pub fn group_order(group: &str) -> usize {
    match group {
        "Data scope" => 0,
        "Input" => 1,
        "Query and order" => 2,
        "Content" => 3,
        "Presentation" | "Output" => 4,
        "Reading" => 5,
        "Write and recovery" => 6,
        "General" => 7,
        _ => 8,
    }
}

pub fn help_order(name: &str) -> usize {
    let names = [
        "data-scope",
        "file",
        "template",
        "input-format",
        "filter",
        "mine",
        "claimed",
        "sort",
        "desc",
        "match",
        "case",
        "full",
        "columns",
        "long",
        "details",
        "display",
        "compact",
        "width",
        "max-width",
        "headers",
        "json",
        "tsv",
        "csv",
        "md",
        "color",
        "plain",
        "overflow",
        "paging",
        "search-mode",
        "message",
        "message-file",
        "batch",
        "step-name",
        "request",
        "preview",
        "dry-run",
        "sync",
        "force",
        "body",
        "from",
        "root",
        "as",
        "quiet",
        "help",
        "version",
    ];
    names
        .iter()
        .position(|item| *item == name)
        .unwrap_or(names.len())
}

pub fn builtin_group(name: &str) -> &'static str {
    match name {
        "context" | "kinds" | "explain" | "brief" | "repos" | "place" | "scope" => "Inspect",
        "batch" | "workflow" | "operation" | "request" | "focus" => "Workflows",
        "session" | "whoami" | "id" | "people" => "Collaboration",
        "check" | "doctor" | "render" | "seal" | "sync" | "init" | "links" => "Maintenance",
        _ => "Help and integration",
    }
}

pub fn requirements(name: &str, value: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match (name, value) {
        ("paging", "always") => &[("overflow", &["pager", "more"])],
        _ => &[],
    }
}

pub fn conflicts(name: &str) -> &'static [&'static str] {
    match name {
        "long" => &["details"],
        "details" => &["long"],
        "plain" => &["width"],
        "width" => &["plain"],
        _ => &[],
    }
}

pub fn validate_requirements(values: &std::collections::BTreeMap<String, String>) -> Result<()> {
    for (name, value) in values {
        for (required, allowed) in requirements(name, value) {
            if !values
                .get(*required)
                .is_some_and(|value| allowed.contains(&value.as_str()))
            {
                return Err(Error::usage(format!(
                    "--{name} {value} requires --{required} {}",
                    allowed.join(" or ")
                )));
            }
        }
    }
    Ok(())
}

pub fn group(name: &str) -> &'static str {
    match name {
        "data-scope" => "Data scope",
        "filter" | "mine" | "claimed" | "sort" | "desc" | "match" | "case" => "Query and order",
        "full" => "Content",
        "columns" | "long" | "details" | "display" | "headers" | "json" | "tsv" | "csv" | "md"
        | "compact" | "max-width" | "width" | "plain" | "color" => "Presentation",
        "paging" | "overflow" | "search-mode" => "Reading",
        "message" | "message-file" | "sync" | "dry-run" | "request" | "batch" | "step-name"
        | "force" | "body" | "file" | "from" | "preview" => "Write and recovery",
        _ => "General",
    }
}

/// Additional semantic restrictions shared by help, dispatch and completion.
pub fn applies(name: &str, command: &str, verb: &str, kind: bool) -> bool {
    if command == "batch" {
        match name {
            "request" | "body" | "from" | "force" | "batch" | "step-name" => return false,
            "file" | "template" | "input-format" => return verb.is_empty(),
            "preview" | "message" | "message-file" | "sync" | "dry-run" => {
                return verb.is_empty() || verb == "apply";
            }
            _ => {}
        }
    }
    if kind && verb.is_empty() {
        return true;
    }
    let reads = kind && matches!(verb, "list" | "show");
    let writes = if kind {
        !reads
    } else {
        matches!(command, "render" | "seal" | "init" | "id")
            || (command == "batch" && !matches!(verb, "show" | "list"))
            || matches!(
                (command, verb),
                ("request", "apply" | "reject")
                    | ("people", "add" | "remove")
                    | ("focus", "set" | "add" | "remove")
            )
            || (command == "links" && verb == "repair")
    };
    let reader = if kind {
        reads
    } else {
        matches!(
            command,
            "help"
                | "context"
                | "kinds"
                | "whoami"
                | "explain"
                | "repos"
                | "place"
                | "scope"
                | "check"
                | "doctor"
                | "links"
        ) || matches!(
            (command, verb),
            ("workflow", "list" | "show" | "")
                | ("session", "list" | "show" | "")
                | ("focus", "show" | "")
                | ("people", "list" | "")
                | ("request", "list" | "")
        )
    };
    match name {
        "data-scope" => reads,
        "filter" | "mine" | "claimed" | "sort" | "desc" | "long" | "details" => {
            kind && verb == "list"
        }
        "match" => (kind && verb == "show") || command == "context",
        "paging" | "overflow" | "search-mode" | "width" | "plain" => reader,
        "case" => reader,
        "full" => command == "context",
        "body" | "file" | "from" | "force" | "request" | "batch" | "step-name" | "message"
        | "message-file" | "dry-run" | "sync" => writes,
        _ => true,
    }
}

pub fn kind_applies(kind: &Kind, verb: &str, name: &str) -> bool {
    if !applies(name, &kind.name, verb, true) {
        return false;
    }
    if verb.is_empty() {
        return true;
    }
    if kind.field(name).is_some() && !reserved(name) {
        return verb == "list"
            || matches!(verb, "new" | "set")
            || kind.transition(verb).is_some()
            || kind
                .action(verb)
                .is_some_and(|action| action_inputs(&action.handler).contains(&name));
    }
    match name {
        "from" | "step-name" => verb == "new",
        "force" => verb == "delete",
        "body" | "file" => {
            (verb == "new" && kind.document_dir().is_some())
                || kind
                    .action(verb)
                    .is_some_and(|action| action_inputs(&action.handler).contains(&name))
        }
        _ => true,
    }
}

/// Contextual candidates for the generic negation operator.
pub fn negatable(command: &str, verb: &str, kind: Option<&Kind>) -> Vec<&'static OptionSpec> {
    OPTIONS
        .iter()
        .filter(|spec| {
            spec.negation.is_some()
                && if let Some(kind) = kind {
                    kind_option(kind, spec.name) && kind_applies(kind, verb, spec.name)
                } else {
                    builtin(command).is_some_and(|builtin| builtin_option(builtin, spec.name))
                        && applies(spec.name, command, verb, false)
                }
        })
        .collect()
}

pub const OPTIONS: &[OptionSpec] = &[
    OptionSpec {
        name: "no",
        value: Some(ValueKind::Text),
        short: Some('N'),
        suffix: " <option>",
        about: "disable an option or suppress its configured default; accepts unique prefixes",
        global: true,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "paging",
        value: Some(ValueKind::Text),
        short: Some('p'),
        suffix: " <auto|always|never>",
        about: "enter a reader automatically, always or never",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Value("never")),
    },
    OptionSpec {
        name: "color",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <auto|always|never>",
        about: "choose when terminal colors are used",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Value("never")),
    },
    OptionSpec {
        name: "case",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <ignore|sensitive>",
        about: "text matching case policy",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "search-mode",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <literal|regex>",
        about: "choose literal or regular-expression search",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "match",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <exact|prefix|contains|regex>",
        about: "select a record by its identifier",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "words-json",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <json>",
        about: "completion context words",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "current",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <word>",
        about: "completion word at cursor",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "overflow",
        value: Some(ValueKind::Overflow),
        short: Some('o'),
        suffix: " <truncate|more|pager|full>",
        about: "choose how oversized terminal output is displayed",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "batch",
        value: Some(ValueKind::Text),
        short: Some('b'),
        suffix: " <name>",
        about: "append this write to a local draft without executing",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "step-name",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <alias>",
        about: "name a captured creation for later @alias references",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "preview",
        value: None,
        short: Some('P'),
        suffix: "",
        about: "validate and display the plan without executing",
        global: false,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "input-format",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <toml|json>",
        about: "format of the workflow input",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "plain",
        value: None,
        short: None,
        suffix: "",
        about: "disable terminal wrapping and excerpt line numbers",
        global: false,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "full",
        value: None,
        short: Some('F'),
        suffix: "",
        about: "include complete context excerpt bodies; keep relation and log counts",
        global: false,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "details",
        value: None,
        short: Some('D'),
        suffix: "",
        about: "all fields in vertical detail blocks; --columns overrides",
        global: false,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "width",
        value: Some(ValueKind::TerminalWidth),
        short: Some('w'),
        suffix: " <columns>",
        about: "total terminal reading width, 20 to 10000; enables reading layout",
        global: false,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "message",
        value: Some(ValueKind::Text),
        short: Some('m'),
        suffix: " <text>",
        about: "Git commit message for this write transaction",
        global: true,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "message-file",
        value: Some(ValueKind::Text),
        short: Some('M'),
        suffix: " <path|->",
        about: "read the Git commit message as UTF-8; exclusive with --message",
        global: true,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "tasks",
        value: Some(ValueKind::List),
        short: None,
        suffix: " <ids>",
        about: "explicit per-entry task references or session tasks",
        global: false,
        default: false,
        reserved: false,
        negation: None,
    },
    OptionSpec {
        name: "chat",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <id>",
        about: "conversation identity for a local session",
        global: false,
        default: false,
        reserved: false,
        negation: None,
    },
    OptionSpec {
        name: "paths",
        value: Some(ValueKind::List),
        short: None,
        suffix: " <paths>",
        about: "comma separated repository-relative session paths",
        global: false,
        default: false,
        reserved: false,
        negation: None,
    },
    OptionSpec {
        name: "ttl",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <seconds>",
        about: "session validity, 1 to 86400 seconds; default 1800",
        global: false,
        default: false,
        reserved: false,
        negation: None,
    },
    OptionSpec {
        name: "add",
        value: Some(ValueKind::List),
        short: None,
        suffix: " <paths>",
        about: "add session paths",
        global: false,
        default: false,
        reserved: false,
        negation: None,
    },
    OptionSpec {
        name: "remove",
        value: Some(ValueKind::List),
        short: None,
        suffix: " <paths>",
        about: "remove session paths",
        global: false,
        default: false,
        reserved: false,
        negation: None,
    },
    OptionSpec {
        name: "apply",
        value: None,
        short: None,
        suffix: "",
        about: "apply the reviewed reference repair plan",
        global: false,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "linkify",
        value: None,
        short: None,
        suffix: "",
        about: "convert confirmed inline file addresses to Markdown links",
        global: false,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "strict",
        value: None,
        short: None,
        suffix: "",
        about: "refuse overlapping active session scopes",
        global: false,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "compact",
        value: None,
        short: None,
        suffix: "",
        about: "omit padding in terminal tables and detail blocks",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "sync",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <ff-only>",
        about: "safely fast-forward before execution",
        global: true,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "root",
        value: Some(ValueKind::Text),
        short: Some('C'),
        suffix: " <path>",
        about: "workspace root",
        global: true,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "as",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <name>",
        about: "operator for this command",
        global: true,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "columns",
        value: Some(ValueKind::List),
        short: Some('c'),
        suffix: " <fields>",
        about: "override the printed columns",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "sort",
        value: Some(ValueKind::List),
        short: Some('s'),
        suffix: " <keys>",
        about: "sort by comma separated keys, e.g. state,id:prefix",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "desc",
        value: None,
        short: Some('r'),
        suffix: "",
        about: "reverse the order",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "long",
        value: None,
        short: Some('l'),
        suffix: "",
        about: "declared extended list fitted to terminal width; --columns overrides",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "display",
        value: None,
        short: Some('d'),
        suffix: "",
        about: "print display text instead of keys",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "filter",
        value: Some(ValueKind::Text),
        short: Some('f'),
        suffix: " <expr>",
        about: "condition, e.g. state = done or title ~ text",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "max-width",
        value: Some(ValueKind::Width),
        short: None,
        suffix: " <n>",
        about: "cap the printed width",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "headers",
        value: Some(ValueKind::Headers),
        short: None,
        suffix: " show|hide",
        about: "print the heading row of a table",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Value("hide")),
    },
    OptionSpec {
        name: "json",
        value: None,
        short: None,
        suffix: "",
        about: "JSON output",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "tsv",
        value: None,
        short: None,
        suffix: "",
        about: "tab separated output",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "csv",
        value: None,
        short: None,
        suffix: "",
        about: "comma separated output",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "md",
        value: None,
        short: None,
        suffix: "",
        about: "markdown table, ready to paste into a page",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "quiet",
        value: None,
        short: Some('q'),
        suffix: "",
        about: "print nothing when the command succeeds",
        global: true,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "dry-run",
        value: None,
        short: Some('n'),
        suffix: "",
        about: "report without retaining changes",
        global: true,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "request",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <name>",
        about: "capture the write as a request",
        global: true,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "help",
        value: None,
        short: Some('h'),
        suffix: "",
        about: "show command help",
        global: true,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "version",
        value: None,
        short: Some('v'),
        suffix: "",
        about: "show the tool version",
        global: true,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "mine",
        value: None,
        short: None,
        suffix: "",
        about: "only instances claimed by this identity",
        global: false,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "claimed",
        value: None,
        short: None,
        suffix: "",
        about: "only instances with a claimant",
        global: false,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "data-scope",
        value: Some(ValueKind::Text),
        short: Some('S'),
        suffix: " <current|archived|all>",
        about: "select current or archived records",
        global: false,
        default: true,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "force",
        value: None,
        short: None,
        suffix: "",
        about: "allow a destructive verb",
        global: false,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "check",
        value: None,
        short: None,
        suffix: "",
        about: "also locate the local checkout",
        global: false,
        default: false,
        reserved: true,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "body",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <text>",
        about: "body text, instead of --file",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "file",
        value: Some(ValueKind::Text),
        short: Some('i'),
        suffix: " <path|->",
        about: "read the body from a file, or from stdin",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "template",
        value: Some(ValueKind::Text),
        short: Some('t'),
        suffix: " <name>",
        about: "expand a declared workflow template",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "from",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <path|->",
        about: "create one instance per CSV row",
        global: false,
        default: false,
        reserved: true,
        negation: None,
    },
    OptionSpec {
        name: "title",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <text>",
        about: "section title an append action writes",
        global: false,
        default: true,
        reserved: false,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "heading",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <text>",
        about: "section title when --title is absent",
        global: false,
        default: true,
        reserved: false,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "side",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <name>",
        about: "maintainer of the log page",
        global: false,
        default: true,
        reserved: false,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "maintainer",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <name>",
        about: "maintainer when --side is absent",
        global: false,
        default: true,
        reserved: false,
        negation: Some(Negation::Reset),
    },
    OptionSpec {
        name: "before",
        value: Some(ValueKind::Text),
        short: None,
        suffix: " <YYYYMMDD>",
        about: "move earlier pages to the archive",
        global: false,
        default: true,
        reserved: false,
        negation: Some(Negation::Reset),
    },
];

pub fn option(name: &str) -> Option<&'static OptionSpec> {
    OPTIONS.iter().find(|spec| spec.name == name)
}
pub fn is_switch(name: &str) -> bool {
    option(name).is_some_and(OptionSpec::switch)
}
pub fn reserved(name: &str) -> bool {
    option(name).is_some_and(|spec| spec.reserved)
}
pub fn long_of_short(short: char) -> Option<&'static str> {
    OPTIONS
        .iter()
        .find(|spec| spec.short == Some(short))
        .map(|spec| spec.name)
}

/// Split short switches until the first value option consumes the remaining text.
/// A missing value is returned as None so callers can handle separate tokens
/// or an incomplete shell-completion input using the same lexical rules.
pub fn short_options(body: &str) -> Result<Vec<(&'static str, Option<&str>)>> {
    let mut out = Vec::new();
    for (index, short) in body.char_indices() {
        let name = long_of_short(short)
            .ok_or_else(|| Error::usage(format!("unknown option `-{short}`")))?;
        let rest = &body[index + short.len_utf8()..];
        if !is_switch(name) {
            out.push((
                name,
                if rest.is_empty() {
                    None
                } else {
                    Some(rest.strip_prefix('=').unwrap_or(rest))
                },
            ));
            return Ok(out);
        }
        if rest.starts_with('=') {
            return Err(Error::usage(format!(
                "`-{short}` takes no value; use --no-{name} to turn it off"
            )));
        }
        out.push((name, None));
    }
    Ok(out)
}

pub fn builtin(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|spec| spec.name == name)
}

/// Kind-specific prose inputs are derived from the declared storage and handlers.
pub fn kind_option(kind: &Kind, name: &str) -> bool {
    if name == "tasks" {
        return kind
            .actions
            .iter()
            .any(|action| action.entry_target.is_some());
    }
    if option(name).is_some_and(|spec| spec.global) {
        return true;
    }
    if kind
        .actions
        .iter()
        .any(|action| action_inputs(&action.handler).contains(&name))
    {
        return true;
    }
    match name {
        "details" => true,
        "plain" | "width" => true,
        "data-scope" | "mine" | "claimed" | "force" | "batch" | "step-name" => true,
        "body" | "file" => kind.document_dir().is_some(),
        "from" => kind.document_dir().is_none(),
        _ => false,
    }
}

/// Inputs understood by each application action handler.
pub fn action_inputs(handler: &str) -> &'static [&'static str] {
    match handler {
        "attach" => &["body", "file"],
        "append" => &[
            "body",
            "file",
            "title",
            "heading",
            "side",
            "maintainer",
            "tasks",
        ],
        "prune" => &["before"],
        _ => &[],
    }
}

pub fn kind_argument(
    kind: &Kind,
    scheme: Option<&crate::storage::numbering::Scheme>,
    name: &str,
) -> bool {
    kind.field(name).is_some()
        || kind_option(kind, name)
        || scheme.is_some_and(|scheme| scheme.tokens.contains_key(name))
}

/// Replay and direct execution reject the same unknown action/numbering inputs.
pub fn validate_tokens(
    kind: &Kind,
    scheme: Option<&crate::storage::numbering::Scheme>,
    tokens: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    for name in tokens.keys() {
        let numbered = scheme.is_some_and(|scheme| scheme.tokens.contains_key(name));
        if !numbered && (reserved(name) || kind.field(name).is_some() || !kind_option(kind, name)) {
            return Err(Error::usage(format!(
                "unknown token `--{name}` for kind `{}`; use a declared numbering token or action input",
                kind.name
            )));
        }
    }
    Ok(())
}

/// Builtin inputs come from the same usage/options entries as their help.
pub fn builtin_option(builtin: &Builtin, name: &str) -> bool {
    option(name).is_some_and(|spec| spec.global)
        || builtin
            .usage
            .iter()
            .copied()
            .chain(builtin.options.iter().map(|(usage, _)| *usage))
            .flat_map(str::split_whitespace)
            .any(|token| {
                token
                    .trim_start_matches('[')
                    .strip_prefix("--")
                    .is_some_and(|token| token.trim_end_matches(']') == name)
            })
}

pub struct Verb {
    pub name: String,
    pub about: String,
    pub group: &'static str,
    pub writes: bool,
}

pub fn verbs(kind: &Kind) -> Vec<Verb> {
    let base = [
        ("list", "print the instances", "Read", false),
        ("show", "print one instance in detail", "Read", false),
        (
            "new",
            "create an instance; --from adds one per CSV row",
            "Write",
            true,
        ),
        ("set", "change fields of an instance", "Write", true),
        ("delete", "remove an instance; needs --force", "Write", true),
        (
            "archive",
            "move a finished instance to the archive",
            "Write",
            true,
        ),
        ("restore", "bring an archived instance back", "Write", true),
    ];
    let mut out: Vec<Verb> = base
        .into_iter()
        .map(|(name, about, group, writes)| Verb {
            name: name.into(),
            about: about.into(),
            group,
            writes,
        })
        .collect();
    out.extend(kind.transitions.iter().map(|spec| Verb {
        name: spec.name.clone(),
        about: format!("move the state to {}", spec.to),
        group: "Transitions",
        writes: true,
    }));
    out.extend(kind.actions.iter().map(|spec| Verb {
        name: spec.name.clone(),
        about: format!("run the {} handler", spec.handler),
        group: "Actions",
        writes: true,
    }));
    if kind.hash.is_some() {
        out.push(Verb {
            name: "verify".into(),
            about: "recompute content hashes".into(),
            group: "Verify",
            writes: false,
        });
    }
    if !kind.relations.is_empty() {
        for (name, about) in [
            ("link", "add a typed relation"),
            ("unlink", "remove a relation"),
        ] {
            out.push(Verb {
                name: name.into(),
                about: about.into(),
                group: "Relations",
                writes: true,
            });
        }
    }
    out
}

pub fn verb_names(kind: &Kind) -> Vec<String> {
    verbs(kind).into_iter().map(|verb| verb.name).collect()
}
pub fn writes(kind: &Kind, verb: &str) -> bool {
    verbs(kind)
        .iter()
        .any(|spec| spec.name == verb && spec.writes)
}

/// The one conversion of declaration defaults into command argument values.
pub fn default_value(name: &str, value: &crate::config::decl::Value) -> Option<String> {
    use crate::config::decl::Value;
    match value {
        Value::Boolean(flag) if is_switch(name) => flag.then(String::new),
        Value::Boolean(flag) => Some(flag.to_string()),
        Value::String(text) => Some(text.clone()),
        Value::Integer(number) => Some(number.to_string()),
        Value::Array(items) => Some(
            items
                .iter()
                .map(|item| match item {
                    Value::String(text) => text.clone(),
                    Value::Integer(number) => number.to_string(),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join(","),
        ),
        _ => None,
    }
}

/// Static description of one built in command.
pub struct Builtin {
    pub name: &'static str,
    pub subcommands: &'static [&'static str],
    pub summary: &'static str,
    pub usage: &'static [&'static str],
    pub options: &'static [(&'static str, &'static str)],
    pub examples: &'static [&'static str],
}

/// Every built in command, in the order the overview lists them.
pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "completion",
        subcommands: &["powershell", "complete"],
        summary: "generate shell integration or query read-only completion candidates",
        usage: &[
            "clerk completion powershell",
            "clerk completion complete --words-json <json> --current <word> --json",
        ],
        options: &[
            ("--words-json <json>", "complete tokens before the cursor"),
            ("--current <word>", "current token"),
        ],
        examples: &["clerk completion powershell"],
    },
    Builtin {
        name: "session",
        subcommands: &["open", "touch", "scope", "close", "list", "show"],
        summary: "register local expiring collaboration scopes",
        usage: &[
            "clerk session open --chat <id> --tasks <ids> --paths <paths> [--ttl <seconds>] [--strict]",
            "clerk session <touch|scope|close|show> <id> [--add <paths>] [--remove <paths>] [--ttl <seconds>] [--strict]",
            "clerk session list",
        ],
        options: &[],
        examples: &[
            "clerk session open --chat reservation --tasks ENG-001 --paths src/reservation",
            "clerk session touch SESSION-ID",
        ],
    },
    Builtin {
        name: "context",
        subcommands: &[],
        summary: "collect a sourced, read-only record handoff bundle",
        usage: &["clerk context <kind> <key> [--json]"],
        options: &[
            ("--plain", "disable wrapping and line numbers"),
            (
                "--full",
                "include complete excerpt bodies; keep relation and log counts",
            ),
            ("--width <columns>", "total reading width"),
        ],
        examples: &["clerk context tasks ENG-021"],
    },
    Builtin {
        name: "workflow",
        subcommands: &["list", "show", "preview"],
        summary: "inspect declared workflow templates or expand parameters",
        usage: &[
            "clerk workflow list [--json]",
            "clerk workflow show <name> [--json]",
            "clerk workflow preview <name> [--<param> <value>...] [--json]",
        ],
        options: &[
            ("--plain", "disable wrapping and line numbers for list/show"),
            ("--width <columns>", "total reading width for list/show"),
        ],
        examples: &["clerk workflow preview start --task ENG-021"],
    },
    Builtin {
        name: "batch",
        subcommands: &["show", "apply", "list", "drop"],
        summary: "apply a structured workflow in one transaction",
        usage: &[
            "clerk batch --file <path|-> [--dry-run] [--json]",
            "clerk batch --template <name> [--<param> <value>...] [--dry-run] [--json]",
            "clerk batch show <name> [--json]",
            "clerk batch apply <name> [--preview|--dry-run] [--json]",
            "clerk batch list [--json]",
            "clerk batch drop <name> [--json]",
        ],
        options: &[
            ("--preview", "display the expanded plan only"),
            ("--input-format <toml|json>", "input format"),
        ],
        examples: &["clerk batch --file workflow.toml --dry-run"],
    },
    Builtin {
        name: "operation",
        subcommands: &["status", "retry"],
        summary: "inspect local execution state or retry Git finalization only",
        usage: &[
            "clerk operation status [id] [--json]",
            "clerk operation retry [id] [--json]",
        ],
        options: &[],
        examples: &["clerk operation status", "clerk operation retry"],
    },
    Builtin {
        name: "whoami",
        subcommands: &[],
        summary: "print the operator, the person and the git identity",
        usage: &["clerk whoami [--as <name>] [--json]"],
        options: &[
            ("--as <name>", "override the operator for this command"),
            ("--json", "machine readable output"),
        ],
        examples: &["clerk whoami"],
    },
    Builtin {
        name: "id",
        subcommands: &[],
        summary: "declare the operator for this clone",
        usage: &["clerk id <name>"],
        options: &[],
        examples: &["clerk id alice"],
    },
    Builtin {
        name: "kinds",
        subcommands: &[],
        summary: "list the declared kinds",
        usage: &["clerk kinds [--json]"],
        options: &[("--json", "machine readable output")],
        examples: &["clerk kinds"],
    },
    Builtin {
        name: "explain",
        subcommands: &[],
        summary: "show where one kind is declared and stored",
        usage: &["clerk explain <kind>"],
        options: &[],
        examples: &["clerk explain tasks"],
    },
    Builtin {
        name: "check",
        subcommands: &[],
        summary: "verify seals, values, documents, tables and pages",
        usage: &["clerk check [--json]"],
        options: &[
            ("--json", "machine readable problems"),
            ("--max-width <n>", "cap the printed width"),
        ],
        examples: &["clerk check"],
    },
    Builtin {
        name: "doctor",
        subcommands: &[],
        summary: "report the workspace, tables and documents",
        usage: &["clerk doctor [--json]"],
        options: &[("--json", "machine readable report")],
        examples: &["clerk doctor"],
    },
    Builtin {
        name: "render",
        subcommands: &[],
        summary: "regenerate the managed regions of the declared pages",
        usage: &["clerk render [--dry-run]"],
        options: &[("--batch <name>", "capture rendering in a local draft")],
        examples: &["clerk render"],
    },
    Builtin {
        name: "seal",
        subcommands: &[],
        summary: "rebuild the seal table and note why",
        usage: &["clerk seal --note <text>"],
        options: &[("--note <text>", "reason for the reseal, required")],
        examples: &["clerk seal --note manual fix"],
    },
    Builtin {
        name: "scope",
        subcommands: &[],
        summary: "list managed paths by governance level",
        usage: &["clerk scope [path]"],
        options: &[("--json", "machine readable rows")],
        examples: &["clerk scope", "clerk scope .clerk/data"],
    },
    Builtin {
        name: "sync",
        subcommands: &["check", "fetch", "update"],
        summary: "compare managed data with the remote branch",
        usage: &["clerk sync check", "clerk sync fetch", "clerk sync update"],
        options: &[],
        examples: &["clerk sync check"],
    },
    Builtin {
        name: "request",
        subcommands: &["list", "show", "apply", "reject"],
        summary: "inspect and apply the pending write requests",
        usage: &[
            "clerk request list",
            "clerk request show <name>",
            "clerk request apply [name...] [--dry-run]",
            "clerk request reject <name> [--reason <text>]",
        ],
        options: &[("--dry-run", "show what would be applied")],
        examples: &["clerk request list", "clerk request apply --dry-run"],
    },
    Builtin {
        name: "people",
        subcommands: &["list", "add", "remove"],
        summary: "list, add or remove the account to person mapping",
        usage: &[
            "clerk people list",
            "clerk people add --person <name> --email <mail> [--name <display>]",
            "clerk people remove --email <mail>",
        ],
        options: &[
            ("--person <name>", "person the account belongs to"),
            ("--email <mail>", "git account email"),
            ("--name <display>", "display name for the account"),
        ],
        examples: &["clerk people list"],
    },
    Builtin {
        name: "repos",
        subcommands: &[],
        summary: "list the repository registry",
        usage: &["clerk repos [--check]"],
        options: &[("--check", "also locate the local checkout")],
        examples: &["clerk repos --check"],
    },
    Builtin {
        name: "brief",
        subcommands: &[],
        summary: "summarise state counts and the current focus",
        usage: &["clerk brief [--json]"],
        options: &[("--json", "machine readable summary")],
        examples: &["clerk brief"],
    },
    Builtin {
        name: "focus",
        subcommands: &["set", "show", "add", "remove"],
        summary: "read or change the current focus",
        usage: &[
            "clerk focus show",
            "clerk focus set <key>...",
            "clerk focus add <key>...",
            "clerk focus remove <key>...",
        ],
        options: &[(
            "--batch <name>",
            "capture this focus change in a local draft",
        )],
        examples: &["clerk focus set APP-006 APP-007"],
    },
    Builtin {
        name: "place",
        subcommands: &[],
        summary: "print the directory a category lives in",
        usage: &["clerk place <category>"],
        options: &[],
        examples: &["clerk place contracts"],
    },
    Builtin {
        name: "init",
        subcommands: &[],
        summary: "create the missing directories, marker and status page",
        usage: &["clerk init"],
        options: &[],
        examples: &["clerk init"],
    },
    Builtin {
        name: "links",
        subcommands: &["check", "scan", "repair"],
        summary: "inspect and repair local document references",
        usage: &["clerk links <check|scan|repair> [path...] [--apply] [--linkify]"],
        options: &[],
        examples: &[
            "clerk links check",
            "clerk links repair --linkify",
            "clerk links repair --apply",
        ],
    },
    Builtin {
        name: "completions",
        subcommands: &["bash", "zsh", "powershell"],
        summary: "print a shell completion script",
        usage: &["clerk completions <bash|zsh|powershell>"],
        options: &[],
        examples: &["clerk completions bash"],
    },
    Builtin {
        name: "help",
        subcommands: &[],
        summary: "show this overview or one page",
        usage: &["clerk help [command]"],
        options: &[],
        examples: &["clerk help", "clerk help tasks"],
    },
];
