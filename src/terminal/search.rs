//! Pager search tracks logical positions, not wrapped rows or terminal colors.

use crate::interface::cli::Args;
use crate::query::matching::Matcher;
use crate::terminal::reader::Document;
use crate::terminal::style::Span;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Hit {
    line: usize,
    range: Range<usize>,
    anchor: Option<(String, usize)>,
}

pub struct Search {
    pub input: crate::terminal::editor::Editor,
    notice: String,
    pub editing: bool,
    regex: bool,
    ignore: bool,
    committed: (String, bool, bool),
    hits: Vec<Hit>,
    current: Option<usize>,
    error: String,
    history: Vec<String>,
    completion: Option<(String, usize)>,
}

impl Search {
    pub fn new(args: &Args) -> Self {
        let regex = args.value("search-mode") == Some("regex");
        let ignore = args.value("case") != Some("sensitive");
        Self {
            input: Default::default(),
            notice: String::new(),
            editing: false,
            regex,
            ignore,
            committed: (String::new(), regex, ignore),
            hits: Vec::new(),
            current: None,
            error: String::new(),
            history: Vec::new(),
            completion: None,
        }
    }

    pub fn rebuild(&mut self, document: &Document) {
        let old = self.current.and_then(|index| self.hits.get(index)).cloned();
        self.error.clear();
        self.hits.clear();
        if !self.input.text.is_empty() {
            match Matcher::new(
                &self.input.text,
                if self.regex { "regex" } else { "literal" },
                self.ignore,
            ) {
                Ok(matcher) => {
                    for (line, text) in document.logical.iter().enumerate() {
                        self.hits
                            .extend(matcher.ranges(text).into_iter().map(|range| Hit {
                                line,
                                range,
                                anchor: document.logical_anchor(line),
                            }));
                    }
                }
                Err(error) => self.error = error.to_string(),
            }
        }
        self.current = old
            .and_then(|old| {
                self.hits.iter().position(|hit| {
                    hit.range == old.range
                        && if old.anchor.is_some() {
                            hit.anchor == old.anchor
                        } else {
                            hit.line == old.line
                        }
                })
            })
            .or_else(|| (!self.hits.is_empty()).then_some(0));
    }

    pub fn row(&self, document: &Document) -> Option<usize> {
        let hit = self.hits.get(self.current?)?;
        document.layout.iter().position(|row| {
            row.line == hit.line
                && row.range.start <= hit.range.start
                && row.range.end > hit.range.start
        })
    }

    pub fn spans(&self, document: &Document, row: usize) -> Vec<Span> {
        let Some(mapped) = document.layout.get(row) else {
            return Vec::new();
        };
        self.hits
            .iter()
            .enumerate()
            .filter_map(|(index, hit)| {
                if hit.line != mapped.line {
                    return None;
                }
                let start = hit.range.start.max(mapped.range.start);
                let end = hit.range.end.min(mapped.range.end);
                (start < end).then(|| Span {
                    range: start - mapped.range.start + mapped.prefix
                        ..end - mapped.range.start + mapped.prefix,
                    role: if self.current == Some(index) {
                        "current"
                    } else {
                        "match"
                    }
                    .into(),
                })
            })
            .collect()
    }

    pub fn status(&self, document: &Document, top: usize, page: usize) -> String {
        let mode = format!(
            "{} {}",
            if self.regex { "regex" } else { "literal" },
            if self.ignore { "icase" } else { "case" }
        );
        let prefix = if self.editing { "Search: " } else { "Search " };
        let visible_query = crate::terminal::text::truncate(
            &crate::terminal::text::visible(&self.input.text, 0),
            24,
        );
        if !self.error.is_empty() {
            return format!(
                "{prefix}{} | {mode} | {}",
                visible_query,
                self.error.replace(['\n', '\r'], " ")
            );
        }
        if self.input.text.is_empty() {
            return if self.editing {
                format!("Search: | {mode} | Alt+r regex Alt+c case Tab history")
            } else {
                String::new()
            };
        }
        let result = if let Some(index) = self.current {
            let row = self.row(document).unwrap_or(0);
            format!(
                "match {}/{} @ row {}{}",
                index + 1,
                self.hits.len(),
                row + 1,
                if row < document.header + top || row >= document.header + top + page {
                    " off-screen"
                } else {
                    ""
                }
            )
        } else {
            "no matches".into()
        };
        format!(
            "{prefix}\"{}\"{} | {result} | {mode}",
            visible_query,
            if self.notice.is_empty() {
                String::new()
            } else {
                format!(" | {}", self.notice)
            }
        )
    }

