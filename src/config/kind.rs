//! Kind declarations: the single extension point of the tool.
//!
//! One TOML file describes one kind: where its instances live, which fields
//! they carry, how they are numbered, which states and transitions exist, and
//! how they are projected. Commands are generated from these declarations.

use std::path::Path;

use crate::config::decl::{self, Table};
use crate::config::registry::Registry;
use crate::config::workspace::Workspace;
use crate::foundation::error::{Error, Result};
use crate::foundation::time;

/// How a document keeps its metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetaStyle {
    /// A block between `<!-- clerk:meta` and `-->`.
    Block,
    /// Leading `- key: value` lines after the heading.
    Bullets,
}

/// How instances of a kind are stored.
#[derive(Clone, Debug)]
pub enum Storage {
    /// One row per instance in a CSV table.
    Table { path: String },
    /// One file per instance in a directory.
    Document {
        dir: String,
        filename: String,
        meta: MetaStyle,
    },
}

impl Storage {
    pub fn location(&self) -> &str {
        match self {
            Storage::Table { path } => path,
            Storage::Document { dir, .. } => dir,
        }
    }

    pub fn is_table(&self) -> bool {
        matches!(self, Storage::Table { .. })
    }
}

/// Type of a field value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Integer,
    Boolean,
    Enum,
    Date,
    Stamp,
    Path,
    Commit,
    Email,
    Reference,
    Slug,
}

impl FieldKind {
    fn parse(text: &str, context: &str) -> Result<Self> {
        Ok(match text {
            "text" => FieldKind::Text,
            "int" => FieldKind::Integer,
            "bool" => FieldKind::Boolean,
            "enum" => FieldKind::Enum,
            "date" => FieldKind::Date,
            "stamp" => FieldKind::Stamp,
            "path" => FieldKind::Path,
            "commit" => FieldKind::Commit,
            "email" => FieldKind::Email,
            "ref" => FieldKind::Reference,
            "slug" => FieldKind::Slug,
            other => {
                return Err(Error::usage(format!(
                    "{context}: unknown field type `{other}`"
                )));
            }
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            FieldKind::Text => "text",
            FieldKind::Integer => "int",
            FieldKind::Boolean => "bool",
            FieldKind::Enum => "enum",
            FieldKind::Date => "date",
            FieldKind::Stamp => "stamp",
            FieldKind::Path => "path",
            FieldKind::Commit => "commit",
            FieldKind::Email => "email",
            FieldKind::Reference => "ref",
            FieldKind::Slug => "slug",
        }
    }

    /// Check one value of a single (non multi) cell.
    pub fn check(self, value: &str) -> Result<()> {
        match self {
            FieldKind::Text | FieldKind::Reference => Ok(()),
            FieldKind::Integer => value
                .parse::<i64>()
                .map(|_| ())
                .map_err(|_| Error::validation(format!("`{value}` is not an integer"))),
            FieldKind::Boolean => match value {
                "true" | "false" => Ok(()),
                _ => Err(Error::validation(format!(
                    "`{value}` is not a boolean; use true or false"
                ))),
            },
            FieldKind::Slug => {
                if !value.is_empty()
                    && value
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    && !value.starts_with('-')
                    && !value.ends_with('-')
                {
                    Ok(())
                } else {
                    Err(Error::validation(
                        "slug must contain ASCII letters, digits or interior hyphens",
                    ))
                }
            }
            FieldKind::Enum => Ok(()),
            FieldKind::Date => {
                if time::is_date(value) {
                    Ok(())
                } else {
                    Err(Error::validation(format!(
                        "`{value}` is not a date; use YYYYMMDD"
                    )))
                }
            }
            FieldKind::Stamp => {
                if time::is_stamp(value) {
                    Ok(())
                } else {
                    Err(Error::validation(format!(
                        "`{value}` is not a stamp; use YYYYMMDD-HHMMSS"
                    )))
                }
            }
            FieldKind::Path => {
                // A path may step out of the workspace to reach another
                // repository, for example ../library-app/benchmarks/...; absolute
                // and drive qualified paths are refused.
                if value.starts_with('/') || value.contains(':') {
                    Err(Error::validation(format!(
                        "`{value}` must be relative; address another repository as ../<name>/..."
                    )))
                } else {
                    Ok(())
                }
            }
            FieldKind::Commit => {
                let ok = value.len() >= 7
                    && value.len() <= 40
                    && value.chars().all(|character| character.is_ascii_hexdigit());
                if ok {
                    Ok(())
                } else {
                    Err(Error::validation(format!("`{value}` is not a commit hash")))
                }
            }
            FieldKind::Email => {
                let ok = value.contains('@')
                    && !value.starts_with('@')
                    && !value.ends_with('@')
                    && !value.contains(' ');
                if ok {
                    Ok(())
                } else {
                    Err(Error::validation(format!("`{value}` is not an email")))
                }
            }
        }
    }
}

/// Who owns a field value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Filled by people.
    Spec,
    /// Filled by the tool.
    Status,
}

/// How a status field takes its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Auto {
    GitName,
    GitEmail,
    Person,
    Operator,
    Today,
    Clock,
    Now,
}

