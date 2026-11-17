//! Which records a list keeps.
//!
//! Field flags are equality sugar, and `--filter` takes one expression:
//!
//! ```text
//! state = done
//! state in (todo, active)
//! title ~ 预约 and state != done
//! updated >= 20261001 and updated <= 20261031
//! (state = done or state = review) and owner = alice
//! ```
//!
//! Operators: `=`, `!=`, `~`, `!~`, `>`, `>=`, `<`, `<=`, `in`. Combine with
//! `and`, `or` and parentheses. A quoted value may contain spaces.

use crate::config::identity::Identity;
use crate::config::kind::Kind;
use crate::execution::context::Context;
use crate::foundation::error::{Error, Result};
use crate::query::matching::Matcher;
use crate::storage::store::Record;

/// Everything a list filters on.
pub struct Keep {
    filter: Option<Expr>,
    filters: Vec<(String, Matcher)>,
    mine: Option<Identity>,
    claimed: bool,
}

impl Keep {
    /// Resolve the filters of one command.
    pub fn load(
        context: &mut Context,
        kind: &Kind,
        filter: Option<&str>,
        filters: &[(String, String)],
        mine: bool,
        claimed: bool,
        case: Option<&str>,
    ) -> Result<Self> {
        let mine = if mine {
            Some(context.identity()?.clone())
        } else {
            None
        };
        let filter = match filter {
            Some(text) => {
                let mut expr = parse_expression(context, kind, text, case)?;
                expr.compile(case)?;
                Some(expr)
            }
            None => None,
        };
        let mut resolved = Vec::new();
        for (name, value) in filters {
            let field = kind.field(name).ok_or_else(|| {
                Error::usage(format!("kind `{}` has no field `{name}`", kind.name))
            })?;
            resolved.push((
                name.clone(),
                Matcher::new(
                    &normalize(context, field, value, case)?,
                    "exact",
                    case == Some("ignore"),
                )?,
            ));
        }
        Ok(Keep {
            filter,
            filters: resolved,
            mine,
            claimed,
        })
    }

    /// Whether one record survives the filters.
    pub fn keep(&self, context: &Context, kind: &Kind, record: &Record) -> Result<bool> {
        if let Some(expr) = &self.filter
            && !expr.eval(context, kind, record)?
        {
            return Ok(false);
        }
        for (name, wanted) in &self.filters {
            let raw = record.get(name).cloned().unwrap_or_default();
            let matched = raw.split('|').any(|part| wanted.is_match(part.trim()));
            if !matched {
                return Ok(false);
            }
        }
        if let Some(identity) = &self.mine
            && !is_mine(identity, record)
        {
            return Ok(false);
        }
        if self.claimed && claimant_of(record).is_empty() {
            return Ok(false);
        }
        Ok(true)
    }
}

/// A parsed condition.
enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Cond {
        field: String,
        op: Op,
        values: Vec<String>,
        matchers: Vec<Matcher>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Like,
    NotLike,
    Matches,
    NotMatches,
    Gt,
    Ge,
    Lt,
    Le,
    In,
}

impl Expr {
    fn compile(&mut self, case: Option<&str>) -> Result<()> {
        match self {
            Self::And(left, right) | Self::Or(left, right) => {
                left.compile(case)?;
                right.compile(case)?;
            }
            Self::Cond {
                op,
                values,
                matchers,
                ..
            } => {
                let mode = match op {
                    Op::Like | Op::NotLike => "contains",
                    Op::Matches | Op::NotMatches => "regex",
                    _ => "exact",
                };
                let ignore = case.map(|value| value == "ignore").unwrap_or(matches!(
                    op,
                    Op::Like | Op::NotLike | Op::Matches | Op::NotMatches
                ));
                *matchers = values
                    .iter()
                    .map(|value| Matcher::new(value, mode, ignore))
                    .collect::<Result<_>>()?;
            }
        }
        Ok(())
    }

