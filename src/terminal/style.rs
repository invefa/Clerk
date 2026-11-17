//! Semantic byte ranges stay separate from terminal escape sequences.

use crate::config::decl::Value;
use crate::foundation::error::{Error, Result};
use crate::interface::cli::Args;
use std::collections::BTreeMap;
use std::ops::Range;

pub const ROLES: &[&str] = &[
    "heading",
    "muted",
    "identifier",
    "info",
    "success",
    "warning",
    "error",
];

#[derive(Clone, Debug)]
pub enum Spec {
    Role(String),
    Values(BTreeMap<String, String>),
}

impl Spec {
    pub fn parse(value: &Value) -> Result<Self> {
        let role = |value: &Value| -> Result<String> {
            value
                .as_str()
                .filter(|text| ROLES.contains(text))
                .map(str::to_string)
                .ok_or_else(|| {
                    Error::usage(format!("terminal_style needs one of {}", ROLES.join(", ")))
                })
        };
        if let Some(table) = value.as_table() {
            Ok(Self::Values(
                table
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), role(value)?)))
                    .collect::<Result<_>>()?,
            ))
        } else {
            Ok(Self::Role(role(value)?))
        }
    }

    pub fn role(&self, value: &str) -> Option<&str> {
        match self {
            Self::Role(role) => Some(role),
            Self::Values(values) => values
                .get(value)
                .or_else(|| values.get("*"))
                .map(String::as_str),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub role: String,
}

pub fn enabled(args: &Args, machine: bool) -> bool {
    use std::io::IsTerminal;
    if machine {
        return false;
    }
    match args.value("color").unwrap_or("auto") {
        "always" => true,
        "never" => false,
        _ => {
            std::io::stdout().is_terminal()
                && std::env::var_os("NO_COLOR").is_none()
                && std::env::var("TERM").as_deref() != Ok("dumb")
        }
    }
}

fn code(role: &str) -> &'static str {
    match role {
        "heading" => "1",
        "muted" => "90",
        "identifier" => "36",
        "info" => "34",
        "success" => "32",
        "warning" => "33",
        "error" => "31",
        "match" => "1;4",
        "current" => "1;7",
        _ => "0",
    }
}

/// Later spans take precedence, allowing search overlays on field styles.
pub fn paint(text: &str, spans: &[Span], enabled: bool) -> String {
    if !enabled || spans.is_empty() {
        return text.to_string();
    }
    let mut bounds = vec![0, text.len()];
    for span in spans {
        if span.range.start <= span.range.end
            && text.is_char_boundary(span.range.start)
            && text.is_char_boundary(span.range.end)
        {
            bounds.extend([span.range.start, span.range.end]);
        }
    }
    bounds.sort_unstable();
    bounds.dedup();
    let mut output = String::new();
    for pair in bounds.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start == end {
            continue;
        }
        let active = spans
            .iter()
            .filter(|span| span.range.start <= start && span.range.end >= end)
            .collect::<Vec<_>>();
        let base = active
            .iter()
            .rev()
            .find(|span| !matches!(span.role.as_str(), "match" | "current"));
        let emphasis = active
            .iter()
            .rev()
            .find(|span| matches!(span.role.as_str(), "match" | "current"));
        let mut codes = Vec::new();
        if let Some(span) = base {
            codes.push(code(&span.role));
        }
        if let Some(span) = emphasis {
            codes.push(code(&span.role));
        }
        if codes.is_empty() {
            output.push_str(&text[start..end]);
        } else {
            output.push_str(&format!(
                "\x1b[{}m{}\x1b[0m",
                codes.join(";"),
                &text[start..end]
            ));
        }
    }
    output
}

/// The generated literal gutter is separate from the source body, even on blank lines.
pub fn gutter(text: &str) -> Option<Span> {
    crate::terminal::text::literal_gutter(text).map(|end| Span {
        range: 0..end,
        role: "muted".into(),
    })
}

/// Reflow source spans and explicitly style newly generated continuation gutters.
pub fn project(source: &str, spans: &[Span], row: &crate::terminal::text::ReadingRow) -> Vec<Span> {
    let mut mapped = Vec::new();
    if row.prefix > 0 && gutter(source).is_some() {
        mapped.push(Span {
            range: 0..row.prefix,
            role: "muted".into(),
        });
    }
    mapped.extend(spans.iter().filter_map(|span| {
        let start = span.range.start.max(row.range.start);
        let end = span.range.end.min(row.range.end);
        (start < end).then(|| Span {
            range: start - row.range.start + row.prefix..end - row.range.start + row.prefix,
            role: span.role.clone(),
        })
    }));
    mapped
}

pub fn plain(text: &str) -> Vec<Span> {
    if let Some(gutter) = gutter(text) {
        // Colons and headings after this boundary are verbatim user content.
        return vec![gutter];
    }
    let mut spans = Vec::new();
    if text.trim_end().ends_with(':') {
        spans.push(Span {
            range: 0..text.len(),
            role: "heading".into(),
        });
    } else if let Some((label, _)) = text.split_once(": ") {
        spans.push(Span {
            range: 0..label.len() + 1,
            role: "heading".into(),
        });
    }
    spans
}

pub fn error(args: Option<&Args>, text: &str) -> String {
    use std::io::IsTerminal;
    let setting = args.and_then(|args| args.value("color")).unwrap_or("auto");
    let enabled = setting == "always"
        || (setting == "auto"
            && std::io::stderr().is_terminal()
            && std::env::var_os("NO_COLOR").is_none());
    paint(
        text,
        &[Span {
            range: 0..text.len(),
            role: "error".into(),
        }],
        enabled,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_are_validated_and_ansi_is_only_added_at_encoding() {
        let text = "中文 Clerk";
        let spans = vec![
            Span {
                range: 0..text.len(),
                role: "identifier".into(),
            },
            Span {
                range: 7..12,
                role: "current".into(),
            },
        ];
        assert_eq!(paint(text, &spans, false), text);
        let styled = paint(text, &spans, true);
        assert!(styled.contains("\x1b[36m中文 "));
        assert!(styled.contains("\x1b[36;1;7mClerk"));
        assert!(Spec::parse(&Value::String("unknown".into())).is_err());
        assert!(Spec::parse(&Value::Integer(1)).is_err());
        let mut args = Args::default();
        args.values.insert("color".into(), "always".into());
        assert!(!enabled(&args, true));
    }
}