impl Auto {
    pub fn name(self) -> &'static str {
        match self {
            Self::GitName => "git_name",
            Self::GitEmail => "git_email",
            Self::Person => "person",
            Self::Operator => "operator",
            Self::Today => "today",
            Self::Clock => "clock",
            Self::Now => "now",
        }
    }
    fn parse(text: &str, context: &str) -> Result<Self> {
        Ok(match text {
            "git_name" => Auto::GitName,
            "git_email" => Auto::GitEmail,
            "person" => Auto::Person,
            "operator" => Auto::Operator,
            "today" => Auto::Today,
            "clock" => Auto::Clock,
            "now" => Auto::Now,
            other => {
                return Err(Error::usage(format!(
                    "{context}: unknown auto source `{other}`"
                )));
            }
        })
    }
}

/// A condition such as `state == done`.
#[derive(Clone, Debug)]
pub struct Condition {
    pub field: String,
    pub value: String,
}

impl Condition {
    fn parse(text: &str, context: &str) -> Result<Self> {
        let (field, value) = text.split_once("==").ok_or_else(|| {
            Error::usage(format!(
                "{context}: condition `{text}` must look like `field == value`"
            ))
        })?;
        Ok(Condition {
            field: field.trim().to_string(),
            value: value.trim().to_string(),
        })
    }

    pub fn holds(&self, record: &BTreeMap<String, String>) -> bool {
        record
            .get(&self.field)
            .map(|value| value == &self.value)
            .unwrap_or(false)
    }
}

use std::collections::BTreeMap;

/// One declared field.
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    pub role: Role,
    pub create_only: bool,
    pub immutable: bool,
    pub terminal_style: Option<crate::terminal::style::Spec>,
    pub multi: bool,
    pub registry: Option<String>,
    /// Kind whose single-field primary key is referenced.
    pub target: Option<String>,
    pub required: bool,
    pub required_when: Option<Condition>,
    pub default: Option<String>,
    pub auto: Option<Auto>,
    pub display: String,
}

impl Field {
    /// One parser for instance fields and workflow parameter constraints.
    pub fn parse(entry: &Table, context: &str) -> Result<Self> {
        decl::only_keys(
            entry,
            &[
                "name",
                "type",
                "role",
                "write",
                "immutable",
                "terminal_style",
                "multi",
                "registry",
                "target",
                "required",
                "required_when",
                "default",
                "auto",
                "display",
            ],
            "field",
        )?;
        let field_name = decl::required_text(entry, "name", "field")?;
        let kind_name = decl::text(entry, "type").unwrap_or_else(|| "text".to_string());
        let kind = FieldKind::parse(&kind_name, "field")?;
        let role = match decl::text(entry, "role").as_deref() {
            None | Some("spec") => Role::Spec,
            Some("status") => Role::Status,
            Some(other) => {
                return Err(Error::usage(format!(
                    "{context}.{field_name}: unknown role `{other}`"
                )));
            }
        };
        if entry.get("write").is_some_and(|value| !value.is_str()) {
            return Err(Error::usage(format!(
                "{context}.{field_name}: write must be a string"
            )));
        }
        let create_only = match decl::text(entry, "write").as_deref() {
            None => false,
            Some("create") if role == Role::Spec => true,
            Some("always") if role == Role::Spec => false,
            Some(_) => {
                return Err(Error::usage(format!(
                    "{context}.{field_name}: write must be create or always on a spec field"
                )));
            }
        };
        let registry = decl::text(entry, "registry");
        let target = decl::text(entry, "target");
        if kind == FieldKind::Enum && registry.is_none() {
            return Err(Error::usage(format!(
                "{context}.{field_name}: an enum field needs a registry"
            )));
        }
        if kind == FieldKind::Reference {
            if registry.is_some() == target.is_some() {
                return Err(Error::usage(format!(
                    "{context}.{field_name}: a ref field needs exactly one of registry or target"
                )));
            }
        } else if target.is_some() || (registry.is_some() && kind != FieldKind::Enum) {
            return Err(Error::usage(format!(
                "{context}.{field_name}: registry requires enum or ref; target requires ref"
            )));
        }
        let required_when = match decl::text(entry, "required_when") {
            Some(text) => Some(Condition::parse(&text, &format!("{context}.{field_name}"))?),
            None => None,
        };
        let auto = match decl::text(entry, "auto") {
            Some(text) => Some(Auto::parse(&text, &format!("{context}.{field_name}"))?),
            None => None,
        };
        if auto.is_some() && role == Role::Spec {
            return Err(Error::usage(format!(
                "{context}.{field_name}: an auto field must declare role = \"status\""
            )));
        }
        let display = decl::text(entry, "display").unwrap_or_else(|| field_name.clone());
        if entry.get("immutable").is_some_and(|value| !value.is_bool()) {
            return Err(Error::usage("field.immutable must be a boolean"));
        }
        Ok(Field {
            name: field_name,
            kind,
            role,
            create_only,
            immutable: decl::flag(entry, "immutable").unwrap_or(false),
            terminal_style: entry
                .get("terminal_style")
                .map(crate::terminal::style::Spec::parse)
                .transpose()?,
            multi: decl::flag(entry, "multi").unwrap_or(false),
            registry,
            target,
            required: decl::flag(entry, "required").unwrap_or(false),
            required_when,
            default: decl::text(entry, "default"),
            auto,
            display,
        })
    }
}

/// State machine attachment of a kind.
#[derive(Clone, Debug)]
pub struct StateSpec {
    pub field: String,
    pub registry: String,
    pub default: Option<String>,
}