    fn eval(&self, context: &Context, kind: &Kind, record: &Record) -> Result<bool> {
        match self {
            Expr::And(left, right) => {
                Ok(left.eval(context, kind, record)? && right.eval(context, kind, record)?)
            }
            Expr::Or(left, right) => {
                Ok(left.eval(context, kind, record)? || right.eval(context, kind, record)?)
            }
            Expr::Cond {
                field,
                op,
                values,
                matchers,
            } => {
                let raw = record.get(field).cloned().unwrap_or_default();
                let mut hits = Vec::new();
                for (value, matcher) in values.iter().zip(matchers) {
                    hits.push(match op {
                        Op::Like | Op::NotLike | Op::Matches | Op::NotMatches => {
                            let shown =
                                crate::terminal::value::cell(context, kind, field, &raw, true)?;
                            let multi = kind.field(field).is_some_and(|field| field.multi);
                            if multi {
                                parts(&raw)
                                    .iter()
                                    .chain(parts(&shown).iter())
                                    .any(|part| matcher.is_match(part))
                            } else {
                                matcher.is_match(&raw) || matcher.is_match(&shown)
                            }
                        }
                        Op::Eq | Op::Ne | Op::In => {
                            parts(&raw).iter().any(|part| matcher.is_match(part))
                        }
                        _ => parts(&raw).iter().any(|part| match op {
                            Op::Gt => part > value,
                            Op::Ge => part >= value,
                            Op::Lt => part < value,
                            Op::Le => part <= value,
                            _ => false,
                        }),
                    });
                }
                Ok(match op {
                    Op::Ne | Op::NotLike | Op::NotMatches => hits.iter().all(|hit| !hit),
                    Op::In => hits.iter().any(|hit| *hit),
                    _ => hits.iter().any(|hit| *hit),
                })
            }
        }
    }
}

/// One token of an expression.
enum Token {
    Word(String),
    Value(String),
    Op(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_whitespace() {
            continue;
        }
        match character {
            '(' => out.push(Token::Open),
            ')' => out.push(Token::Close),
            '\'' | '"' => {
                let quote = character;
                let mut value = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == quote {
                        closed = true;
                        break;
                    }
                    value.push(next);
                }
                if !closed {
                    return Err(Error::usage("filter: an unterminated quote"));
                }
                out.push(Token::Value(value));
            }
            '=' => out.push(Token::Op("=".to_string())),
            '!' => match chars.peek() {
                Some('=') => {
                    chars.next();
                    out.push(Token::Op("!=".to_string()));
                }
                Some('~') => {
                    chars.next();
                    out.push(Token::Op("!~".to_string()));
                }
                _ => return Err(Error::usage("filter: `!` must be followed by `=` or `~`")),
            },
            '~' => out.push(Token::Op("~".to_string())),
            '>' | '<' => {
                let mut op = character.to_string();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    op.push('=');
                }
                out.push(Token::Op(op));
            }
            ',' => out.push(Token::Op(",".to_string())),
            _ => {
                let mut word = String::new();
                word.push(character);
                while let Some(next) = chars.peek() {
                    if next.is_whitespace()
                        || matches!(
                            next,
                            '(' | ')' | '=' | '!' | '~' | '>' | '<' | ',' | '"' | '\''
                        )
                    {
                        break;
                    }
                    word.push(*next);
                    chars.next();
                }
                out.push(Token::Word(word));
            }
        }
    }
    Ok(out)
}

fn parse_expression(
    context: &Context,
    kind: &Kind,
    text: &str,
    case: Option<&str>,
) -> Result<Expr> {
    let tokens = tokenize(text)?;
    let mut at = 0;
    let expr = parse_or(context, kind, &tokens, &mut at, case)?;
    if at != tokens.len() {
        return Err(Error::usage(
            "filter: unexpected tokens after the expression",
        ));
    }
    Ok(expr)
}

fn parse_or(
    context: &Context,
    kind: &Kind,
    tokens: &[Token],
    at: &mut usize,
    case: Option<&str>,
) -> Result<Expr> {
    let mut left = parse_and(context, kind, tokens, at, case)?;
    while keyword(tokens, *at, "or") {
        *at += 1;
        let right = parse_and(context, kind, tokens, at, case)?;
        left = Expr::Or(Box::new(left), Box::new(right));
    }
    Ok(left)
}

fn parse_and(
    context: &Context,
    kind: &Kind,
    tokens: &[Token],
    at: &mut usize,
    case: Option<&str>,
) -> Result<Expr> {
    let mut left = parse_primary(context, kind, tokens, at, case)?;
    while keyword(tokens, *at, "and") {
        *at += 1;
        let right = parse_primary(context, kind, tokens, at, case)?;
        left = Expr::And(Box::new(left), Box::new(right));
    }
    Ok(left)
}

