//! Structured references belong to an append entry, never to a whole daily page.

use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};

const OPEN: &str = "<!-- clerk:entry\n";
const CLOSE: &str = "\n-->";

pub fn unique_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos}-{}", std::process::id())
}

pub fn validate_refs(context: &Context, target: &str, keys: &[String]) -> Result<()> {
    let mut declaration = toml::Table::new();
    for (key, value) in [("name", "entry_refs"), ("type", "ref"), ("target", target)] {
        declaration.insert(key.into(), toml::Value::String(value.into()));
    }
    declaration.insert("multi".into(), toml::Value::Boolean(true));
    context.check_field(
        &crate::config::kind::Field::parse(&declaration, "entry")?,
        &keys.join("|"),
    )
}

pub fn keys(value: &str) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    for key in value.split([',', '|']) {
        let key = key.trim();
        if key.is_empty() {
            return Err(Error::usage("references need non-empty keys"));
        }
        if !keys.iter().any(|value| value == key) {
            keys.push(key.into());
        }
    }
    Ok(keys)
}

pub fn create(
    context: &Context,
    action: &crate::config::kind::Action,
    input: Option<&str>,
) -> Result<String> {
    let keys = input.map(keys).transpose()?.unwrap_or_default();
    if input.is_some() && action.entry_target.is_none() {
        return Err(Error::usage(
            "--tasks requires entry_target on this append action",
        ));
    }
    if let Some(target) = &action.entry_target {
        validate_refs(context, target, &keys)?;
    }
    let mut data = toml::Table::new();
    data.insert("version".into(), toml::Value::Integer(1));
    data.insert("id".into(), toml::Value::String(unique_id()));
    if let Some(target) = &action.entry_target {
        data.insert("target".into(), toml::Value::String(target.clone()));
    }
    data.insert(
        "keys".into(),
        toml::Value::Array(keys.into_iter().map(toml::Value::String).collect()),
    );
    let serialized = toml::to_string(&data).map_err(|e| Error::io(e.to_string()))?;
    Ok(format!("{OPEN}{}{CLOSE}\n", serialized.trim_end()))
}

pub struct Entry {
    pub id: Option<String>,
    pub target: Option<String>,
    pub keys: Vec<String>,
    pub line: usize,
    pub text: String,
}

impl Entry {
    pub fn matches(&self, target: Option<&str>, key: &str) -> bool {
        self.explicit(target, key) || crate::storage::records::mentions(&self.text, key)
    }

    pub fn explicit(&self, target: Option<&str>, key: &str) -> bool {
        (target.is_none() || self.target.as_deref() == target)
            && self.keys.iter().any(|item| item == key)
    }
}

/// Fenced headings are body content. Metadata is recognized only immediately after a heading.
pub fn parse(body: &str) -> Result<Vec<Entry>> {
    let lines: Vec<&str> = body.lines().collect();
    let mut starts = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().unwrap_or(' ');
        let count = trimmed.chars().take_while(|ch| *ch == marker).count();
        if matches!(marker, '`' | '~') && count >= 3 {
            if let Some((active, width)) = fence {
                if marker == active && count >= width {
                    fence = None;
                }
            } else {
                fence = Some((marker, count));
            }
            continue;
        }
        if fence.is_none() && line.starts_with("## ") {
            starts.push(index);
        }
    }
    let first_structured = starts.iter().copied().find(|index| {
        lines
            .get(index + 1)
            .is_some_and(|line| *line == "<!-- clerk:entry")
    });
    if let Some(first) = first_structured {
        starts.retain(|index| {
            *index < first
                || lines
                    .get(index + 1)
                    .is_some_and(|line| *line == "<!-- clerk:entry")
                || lines[*index].starts_with("## [T")
        });
    }
    if starts.is_empty() {
        starts.push(0);
    }
    starts.push(lines.len());
    let mut entries = Vec::new();
    for pair in starts.windows(2) {
        let mut text = lines[pair[0]..pair[1]].join("\n");
        let mut entry = Entry {
            id: None,
            target: None,
            keys: Vec::new(),
            line: pair[0] + 1,
            text: String::new(),
        };
        let offset = text.find('\n').map(|i| i + 1).unwrap_or(0);
        if text[offset..].starts_with(OPEN) {
            let end = text[offset + OPEN.len()..]
                .find(CLOSE)
                .map(|i| offset + OPEN.len() + i)
                .ok_or_else(|| Error::validation("unterminated log entry metadata"))?;
            let table: toml::Table = toml::from_str(&text[offset + OPEN.len()..end])
                .map_err(|e| Error::validation(format!("invalid log entry metadata: {e}")))?;
            crate::config::decl::only_keys(
                &table,
                &["version", "id", "target", "keys"],
                "log entry",
            )?;
            if table.get("version").and_then(toml::Value::as_integer) != Some(1) {
                return Err(Error::validation("log entry version must be 1"));
            }
            let id = crate::config::decl::required_text(&table, "id", "log entry")?;
            if id.is_empty() {
                return Err(Error::validation("log entry id cannot be empty"));
            }
            entry.id = Some(id);
            entry.target = table
                .get("target")
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| Error::validation("log entry target must be a string"))
                })
                .transpose()?;
            entry.keys = table
                .get("keys")
                .and_then(toml::Value::as_array)
                .ok_or_else(|| Error::validation("log entry keys must be an array"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .filter(|key| !key.is_empty())
                        .map(str::to_string)
                        .ok_or_else(|| {
                            Error::validation("log entry keys must be non-empty strings")
                        })
                })
                .collect::<Result<Vec<_>>>()?;
            if !entry.keys.is_empty() && entry.target.is_none() {
                return Err(Error::validation("log entry keys require target"));
            }
            text.replace_range(offset..end + CLOSE.len(), "");
        }
        entry.text = text;
        entries.push(entry);
    }
    Ok(entries)
}

/// Reuse reference validation for writes and workspace diagnostics, including archived pages.
pub fn validate_page(
    context: &Context,
    kind: &crate::config::kind::Kind,
    body: &str,
) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    for entry in parse(body)? {
        if let Some(id) = &entry.id
            && !ids.insert(id.clone())
        {
            return Err(Error::validation(format!("duplicate log entry id `{id}`")));
        }
        if let Some(target) = &entry.target {
            if !kind.actions.iter().any(|action| {
                action.handler == "append" && action.entry_target.as_ref() == Some(target)
            }) {
                return Err(Error::validation(format!(
                    "log entry target `{target}` is not declared on an append action"
                )));
            }
            validate_refs(context, target, &entry.keys)?;
        }
    }
    Ok(())
}