/// A declared action other than a transition.
#[derive(Clone, Debug)]
pub struct Action {
    /// Optional target kind for structured append entry references.
    pub entry_target: Option<String>,
    pub name: String,
    /// Built in handler: `identity` or `clear`.
    pub handler: String,
    /// Fields the handler fills or clears.
    pub fields: Vec<String>,
    /// Field receiving the attachment path.
    pub field: Option<String>,
    /// Heading template of the append handler, with `{time}` and `{title}`.
    pub heading: Option<String>,
    /// File name template of an attachment.
    pub filename: Option<String>,
    /// States the instance must be in.
    pub states: Vec<String>,
    /// Fields that must be empty before the action runs.
    pub require_empty: Vec<String>,
    /// Refuse the action when the worktree has pending changes.
    pub require_clean: bool,
    /// Fields that must be filled before the action runs.
    pub requires: Vec<String>,
    /// Field values the `set` handler writes, each a template.
    pub values: std::collections::BTreeMap<String, String>,
}

/// Values of a `set` action: field name to template.
fn action_values(entry: &Table) -> std::collections::BTreeMap<String, String> {
    let mut values = std::collections::BTreeMap::new();
    if let Some(table) = decl::sub_table(entry, "values") {
        for (name, value) in &table {
            if let Some(text) = value.as_str() {
                values.insert(name.clone(), text.to_string());
            }
        }
    }
    values
}
/// A declared transition.
#[derive(Clone, Debug)]
pub struct Transition {
    pub name: String,
    pub to: String,
    pub from: Vec<String>,
    pub requires: Vec<String>,
    pub archive: bool,
}

/// A declared relation type.
#[derive(Clone, Debug)]
pub struct Relation {
    pub name: String,
    pub target: String,
}

/// One row of a merged vocabulary table.
#[derive(Clone, Debug)]
pub struct VocabEntry {
    pub display: String,
    pub registry: Option<String>,
    pub kind: Option<String>,
    pub multi: bool,
}

/// A declared projection.
#[derive(Clone, Debug)]
pub struct View {
    pub name: String,
    pub columns: Vec<String>,
    pub summary: bool,
    pub flex: Option<String>,
    pub priority: Vec<String>,
    pub empty: std::collections::BTreeMap<String, String>,
    /// Line template for the `lines` renderer.
    pub template: Option<String>,
    /// Whether enum values are shown as display text instead of keys.
    pub display: bool,
    /// Heading written above the block of a composite fragment.
    pub title: Option<String>,
    /// Fragment view names of a composite view, in output order.
    pub fragments: Vec<String>,
    /// Registry name or data file the renderer reads.
    pub source: Option<String>,
    /// Table headings, one per column.
    pub headers: Vec<String>,
    /// Rows of the vocabularies renderer.
    pub entries: Vec<VocabEntry>,
    /// Whether the details renderer lists the logs that mention the instance.
    pub logs: bool,
    /// Prefix written before each related log path.
    pub logs_prefix: Option<String>,
    /// Words the vocabularies renderer uses for a multi valued row.
    pub multi_display: Vec<String>,
    /// Separator between the values of one multi valued cell.
    pub separator: Option<String>,
    /// Field printed on an indented second line by the lines renderer.
    pub detail_field: Option<String>,
    /// Sort keys, a column name or `field:part` for a numbered identifier.
    pub sort: Vec<String>,
    pub renderer: Option<String>,
    pub target: Option<String>,
    pub region: Option<String>,
}

/// Lifecycle and governance settings of a kind.
#[derive(Clone, Debug)]
pub struct KindPolicy {
    pub archive_dir: String,
    /// Full archive directory, when the kind keeps its pages elsewhere.
    pub archive: Option<String>,
}

/// Index page listing the documents of a kind.
#[derive(Clone, Debug)]
pub struct IndexSpec {
    /// Workspace relative path of the index page.
    pub file: String,
    /// Columns of the table, where `file` stands for the document itself.
    pub columns: Vec<String>,
}

/// Content hash tracking of one file field.
#[derive(Clone, Debug)]
pub struct HashSpec {
    /// Field holding the workspace relative path.
    pub file: String,
    /// Field holding the hex digest, maintained by the tool.
    pub digest: String,
}

impl Default for KindPolicy {
    fn default() -> Self {
        KindPolicy {
            archive_dir: ".clerk/archive".to_string(),
            archive: None,
        }
    }
}

/// One kind.
#[derive(Clone, Debug)]
pub struct Kind {
    pub name: String,
    pub title: String,
    /// One line about what the kind holds.
    pub description: Option<String>,
    pub storage: Storage,
    pub key: Vec<String>,
    pub numbering: Option<String>,
    pub state: Option<StateSpec>,
    pub fields: Vec<Field>,
    pub transitions: Vec<Transition>,
    pub actions: Vec<Action>,
    pub relations: Vec<Relation>,
    pub relations_table: Option<String>,
    pub hash: Option<HashSpec>,
    pub index: Option<IndexSpec>,
    /// Body template of a document kind, relative to the workspace root.
    pub template: Option<String>,
    /// Headings every document of the kind must carry.
    pub sections: Vec<String>,
    /// Header template of a new document page: literal text or a `.md` path.
    /// Placeholders are `{date}`, `{maintainer}` and `{operator}`.
    pub header: Option<String>,
    /// Metadata keys that must be present.
    pub metadata_required: Vec<String>,
    /// Whether metadata keys outside the declared fields are refused.
    pub metadata_strict: bool,
    pub views: Vec<View>,
    pub policy: KindPolicy,
    pub source: String,
    pub context: Option<ContextSpec>,
}