    /// Some means handled; true requests scrolling the selected match into view.
    pub fn key(&mut self, key: KeyEvent, document: &Document) -> Option<bool> {
        self.notice.clear();
        if key.modifiers.contains(KeyModifiers::ALT) && matches!(key.code, KeyCode::Char('r' | 'c'))
        {
            if key.code == KeyCode::Char('r') {
                self.regex = !self.regex;
            } else {
                self.ignore = !self.ignore;
            }
            self.rebuild(document);
            if !self.editing {
                self.committed = (self.input.text.clone(), self.regex, self.ignore);
            }
            return Some(false);
        }
        if !self.editing {
            match key.code {
                KeyCode::Esc if !self.input.text.is_empty() => {
                    self.input.set(String::new());
                    self.committed = (String::new(), self.regex, self.ignore);
                    self.completion = None;
                    self.rebuild(document);
                    Some(false)
                }
                KeyCode::Char('/') => {
                    self.editing = true;
                    self.input.set(self.input.text.clone());
                    self.notice.clear();
                    self.completion = None;
                    self.rebuild(document);
                    Some(false)
                }
                KeyCode::Char('n' | 'N') | KeyCode::Enter if !self.input.text.is_empty() => {
                    self.notice.clear();
                    if !self.hits.is_empty() {
                        let index = self.current.unwrap_or(0);
                        if key.code != KeyCode::Char('N') && index + 1 == self.hits.len() {
                            self.notice = "Wrapped to first match".into();
                        }
                        if key.code == KeyCode::Char('N') && index == 0 {
                            self.notice = "Wrapped to last match".into();
                        }
                        self.current = Some(if key.code == KeyCode::Char('N') {
                            (index + self.hits.len() - 1) % self.hits.len()
                        } else {
                            (index + 1) % self.hits.len()
                        });
                    }
                    Some(true)
                }
                _ => None,
            }
        } else {
            if key.code != KeyCode::Tab {
                self.completion = None;
            }
            match key.code {
                KeyCode::Esc => {
                    let (text, regex, ignore) = self.committed.clone();
                    self.input.set(text);
                    self.regex = regex;
                    self.ignore = ignore;
                    self.editing = false;
                    self.notice.clear();
                }
                KeyCode::Enter => {
                    self.rebuild(document);
                    if !self.error.is_empty() {
                        return Some(false);
                    }
                    self.committed = (self.input.text.clone(), self.regex, self.ignore);
                    self.editing = false;
                    if !self.input.text.is_empty() {
                        self.history.retain(|value| *value != self.input.text);
                        self.history.push(self.input.text.clone());
                    }
                    return Some(true);
                }
                KeyCode::Tab => {
                    let (prefix, index) = self
                        .completion
                        .get_or_insert_with(|| (self.input.text.clone(), 0));
                    let candidates = self
                        .history
                        .iter()
                        .filter(|value| value.to_lowercase().starts_with(&prefix.to_lowercase()))
                        .collect::<Vec<_>>();
                    if !candidates.is_empty() {
                        self.input
                            .set(candidates[*index % candidates.len()].clone());
                        *index += 1;
                    }
                }
                _ => self.input.key(key),
            }
            self.rebuild(document);
            Some(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn escape_cancels_edits_then_clears_search_without_requesting_a_scroll() {
        let document = Document::reading("alpha beta alpha\n".into(), Some(20));
        let mut search = Search::new(&Args::default());
        search.key(key(KeyCode::Char('/')), &document);
        search.input.set("alpha".into());
        search.key(key(KeyCode::Enter), &document);
        search.key(key(KeyCode::Char('/')), &document);
        search.input.set("beta".into());
        assert_eq!(search.key(key(KeyCode::Esc), &document), Some(false));
        assert_eq!(search.input.text, "alpha");
        assert_eq!(search.key(key(KeyCode::Esc), &document), Some(false));
        assert!(search.status(&document, 0, 10).is_empty());
        assert!(search.spans(&document, 0).is_empty());
        assert_eq!(search.key(key(KeyCode::Esc), &document), None);
        search.key(key(KeyCode::Char('/')), &document);
        search.key(key(KeyCode::Tab), &document);
        assert_eq!(search.input.text, "alpha");
        search.key(key(KeyCode::Char('q')), &document);
        assert_eq!(search.input.text, "alphaq");
    }

    #[test]
    fn search_edits_in_place_and_enter_advances_only_after_confirmation() {
        let doc = Document::reading("alpha alpha\n中文中文\n".into(), Some(20));
        let mut search = Search::new(&Args::default());
        assert!(search.key(key(KeyCode::Enter), &doc).is_none());
        search.key(key(KeyCode::Char('/')), &doc);
        search.input.insert("alpa");
        search.key(key(KeyCode::Left), &doc);
        search.key(key(KeyCode::Char('h')), &doc);
        assert_eq!(search.input.text, "alpha");
        search.key(key(KeyCode::Enter), &doc);
        assert!(search.status(&doc, 0, 10).contains("match 1/2"));
        search.key(key(KeyCode::Enter), &doc);
        assert!(search.status(&doc, 0, 10).contains("match 2/2"));
        search.key(key(KeyCode::Enter), &doc);
        assert!(
            search
                .status(&doc, 0, 10)
                .contains("Wrapped to first match")
        );
        search.key(key(KeyCode::Char('/')), &doc);
        assert_eq!(search.input.text, "alpha");
        search.key(key(KeyCode::Home), &doc);
        search.key(key(KeyCode::Delete), &doc);
        search.key(key(KeyCode::End), &doc);
        search.key(key(KeyCode::Backspace), &doc);
        assert_eq!(search.input.text, "lph");
        search.key(key(KeyCode::Esc), &doc);
        assert_eq!(search.input.text, "alpha");
        assert!(!search.editing);
        let mut colored = doc.clone();
        colored.styles[0] = vec![Span {
            range: 0..11,
            role: "warning".into(),
        }];
        search.rebuild(&colored);
        assert_eq!(search.hits.len(), 2);
        let mut spans = colored.visual_spans(0);
        spans.extend(search.spans(&colored, 0));
        let output = crate::terminal::style::paint(&colored.logical[0], &spans, true);
        assert!(output.contains("\x1b[33;1;7malpha"));
        assert_eq!(
            crate::terminal::style::paint(&colored.logical[0], &spans, false),
            "alpha alpha"
        );
    }

    #[test]
    fn occurrence_counts_survive_wrapping_and_history_cancel_and_errors_are_visible() {
        let document = Document::reading("Clerk Clerk and more\nlast\n".into(), Some(10));
        let mut search = Search::new(&Args::default());
        search.key(key(KeyCode::Char('/')), &document);
        for ch in "clerk".chars() {
            search.key(key(KeyCode::Char(ch)), &document);
        }
        search.key(key(KeyCode::Enter), &document);
        assert!(search.status(&document, 0, 10).contains("match 1/2"));
        search.key(key(KeyCode::Char('n')), &document);
        assert!(search.status(&document, 0, 10).contains("match 2/2"));
        search.rebuild(&document.reading_width(30));
        assert_eq!(search.hits.len(), 2);
        search.key(key(KeyCode::Char('/')), &document);
        search.key(key(KeyCode::Tab), &document);
        assert_eq!(search.input.text, "clerk");
        search.key(key(KeyCode::Esc), &document);
        assert_eq!(search.input.text, "clerk");
        search.key(key(KeyCode::Char('/')), &document);
        search.key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &document,
        );
        search.key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT),
            &document,
        );
        search.key(key(KeyCode::Char('[')), &document);
        search.key(key(KeyCode::Enter), &document);
        assert!(search.editing && search.status(&document, 0, 10).contains("invalid search"));
    }
}
