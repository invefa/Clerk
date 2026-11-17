//! Numbering: allocating and checking identifiers.
//!
//! A scheme declares the shape of an identifier. Allocation scans the keys
//! already in use and takes the next free counter, so the data file stays the
//! only source of truth and no counter file can drift.

use std::collections::BTreeMap;

use crate::config::decl::{self, Table};
use crate::config::registry::Registry;
use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};

/// Registries loaded for token lookup.
#[derive(Clone, Debug, Default)]
pub struct Registries {
    tables: BTreeMap<String, Registry>,
}

impl Registries {
    pub fn load(workspace: &Workspace, names: &[&str]) -> Result<Self> {
        let mut tables = BTreeMap::new();
        for name in names {
            if !tables.contains_key(*name) {
                let registry = Registry::load(workspace, name)?;
                tables.insert((*name).to_string(), registry);
            }
        }
        Ok(Registries { tables })
    }

    pub fn get(&self, name: &str) -> Result<&Registry> {
        self.tables
            .get(name)
            .ok_or_else(|| Error::usage(format!("registry `{name}` is not loaded")))
    }

    pub fn with(mut self, name: impl Into<String>, registry: Registry) -> Self {
        self.tables.insert(name.into(), registry);
        self
    }
}

/// One declared numbering scheme.
#[derive(Clone, Debug)]
pub struct Scheme {
    pub name: String,
    parts: Vec<Part>,
    pub counter: String,
    pub counter_width: Option<usize>,
    pub sort: Vec<String>,
    pub start: u64,
    /// Token name to registry name for values that are not the counter.
    pub tokens: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
enum Part {
    Literal(String),
    Placeholder { name: String, width: Option<usize> },
}

impl Scheme {
    pub fn parse(name: &str, table: &Table) -> Result<Self> {
        let context = format!("ids.{name}");
        decl::only_keys(
            table,
            &["pattern", "counter", "render", "sort", "start", "tokens"],
            &context,
        )?;
        let pattern = decl::required_text(table, "pattern", &context)?;
        let parts = parse_parts(&pattern, &context)?;
        let counter = decl::text(table, "counter").unwrap_or_else(|| "serial".to_string());
        let mut tokens = BTreeMap::new();
        if let Some(section) = decl::sub_table(table, "tokens") {
            for (key, value) in &section {
                let registry = value.as_str().ok_or_else(|| {
                    Error::usage(format!("{context}.tokens.{key}: expected a registry name"))
                })?;
                if key == &counter {
                    return Err(Error::usage(format!(
                        "{context}.tokens: the counter `{counter}` cannot also be a token"
                    )));
                }
                tokens.insert(key.clone(), registry.to_string());
            }
        }
        let mut counter_width = None;
        let mut seen_counter = false;
        for part in &parts {
            match part {
                Part::Placeholder { name, width } if name == &counter => {
                    seen_counter = true;
                    counter_width = *width;
                }
                Part::Placeholder { width: Some(_), .. } => {}
                Part::Placeholder { name, width: None } => {
                    if !tokens.contains_key(name) {
                        return Err(Error::usage(format!(
                            "{context}: placeholder `{{{name}}}` needs a width or must be listed in `[tokens]`"
                        )));
                    }
                }
                Part::Literal(_) => {}
            }
        }
        if !seen_counter {
            return Err(Error::usage(format!(
                "{context}: counter `{counter}` does not appear in the pattern"
            )));
        }
        let start = decl::integer(table, "start").unwrap_or(1);
        let start = u64::try_from(start)
            .map_err(|_| Error::usage(format!("{context}: start must be positive")))?;
        let mut tokens = BTreeMap::new();
        if let Some(section) = decl::sub_table(table, "tokens") {
            for (key, value) in &section {
                let registry = value.as_str().ok_or_else(|| {
                    Error::usage(format!("{context}.tokens.{key}: expected a registry name"))
                })?;
                if key == &counter {
                    return Err(Error::usage(format!(
                        "{context}.tokens: the counter `{counter}` cannot also be a token"
                    )));
                }
                tokens.insert(key.clone(), registry.to_string());
            }
        }
        for part in &parts {
            if let Part::Placeholder { name, width: None } = part
                && !tokens.contains_key(name)
            {
                return Err(Error::usage(format!(
                    "{context}: `{{{name}}}` has no width and no registry in `[tokens]`"
                )));
            }
        }
        let mut sort = decl::strings(table, "sort");
        if sort.is_empty() {
            sort.push(counter.clone());
        }
        // The pattern must be tokenised so that a name can be matched later.
        let mut rebuilt = String::new();
        for part in &parts {
            match part {
                Part::Literal(text) => rebuilt.push_str(text),
                Part::Placeholder { name, width } => match width {
                    Some(width) => rebuilt.push_str(&format!("{{{name}:{width}}}")),
                    None => rebuilt.push_str(&format!("{{{name}}}")),
                },
            }
        }
        if rebuilt != pattern {
            return Err(Error::usage(format!(
                "{context}: pattern uses a form this version cannot read"
            )));
        }
        Ok(Scheme {
            name: name.to_string(),
            parts,
            counter,
            counter_width,
            sort,
            start,
            tokens,
        })
    }