#[derive(Clone, Debug)]
pub struct ContextSpec {
    pub attachments: Vec<String>,
    pub repos: Option<String>,
    pub logs: Option<String>,
}

impl Kind {
    pub fn parse(name: &str, source: &str, table: &Table) -> Result<Self> {
        let context = format!("kind `{name}`");
        decl::only_keys(
            table,
            &[
                "title",
                "description",
                "storage",
                "table",
                "key",
                "numbering",
                "state",
                "field",
                "transition",
                "action",
                "relation",
                "relations_table",
                "view",
                "archive_dir",
                "archive",
                "dir",
                "filename",
                "meta",
                "hash",
                "index",
                "template",
                "sections",
                "metadata",
                "header",
                "context",
            ],
            &context,
        )?;
        let title = decl::text(table, "title").unwrap_or_else(|| name.to_string());
        let storage_name = decl::text(table, "storage").unwrap_or_else(|| "table".to_string());
        let storage = match storage_name.as_str() {
            "table" => Storage::Table {
                path: decl::required_text(table, "table", &context)?,
            },
            "document" => Storage::Document {
                dir: decl::required_text(table, "dir", &context)?,
                filename: decl::text(table, "filename").unwrap_or_else(|| "{id}.md".to_string()),
                meta: match decl::text(table, "meta").as_deref() {
                    None | Some("block") => MetaStyle::Block,
                    Some("bullets") => MetaStyle::Bullets,
                    Some(other) => {
                        return Err(Error::usage(format!(
                            "{context}: unknown meta style `{other}`; expected block or bullets"
                        )));
                    }
                },
            },
            other => {
                return Err(Error::usage(format!(
                    "{context}: unknown storage `{other}`; expected table or document"
                )));
            }
        };
        let mut key = decl::strings(table, "key");
        if key.is_empty() {
            key.push("id".to_string());
        }
        let numbering = decl::text(table, "numbering");
        let state = match decl::sub_table(table, "state") {
            Some(section) => {
                decl::only_keys(&section, &["field", "registry", "default"], "state")?;
                Some(StateSpec {
                    field: decl::text(&section, "field").unwrap_or_else(|| "state".to_string()),
                    registry: decl::required_text(&section, "registry", "state")?,
                    default: decl::text(&section, "default"),
                })
            }
            None => None,
        };
        if let Some(spec) = &state
            && numbering.is_some()
            && spec.field.is_empty()
        {
            return Err(Error::usage(format!(
                "{context}: state.field must not be empty"
            )));
        }

        let mut fields = Vec::new();
        for entry in decl::array_of_tables(table, "field") {
            fields.push(Field::parse(&entry, &context)?);
        }
        if let Storage::Document { filename, .. } = &storage {
            for part in filename.split('{').skip(1) {
                let name = part.split('}').next().unwrap_or("");
                let field = fields
                    .iter()
                    .find(|field| field.name == name)
                    .ok_or_else(|| {
                        Error::usage(format!(
                            "{context}: filename field `{name}` is not declared"
                        ))
                    })?;
                let numbered = numbering.is_some() && key.first() == Some(&field.name);
                let derived = decl::array_of_tables(table, "action")
                    .iter()
                    .any(|action| decl::text(action, "handler").as_deref() == Some("append"));
                if field.role == Role::Status
                    && field.auto.is_none()
                    && field.default.is_none()
                    && !numbered
                    && !derived
                {
                    return Err(Error::usage(format!(
                        "{context}: filename field `{name}` is tool maintained but has no generation source"
                    )));
                }
            }
        }
        let mut seen = BTreeMap::new();
        for field in &fields {
            if field.name.starts_with('@') {
                return Err(Error::usage(
                    "field names starting with @ are reserved for query provenance",
                ));
            }
            if seen.insert(field.name.clone(), ()).is_some() {
                return Err(Error::usage(format!(
                    "{context}: field `{}` is declared twice",
                    field.name
                )));
            }
        }
        for name in &key {
            if !seen.contains_key(name) {
                return Err(Error::usage(format!(
                    "{context}: key field `{name}` is not declared"
                )));
            }
        }
        if let Some(spec) = &state
            && !seen.contains_key(&spec.field)
        {
            return Err(Error::usage(format!(
                "{context}: state field `{}` is not declared",
                spec.field
            )));
        }

        let mut transitions = Vec::new();
        for entry in decl::array_of_tables(table, "transition") {
            decl::only_keys(
                &entry,
                &["name", "to", "from", "requires", "archive"],
                "transition",
            )?;
            let to = decl::required_text(&entry, "to", "transition")?;
            let name = decl::text(&entry, "name").unwrap_or_else(|| to.clone());
            let from = decl::strings(&entry, "from");
            if from.is_empty() {
                return Err(Error::usage(format!(
                    "{context}: transition `{name}` needs at least one source state"
                )));
            }
            if state.is_none() {
                return Err(Error::usage(format!(
                    "{context}: transition `{name}` needs a [state] section"
                )));
            }
            let requires = decl::strings(&entry, "requires");
            for field in &requires {
                if !seen.contains_key(field) {
                    return Err(Error::usage(format!(
                        "{context}: transition `{name}` requires undeclared field `{field}`"
                    )));
                }
            }
            transitions.push(Transition {
                name,
                to,
                from,
                requires,
                archive: decl::flag(&entry, "archive").unwrap_or(false),
            });
        }

        let mut actions = Vec::new();
        for entry in decl::array_of_tables(table, "action") {
            decl::only_keys(
                &entry,
                &[
                    "name",
                    "handler",
                    "fields",
                    "field",
                    "filename",
                    "heading",
                    "entry_target",
                    "states",
                    "require_empty",
                    "require_clean",
                    "requires",
                    "values",
                ],
                "action",
            )?;
            let name = decl::required_text(&entry, "name", "action")?;
            let handler = decl::required_text(&entry, "handler", "action")?;
            if handler != "identity"
                && handler != "clear"
                && handler != "attach"
                && handler != "append"
                && handler != "prune"
                && handler != "set"
            {
                return Err(Error::usage(format!(
                    "{context}: action `{name}` uses unknown handler `{handler}`; expected identity, clear, attach, append, prune or set"
                )));
            }
            let field = decl::text(&entry, "field");
            let filename = decl::text(&entry, "filename");
            let heading = decl::text(&entry, "heading");
            let entry_target = decl::text(&entry, "entry_target");
            if entry.contains_key("entry_target") && (entry_target.is_none() || handler != "append")
            {
                return Err(Error::usage(
                    "entry_target requires a string on an append action",
                ));
            }
            let fields = decl::strings(&entry, "fields");
            let values = action_values(&entry);
            if handler == "append" {
                if heading.is_none() {
                    return Err(Error::usage(format!(
                        "{context}: append action `{name}` needs `heading`"
                    )));
                }
            } else if handler == "attach" {
                let named = field.clone().ok_or_else(|| {
                    Error::usage(format!("{context}: attach action `{name}` needs `field`"))
                })?;
                if !seen.contains_key(&named) {
                    return Err(Error::usage(format!(
                        "{context}: attach action `{name}` names undeclared field `{named}`"
                    )));
                }
                if filename.is_none() {
                    return Err(Error::usage(format!(
                        "{context}: attach action `{name}` needs `filename`"
                    )));
                }
            } else if handler == "set" {
                if values.is_empty() {
                    return Err(Error::usage(format!(
                        "{context}: set action `{name}` needs at least one value in `values`"
                    )));
                }
                for field in values.keys() {
                    if !seen.contains_key(field) {
                        return Err(Error::usage(format!(
                            "{context}: set action `{name}` names undeclared field `{field}`"
                        )));
                    }
                }
            } else if handler != "prune" && fields.is_empty() {
                return Err(Error::usage(format!(
                    "{context}: action `{name}` needs at least one field"
                )));
            }
            for field in &fields {
                if !seen.contains_key(field) {
                    return Err(Error::usage(format!(
                        "{context}: action `{name}` names undeclared field `{field}`"
                    )));
                }
            }
            actions.push(Action {
                entry_target,
                name,
                handler,
                fields,
                field,
                filename,
                heading,
                states: decl::strings(&entry, "states"),
                require_empty: decl::strings(&entry, "require_empty"),
                require_clean: decl::flag(&entry, "require_clean").unwrap_or(false),
                requires: decl::strings(&entry, "requires"),
                values,
            });
        }

        let mut relations = Vec::new();
        for entry in decl::array_of_tables(table, "relation") {
            decl::only_keys(&entry, &["name", "target"], "relation")?;
            relations.push(Relation {
                name: decl::required_text(&entry, "name", "relation")?,
                target: decl::required_text(&entry, "target", "relation")?,
            });
        }
        let relations_table = decl::text(table, "relations_table");

        let index = match decl::sub_table(table, "index") {
            Some(section) => {
                decl::only_keys(&section, &["file", "columns"], "index")?;
                let columns = decl::strings(&section, "columns");
                if columns.is_empty() {
                    return Err(Error::usage(format!(
                        "{context}: index.columns must list at least one column"
                    )));
                }
                Some(IndexSpec {
                    file: decl::required_text(&section, "file", "index")?,
                    columns,
                })
            }
            None => None,
        };

        let sections = match decl::sub_table(table, "sections") {
            Some(section) => {
                decl::only_keys(&section, &["required"], "sections")?;
                decl::strings(&section, "required")
            }
            None => Vec::new(),
        };
        let header = decl::text(table, "header");
        let (metadata_required, metadata_strict) = match decl::sub_table(table, "metadata") {
            Some(section) => {
                decl::only_keys(&section, &["required", "strict"], "metadata")?;
                (
                    decl::strings(&section, "required"),
                    decl::flag(&section, "strict").unwrap_or(false),
                )
            }
            None => (Vec::new(), false),
        };

        let hash = match decl::sub_table(table, "hash") {
            Some(section) => {
                decl::only_keys(&section, &["file", "digest", "algorithm"], "hash")?;
                let algorithm =
                    decl::text(&section, "algorithm").unwrap_or_else(|| "sha256".to_string());
                if algorithm != "sha256" {
                    return Err(Error::usage(format!(
                        "{context}: hash algorithm `{algorithm}` is not supported"
                    )));
                }
                let file = decl::required_text(&section, "file", "hash")?;
                let digest = decl::required_text(&section, "digest", "hash")?;
                if !seen.contains_key(&file) || !seen.contains_key(&digest) {
                    return Err(Error::usage(format!(
                        "{context}: hash fields `{file}` and `{digest}` must be declared"
                    )));
                }
                Some(HashSpec { file, digest })
            }
            None => None,
        };

        let mut views = Vec::new();
        for entry in decl::array_of_tables(table, "view") {
            // Each renderer accepts its own options, so a key written for one
            // renderer on another is reported instead of silently ignored.
            let renderer = decl::text(&entry, "renderer").unwrap_or_else(|| "table".to_string());
            let mut allowed = vec![
                "name", "columns", "renderer", "target", "region", "display", "title", "headers",
                "sort",
            ];
            let extra: &[&str] = match renderer.as_str() {
                "table" => &["layout", "empty", "flex", "priority"],
                "lines" => &["template", "detail_field"],
                "composite" => &["fragments"],
                "registry" | "focus" | "filetable" => &["source"],
                "details" => &["logs", "logs_prefix"],
                "vocabularies" => &["entries", "separator", "multi_display"],
                other => {
                    return Err(Error::usage(format!(
                        "{context}: view `{}` uses unknown renderer `{other}`",
                        decl::text(&entry, "name").unwrap_or_default()
                    )));
                }
            };
            allowed.extend_from_slice(extra);
            decl::only_keys(&entry, &allowed, "view")?;
            if entry.get("layout").is_some_and(|value| !value.is_str()) {
                return Err(Error::usage("view.layout must be a string"));
            }
            let summary = match decl::text(&entry, "layout").as_deref() {
                None | Some("table") => false,
                Some("summary") => true,
                Some(_) => return Err(Error::usage("view.layout must be table or summary")),
            };
            let mut empty = std::collections::BTreeMap::new();
            if let Some(value) = entry.get("empty") {
                let values = value
                    .as_table()
                    .ok_or_else(|| Error::usage("view.empty must be a table"))?;
                for (name, value) in values {
                    empty.insert(
                        name.clone(),
                        value
                            .as_str()
                            .ok_or_else(|| Error::usage("view.empty values must be strings"))?
                            .to_string(),
                    );
                }
            }
            let terminal = matches!(decl::text(&entry, "name").as_deref(), Some("list" | "long"))
                && !entry.contains_key("target");
            let flex = decl::text(&entry, "flex");
            let priority = decl::strings(&entry, "priority");
            if entry.get("flex").is_some_and(|value| !value.is_str())
                || entry.get("priority").is_some_and(|value| {
                    !value
                        .as_array()
                        .is_some_and(|values| values.iter().all(toml::Value::is_str))
                })
            {
                return Err(Error::usage(
                    "view.flex must be a string and view.priority an array of strings",
                ));
            }
            if (entry.contains_key("flex") || entry.contains_key("priority")) && !terminal {
                return Err(Error::usage(
                    "flex and priority require a terminal list or long view",
                ));
            }
            let columns = decl::strings(&entry, "columns");
            for name in priority.iter().chain(flex.iter()) {
                if !columns.contains(name) {
                    return Err(Error::usage(format!(
                        "view layout field `{name}` must appear in columns"
                    )));
                }
            }
            if priority
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != priority.len()
            {
                return Err(Error::usage("view.priority contains duplicate fields"));
            }
            if summary
                || entry.contains_key("empty")
                || (terminal
                    && (entry.contains_key("flex")
                        || entry.contains_key("priority")
                        || decl::text(&entry, "name").as_deref() == Some("long")))
            {
                if (entry.contains_key("empty") && !summary) || !terminal || columns.is_empty() {
                    return Err(Error::usage(
                        "terminal layout requires a list or long view with columns and no target; empty labels need summary layout",
                    ));
                }
                for name in decl::strings(&entry, "columns").iter().chain(empty.keys()) {
                    if !fields.iter().any(|field| &field.name == name) {
                        return Err(Error::usage(format!("view field `{name}` is not declared")));
                    }
                }
                if entry.get("columns").is_some_and(|value| {
                    !value
                        .as_array()
                        .is_some_and(|values| values.iter().all(toml::Value::is_str))
                }) {
                    return Err(Error::usage("view.columns must be an array of strings"));
                }
            }
            views.push(View {
                name: decl::required_text(&entry, "name", "view")?,
                columns: decl::strings(&entry, "columns"),
                summary,
                flex,
                priority,
                empty,
                template: decl::text(&entry, "template"),
                display: decl::flag(&entry, "display").unwrap_or(true),
                title: decl::text(&entry, "title"),
                fragments: decl::strings(&entry, "fragments"),
                source: decl::text(&entry, "source"),
                headers: decl::strings(&entry, "headers"),
                entries: decl::array_of_tables(&entry, "entries")
                    .into_iter()
                    .map(|item| {
                        decl::only_keys(
                            &item,
                            &["display", "registry", "kind", "multi"],
                            "entries",
                        )?;
                        Ok(VocabEntry {
                            display: decl::required_text(&item, "display", "entries")?,
                            registry: decl::text(&item, "registry"),
                            kind: decl::text(&item, "kind"),
                            multi: decl::flag(&item, "multi").unwrap_or(false),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                logs: decl::flag(&entry, "logs").unwrap_or(false),
                logs_prefix: decl::text(&entry, "logs_prefix"),
                multi_display: decl::strings(&entry, "multi_display"),
                separator: decl::text(&entry, "separator"),
                detail_field: decl::text(&entry, "detail_field"),
                sort: decl::strings(&entry, "sort"),
                renderer: decl::text(&entry, "renderer"),
                target: decl::text(&entry, "target"),
                region: decl::text(&entry, "region"),
            });
        }

        let policy = KindPolicy {
            archive_dir: decl::text(table, "archive_dir")
                .unwrap_or_else(|| KindPolicy::default().archive_dir),
            archive: decl::text(table, "archive"),
        };

        let context_spec = if let Some(entry) = table.get("context") {
            let entry = entry
                .as_table()
                .ok_or_else(|| Error::usage("context must be a table"))?;
            decl::only_keys(entry, &["attachments", "repos", "logs"], "context")?;
            if entry.get("attachments").is_some_and(|v| {
                !v.as_array()
                    .is_some_and(|a| a.iter().all(toml::Value::is_str))
            }) {
                return Err(Error::usage(
                    "context.attachments must be an array of fields",
                ));
            }
            for key in ["repos", "logs"] {
                if entry.get(key).is_some_and(|v| !v.is_str()) {
                    return Err(Error::usage(format!("context.{key} must be a string")));
                }
            }
            let attachments = decl::strings(entry, "attachments");
            for name in &attachments {
                if !fields
                    .iter()
                    .any(|field| &field.name == name && field.kind == FieldKind::Path)
                {
                    return Err(Error::usage(format!(
                        "context attachment `{name}` must be a path field"
                    )));
                }
            }
            let repos = decl::text(entry, "repos");
            if let Some(name) = &repos
                && !fields
                    .iter()
                    .any(|field| &field.name == name && field.registry.is_some())
            {
                return Err(Error::usage(format!(
                    "context repos `{name}` must have a registry"
                )));
            }
            Some(ContextSpec {
                attachments,
                repos,
                logs: decl::text(entry, "logs"),
            })
        } else {
            None
        };
        Ok(Kind {
            name: name.to_string(),
            title,
            description: decl::text(table, "description"),
            storage,
            key,
            numbering,
            state,
            fields,
            transitions,
            actions,
            relations,
            relations_table,
            hash,
            index,
            template: decl::text(table, "template"),
            sections,
            header,
            metadata_required,
            metadata_strict,
            views,
            policy,
            source: source.to_string(),
            context: context_spec,
        })
    }

    pub fn table_path(&self) -> &str {
        self.storage.location()
    }

    /// Directory holding document instances.
    pub fn document_dir(&self) -> Option<&str> {
        match &self.storage {
            Storage::Document { dir, .. } => Some(dir),
            Storage::Table { .. } => None,
        }
    }

    /// Table holding archived instances.
    pub fn archive_table(&self) -> String {
        format!("{}/{}.csv", self.policy.archive_dir, self.name)
    }

    /// Directory holding archived documents.
    pub fn archive_dir(&self) -> String {
        self.policy
            .archive
            .clone()
            .unwrap_or_else(|| format!("{}/{}", self.policy.archive_dir, self.name))
    }

    /// File name template of a document kind.
    pub fn document_filename(&self) -> Option<&str> {
        match &self.storage {
            Storage::Document { filename, .. } => Some(filename),
            Storage::Table { .. } => None,
        }
    }

    /// Metadata style of a document kind.
    pub fn meta_style(&self) -> MetaStyle {
        match &self.storage {
            Storage::Document { meta, .. } => *meta,
            Storage::Table { .. } => MetaStyle::Block,
        }
    }

    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.name == name)
    }

    pub fn action(&self, name: &str) -> Option<&Action> {
        self.actions.iter().find(|item| item.name == name)
    }

    pub fn transition(&self, name: &str) -> Option<&Transition> {
        self.transitions.iter().find(|item| item.name == name)
    }

    /// Field names used for the list view, in declaration order.
    pub fn list_columns(&self) -> Vec<String> {
        if let Some(view) = self.views.iter().find(|view| view.name == "list")
            && !view.columns.is_empty()
        {
            return view.columns.clone();
        }
        self.fields.iter().map(|field| field.name.clone()).collect()
    }

    pub fn spec_fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter().filter(|field| field.role == Role::Spec)
    }
}

/// All kinds of a workspace.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    kinds: Vec<Kind>,
}