fn parse_primary(
    context: &Context,
    kind: &Kind,
    tokens: &[Token],
    at: &mut usize,
    case: Option<&str>,
) -> Result<Expr> {
    if matches!(tokens.get(*at), Some(Token::Open)) {
        *at += 1;
        let expr = parse_or(context, kind, tokens, at, case)?;
        match tokens.get(*at) {
            Some(Token::Close) => *at += 1,
            _ => return Err(Error::usage("filter: a group is not closed")),
        }
        return Ok(expr);
    }
    let field = match tokens.get(*at) {
        Some(Token::Word(name)) => name.clone(),
        _ => return Err(Error::usage("filter: expected a field name")),
    };
    let spec = kind
        .field(&field)
        .ok_or_else(|| Error::usage(format!("kind `{}` has no field `{field}`", kind.name)))?;
    *at += 1;
    let op = match tokens.get(*at) {
        Some(Token::Op(op)) => match op.as_str() {
            "=" => Op::Eq,
            "!=" => Op::Ne,
            "~" => Op::Like,
            "!~" => Op::NotLike,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            "<" => Op::Lt,
            "<=" => Op::Le,
            "," => return Err(Error::usage("filter: expected an operator")),
            _ => return Err(Error::usage("filter: expected an operator")),
        },
        Some(Token::Word(word)) if word.eq_ignore_ascii_case("in") => Op::In,
        Some(Token::Word(word)) if word.eq_ignore_ascii_case("matches") => Op::Matches,
        Some(Token::Word(word))
            if word.eq_ignore_ascii_case("not") && keyword(tokens, *at + 1, "matches") =>
        {
            *at += 1;
            Op::NotMatches
        }
        _ => return Err(Error::usage("filter: expected an operator")),
    };
    *at += 1;
    let mut values = Vec::new();
    if op == Op::In {
        match tokens.get(*at) {
            Some(Token::Open) => *at += 1,
            _ => return Err(Error::usage("filter: `in` needs a parenthesized list")),
        }
        loop {
            match tokens.get(*at) {
                Some(Token::Value(value)) | Some(Token::Word(value)) => {
                    values.push(normalize(context, spec, value, case)?);
                    *at += 1;
                }
                _ => return Err(Error::usage("filter: `in` needs a parenthesized list")),
            }
            match tokens.get(*at) {
                Some(Token::Op(op)) if op == "," => *at += 1,
                Some(Token::Close) => {
                    *at += 1;
                    break;
                }
                _ => return Err(Error::usage("filter: `in` list is not closed")),
            }
        }
    } else {
        match tokens.get(*at) {
            Some(Token::Value(value)) | Some(Token::Word(value)) => {
                values.push(if matches!(op, Op::Matches | Op::NotMatches) {
                    value.clone()
                } else {
                    normalize(context, spec, value, case)?
                });
                *at += 1;
            }
            _ => return Err(Error::usage("filter: expected a value")),
        }
    }
    Ok(Expr::Cond {
        field,
        op,
        values,
        matchers: Vec::new(),
    })
}

fn keyword(tokens: &[Token], at: usize, word: &str) -> bool {
    matches!(tokens.get(at), Some(Token::Word(value)) if value.eq_ignore_ascii_case(word))
}

/// The values of a cell, one part per value for a multi valued field.
fn parts(raw: &str) -> Vec<String> {
    raw.split('|')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

fn claimant_of(record: &Record) -> &str {
    record
        .get("claimant")
        .map(|value| value.trim())
        .unwrap_or("")
}

fn claimant_email_of(record: &Record) -> &str {
    record
        .get("claimant_email")
        .map(|value| value.trim())
        .unwrap_or("")
}

/// Whether the identity holds the instance.
fn is_mine(identity: &Identity, record: &Record) -> bool {
    let email = claimant_email_of(record);
    if !email.is_empty() && email.eq_ignore_ascii_case(&identity.git_email) {
        return true;
    }
    let claimant = claimant_of(record);
    if claimant.is_empty() {
        return false;
    }
    claimant == identity.display()
}

fn normalize(
    context: &Context,
    field: &crate::config::kind::Field,
    value: &str,
    case: Option<&str>,
) -> Result<String> {
    if case == Some("ignore")
        && let Some(registry) = &field.registry
    {
        let matcher = Matcher::new(value, "exact", true)?;
        let keys = context
            .registry(registry)?
            .entries()
            .iter()
            .filter(|entry| matcher.is_match(&entry.key))
            .collect::<Vec<_>>();
        if let [entry] = keys.as_slice() {
            return Ok(entry.key.clone());
        }
        if keys.len() > 1 {
            return Err(Error::usage(format!(
                "ambiguous case-insensitive value `{value}`"
            )));
        }
    }
    crate::execution::engine::normalize(context, field, value)
}