    /// Allocate the next identifier for one combination of token values.
    pub fn allocate(
        &self,
        existing: &[String],
        values: &BTreeMap<String, String>,
        registries: &Registries,
    ) -> Result<String> {
        self.check_values(values, registries)?;
        let mut highest: Option<u64> = None;
        for key in existing {
            if let Some(matched) = self.tokens_of_key(key, registries)? {
                if self
                    .tokens
                    .iter()
                    .any(|(name, _)| values.get(name) != matched.get(name))
                {
                    continue;
                }
                if let Some(text) = matched.get(&self.counter)
                    && let Ok(number) = text.parse::<u64>()
                {
                    highest = Some(highest.map_or(number, |current| current.max(number)));
                }
            }
        }
        let next = match highest {
            Some(number) => number
                .checked_add(1)
                .ok_or_else(|| Error::usage(format!("ids.{}: counter overflow", self.name)))?,
            None => self.start,
        };
        let mut with_counter = values.clone();
        with_counter.insert(self.counter.clone(), next.to_string());
        let rendered = self.render(&with_counter);
        if self.tokens_of_key(&rendered, registries)?.is_none() {
            return Err(Error::usage(format!(
                "ids.{}: rendered `{rendered}` does not match the declared pattern",
                self.name
            )));
        }
        if existing.iter().any(|key| key == &rendered) {
            return Err(Error::conflict(format!(
                "ids.{}: `{rendered}` is already used",
                self.name
            )));
        }
        Ok(rendered)
    }

    /// Allocate using defaults taken from the registries for missing tokens.
    pub fn allocate_with_defaults(
        &self,
        existing: &[String],
        values: &mut BTreeMap<String, String>,
        registries: &Registries,
    ) -> Result<String> {
        for (name, registry_name) in &self.tokens {
            if !values.contains_key(name) {
                let registry = registries.get(registry_name)?;
                let first =
                    registry.keys().into_iter().next().ok_or_else(|| {
                        Error::usage(format!("registry `{registry_name}` is empty"))
                    })?;
                values.insert(name.clone(), first);
            }
        }
        self.allocate(existing, values, registries)
    }

    fn check_values(
        &self,
        values: &BTreeMap<String, String>,
        registries: &Registries,
    ) -> Result<()> {
        for (name, registry_name) in &self.tokens {
            let Some(value) = values.get(name) else {
                return Err(Error::usage(format!(
                    "ids.{}: token `{name}` is required",
                    self.name
                )));
            };
            let registry = registries.get(registry_name)?;
            if !registry.has(value) {
                return Err(Error::usage(format!(
                    "`{value}` is not registered in `{registry_name}`"
                )));
            }
        }
        Ok(())
    }

    fn render(&self, values: &BTreeMap<String, String>) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Literal(text) => out.push_str(text),
                Part::Placeholder { name, width } => {
                    let value = values.get(name).cloned().unwrap_or_default();
                    match width {
                        Some(width)
                            if value.chars().all(|character| character.is_ascii_digit()) =>
                        {
                            out.push_str(&format!("{value:0>width$}", width = *width));
                        }
                        _ => out.push_str(&value),
                    }
                }
            }
        }
        out
    }

    /// Placeholder names in the order the pattern declares them.
    pub fn part_names(&self) -> Vec<String> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                Part::Placeholder { name, .. } => Some(name.clone()),
                Part::Literal(_) => None,
            })
            .collect()
    }
    /// Split a key into its tokens, using the registries to disambiguate.
    pub fn tokens_of_key(
        &self,
        key: &str,
        registries: &Registries,
    ) -> Result<Option<BTreeMap<String, String>>> {
        let mut names: Vec<(String, Vec<String>)> = Vec::new();
        for (name, registry_name) in &self.tokens {
            let registry = registries.get(registry_name)?;
            let mut candidates = registry.keys();
            candidates.sort_by_key(|value| std::cmp::Reverse(value.len()));
            names.push((name.clone(), candidates));
        }
        // Few tokens with small registries; the product stays bounded.
        let mut combinations: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
        for (name, candidates) in &names {
            let mut next = Vec::new();
            for combination in &combinations {
                for candidate in candidates {
                    let mut extended = combination.clone();
                    extended.insert(name.clone(), candidate.clone());
                    next.push(extended);
                }
            }
            combinations = next;
        }
        for combination in combinations {
            if let Some(matched) = self.match_with(key, &combination) {
                return Ok(Some(matched));
            }
        }
        Ok(None)
    }

    fn match_with(
        &self,
        key: &str,
        values: &BTreeMap<String, String>,
    ) -> Option<BTreeMap<String, String>> {
        let mut rest = key;
        let mut tokens = BTreeMap::new();
        for part in &self.parts {
            match part {
                Part::Literal(text) => {
                    rest = rest.strip_prefix(text.as_str())?;
                }
                Part::Placeholder { name, width } => {
                    if let Some(value) = values.get(name) {
                        rest = rest.strip_prefix(value.as_str())?;
                        tokens.insert(name.clone(), value.clone());
                        continue;
                    }
                    let width = (*width)?;
                    let count = rest.chars().count();
                    if count < width {
                        return None;
                    }
                    let split = if count == width {
                        rest.len()
                    } else {
                        rest.char_indices().nth(width).map(|(index, _)| index)?
                    };
                    let (head, tail) = rest.split_at(split);
                    if !head.chars().all(|character| character.is_ascii_digit()) {
                        return None;
                    }
                    tokens.insert(name.clone(), head.to_string());
                    rest = tail;
                }
            }
        }
        if rest.is_empty() { Some(tokens) } else { None }
    }

    /// Whether the key has the declared shape, whatever its tokens are.
    pub fn matches(&self, key: &str, registries: &Registries) -> Result<bool> {
        Ok(self.tokens_of_key(key, registries)?.is_some())
    }

    /// Sorted tokens used for ordering.
    pub fn sort_key(&self, key: &str, registries: &Registries) -> Result<Vec<u64>> {
        let Some(tokens) = self.tokens_of_key(key, registries)? else {
            return Ok(vec![u64::MAX]);
        };
        let mut out = Vec::new();
        for name in &self.sort {
            let value = tokens.get(name).cloned().unwrap_or_default();
            out.push(value.parse::<u64>().unwrap_or(u64::MAX));
        }
        Ok(out)
    }
}