impl Catalog {
    pub fn load(workspace: &Workspace) -> Result<Self> {
        let dir = workspace.kinds_dir();
        let mut kinds = Vec::new();
        if dir.exists() {
            let mut paths: Vec<_> = std::fs::read_dir(&dir)
                .map_err(|error| crate::foundation::error::io_error(&dir, error))?
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().map(|ext| ext == "toml").unwrap_or(false))
                .collect();
            paths.sort();
            for path in paths {
                let stem = path
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                let body = std::fs::read_to_string(&path)
                    .map_err(|error| crate::foundation::error::io_error(&path, error))?;
                let relative = workspace.relative(&path);
                let table = decl::parse(&body, &relative)?;
                kinds.push(Kind::parse(&stem, &relative, &table)?);
            }
        }
        let mut seen = BTreeMap::new();
        for kind in &kinds {
            if seen
                .insert(kind.name.clone(), kind.source.clone())
                .is_some()
            {
                return Err(Error::usage(format!(
                    "kind `{}` is declared twice",
                    kind.name
                )));
            }
        }
        let catalog = Catalog { kinds };
        for kind in catalog.kinds() {
            for name in kind
                .actions
                .iter()
                .filter_map(|action| action.entry_target.as_deref())
            {
                let target = catalog.require(name)?;
                if target.key.len() != 1 || target.field(&target.key[0]).is_none() {
                    return Err(Error::usage(format!(
                        "entry_target `{name}` needs one declared key field"
                    )));
                }
            }
            for field in &kind.fields {
                if let Some(name) = &field.target {
                    let target = catalog.require(name)?;
                    if target.key.len() != 1 || target.field(&target.key[0]).is_none() {
                        return Err(Error::usage(format!(
                            "kind `{}` field `{}`: target `{name}` needs one declared key field",
                            kind.name, field.name
                        )));
                    }
                }
            }
        }
        Ok(catalog)
    }

    pub fn kinds(&self) -> &[Kind] {
        &self.kinds
    }

    pub fn find(&self, name: &str) -> Option<&Kind> {
        let lowered = name.to_ascii_lowercase();
        self.kinds
            .iter()
            .find(|kind| kind.name.eq_ignore_ascii_case(name))
            .or_else(|| {
                self.kinds
                    .iter()
                    .find(|kind| kind.name.to_ascii_lowercase() == format!("{lowered}s"))
            })
            .or_else(|| {
                self.kinds.iter().find(|kind| {
                    let singular = kind.name.strip_suffix('s').unwrap_or(&kind.name);
                    singular.eq_ignore_ascii_case(name)
                })
            })
    }

    pub fn require(&self, name: &str) -> Result<&Kind> {
        self.find(name).ok_or_else(|| {
            let known: Vec<&str> = self.kinds.iter().map(|kind| kind.name.as_str()).collect();
            Error::usage(format!(
                "unknown kind `{name}`; known kinds: {}",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ))
        })
    }
}

/// Validate one value against a field declaration.
pub fn check_value(field: &Field, value: &str, registry: Option<&Registry>) -> Result<()> {
    if value.trim().is_empty() && field.kind == FieldKind::Reference {
        return Ok(());
    }
    if field.multi {
        if !value.trim().is_empty() && value.split('|').all(|item| item.trim().is_empty()) {
            return Err(Error::validation(format!(
                "field `{}` has no nonempty values",
                field.name
            )));
        }
        for item in value.split('|').filter(|item| !item.trim().is_empty()) {
            check_single(field, item.trim(), registry)?;
        }
        return Ok(());
    }
    check_single(field, value, registry)
}

fn check_single(field: &Field, value: &str, registry: Option<&Registry>) -> Result<()> {
    field.kind.check(value)?;
    if field.kind == FieldKind::Reference
        && let Some(registry) = registry
        && !registry.has(value)
    {
        return Err(Error::validation(format!(
            "`{value}` is not registered for field `{}`; expected a key in `{}`",
            field.name, registry.name
        )));
    }
    if field.kind == FieldKind::Enum {
        let Some(registry) = registry else {
            return Err(Error::usage(format!(
                "field `{}` needs a registry",
                field.name
            )));
        };
        if registry.key_of(value).is_none() {
            return Err(Error::validation(format!(
                "`{value}` is not registered for field `{}`; known values: {}",
                field.name,
                registry.keys().join(", ")
            )));
        }
    }
    Ok(())
}