fn parse_parts(text: &str, context: &str) -> Result<Vec<Part>> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let literal = &rest[..open];
        if !literal.is_empty() {
            parts.push(Part::Literal(literal.to_string()));
        }
        let close = rest[open..]
            .find('}')
            .map(|offset| open + offset)
            .ok_or_else(|| Error::usage(format!("{context}: unclosed placeholder in `{text}`")))?;
        let inner = &rest[open + 1..close];
        let (name, width) = match inner.split_once(':') {
            Some((name, width)) => {
                let width = width.parse::<usize>().map_err(|_| {
                    Error::usage(format!("{context}: bad width in placeholder `{{{inner}}}`"))
                })?;
                (name.to_string(), Some(width))
            }
            None => (inner.to_string(), None),
        };
        if name.is_empty() {
            return Err(Error::usage(format!("{context}: empty placeholder name")));
        }
        parts.push(Part::Placeholder { name, width });
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        parts.push(Part::Literal(rest.to_string()));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registries() -> Registries {
        Registries::default().with(
            "prefixes",
            Registry::from_entries(
                "prefixes",
                vec![
                    crate::config::registry::Entry::new("APP", "服务端"),
                    crate::config::registry::Entry::new("CLN", "客户端"),
                ],
            ),
        )
    }

    fn scheme() -> Scheme {
        let text = r#"
[ids.tasks]
pattern = "{prefix}-{serial:3}"
counter = "serial"
[ids.tasks.tokens]
prefix = "prefixes"
"#;
        let table = decl::parse(text, "test").expect("parsed");
        let ids = decl::sub_table(&table, "ids").expect("ids");
        let tasks = decl::sub_table(&ids, "tasks").expect("tasks");
        Scheme::parse("tasks", &tasks).expect("scheme")
    }

    #[test]
    fn allocates_after_the_highest_matching_key() {
        let scheme = scheme();
        let set = registries();
        let existing = vec![
            "APP-001".to_string(),
            "APP-007".to_string(),
            "CLN-004".to_string(),
        ];
        let mut values = BTreeMap::new();
        values.insert("prefix".to_string(), "APP".to_string());
        let key = scheme
            .allocate(&existing, &values, &set)
            .expect("allocated");
        assert_eq!(key, "APP-008");
    }

    #[test]
    fn starts_at_the_declared_value_when_empty() {
        let scheme = scheme();
        let set = registries();
        let mut values = BTreeMap::new();
        values.insert("prefix".to_string(), "CLN".to_string());
        assert_eq!(
            scheme.allocate(&[], &values, &set).expect("allocated"),
            "CLN-001"
        );
    }

    #[test]
    fn rejects_unregistered_token_values() {
        let scheme = scheme();
        let set = registries();
        let mut values = BTreeMap::new();
        values.insert("prefix".to_string(), "ZZZ".to_string());
        assert!(scheme.allocate(&[], &values, &set).is_err());
    }

    #[test]
    fn reads_tokens_from_a_key() {
        let scheme = scheme();
        let set = registries();
        let tokens = scheme
            .tokens_of_key("APP-042", &set)
            .expect("parsed")
            .expect("matched");
        assert_eq!(tokens.get("prefix").map(String::as_str), Some("APP"));
        assert_eq!(tokens.get("serial").map(String::as_str), Some("042"));
    }

    #[test]
    fn rejects_foreign_shapes() {
        let scheme = scheme();
        let set = registries();
        assert!(
            scheme
                .tokens_of_key("APP-42", &set)
                .expect("parsed")
                .is_none()
        );
        assert!(
            scheme
                .tokens_of_key("XXX-001", &set)
                .expect("parsed")
                .is_none()
        );
    }
}