/// Resolve the value of an auto field.
pub fn auto_value(
    auto: Auto,
    identity: &crate::config::identity::Identity,
    offset_hours: i32,
) -> String {
    match auto {
        Auto::GitName => identity.git_name.clone(),
        Auto::GitEmail => identity.git_email.clone(),
        Auto::Person => identity
            .person
            .clone()
            .unwrap_or_else(|| identity.git_name.clone()),
        Auto::Operator => identity
            .operator
            .clone()
            .unwrap_or_else(|| identity.display()),
        Auto::Clock => time::stamp(time::now_epoch(), offset_hours)[9..].to_string(),
        Auto::Today => time::date(time::now_epoch(), offset_hours),
        Auto::Now => time::stamp(time::now_epoch(), offset_hours),
    }
}

/// Find a kind file by name in a directory.
pub fn kind_path(workspace: &Workspace, name: &str) -> std::path::PathBuf {
    workspace.kinds_dir().join(format!("{name}.toml"))
}

/// Read a kind declaration from an explicit path.
pub fn load_kind(workspace: &Workspace, path: &Path) -> Result<Kind> {
    let body = std::fs::read_to_string(path)
        .map_err(|error| crate::foundation::error::io_error(path, error))?;
    let relative = workspace.relative(path);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let table = decl::parse(&body, &relative)?;
    Kind::parse(&stem, &relative, &table)
}

/// Enforce lifetime constraints independently of the mutation handler.
pub(crate) fn validate_change(
    kind: &Kind,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Result<()> {
    for field in kind.fields.iter().filter(|field| field.immutable) {
        if before.get(&field.name).map(String::as_str).unwrap_or("")
            != after.get(&field.name).map(String::as_str).unwrap_or("")
        {
            return Err(Error::validation(format!(
                "field `{}` is immutable after creation",
                field.name
            )));
        }
    }
    Ok(())
}
