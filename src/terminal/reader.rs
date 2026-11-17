//! Screen budgets and paging over complete, already rendered documents.

use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};

use crossterm::{
    Command,
    cursor::{Hide, MoveTo, MoveToColumn, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    style::Print,
    terminal::{
        self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};

use crate::foundation::error::{Error, Result};
use crate::interface::cli::Args;
use crate::terminal::style::Span;
use crate::terminal::text::{self, Reading};

/// Projections provide boundaries; the screen layer never guesses field names.
#[derive(Clone, Debug)]
pub struct Document {
    pub text: String,
    pub header: usize,
    pub ends: Vec<usize>,
    pub logical: Vec<String>,
    pub layout: Vec<text::ReadingRow>,
    pub styles: Vec<Vec<Span>>,
    pub identities: Vec<String>,
    base_header: usize,
    base_ends: Vec<usize>,
    projected: bool,
}

impl From<String> for Document {
    fn from(text: String) -> Self {
        let logical = text.lines().map(str::to_string).collect::<Vec<_>>();
        let layout = logical
            .iter()
            .enumerate()
            .map(|(line, text)| text::ReadingRow {
                line,
                range: 0..text.len(),
                prefix: 0,
            })
            .collect();
        let styles = logical
            .iter()
            .map(|line| crate::terminal::style::plain(line))
            .collect();
        Self {
            projected: false,
            text,
            header: 0,
            ends: Vec::new(),
            logical,
            layout,
            styles,
            identities: Vec::new(),
            base_header: 0,
            base_ends: Vec::new(),
        }
    }
}

impl Document {
    pub fn logical_anchor(&self, line: usize) -> Option<(String, usize)> {
        let index = self.base_ends.iter().position(|end| *end > line)?;
        let start = if index == 0 {
            self.base_header
        } else {
            self.base_ends[index - 1]
        };
        Some((
            self.identities.get(index)?.clone(),
            line.saturating_sub(start),
        ))
    }

    fn anchor(&self, top: usize) -> Option<String> {
        let index = self.ends.iter().position(|end| *end > self.header + top)?;
        self.identities.get(index).cloned()
    }

    fn anchor_top(&self, anchor: Option<&str>) -> usize {
        anchor
            .and_then(|id| self.identities.iter().position(|key| key == id))
            .map(|i| {
                if i == 0 {
                    0
                } else {
                    self.ends[i - 1].saturating_sub(self.header)
                }
            })
            .unwrap_or(0)
    }
    /// A structured projection supplies its own lossless source-to-screen map.
    pub fn projected(
        text: String,
        logical: Vec<String>,
        layout: Vec<text::ReadingRow>,
        styles: Vec<Vec<Span>>,
    ) -> Self {
        let mut document = Self::from(text);
        document.logical = logical;
        document.layout = layout;
        document.styles = styles;
        document.projected = true;
        document
    }

    pub fn reading(text: String, width: Option<usize>) -> Self {
        let document: Self = text.into();
        if let Some(width) = width {
            document.screen(width)
        } else {
            document
        }
    }

    pub fn table(text: String, header: usize, count: usize) -> Self {
        let mut document: Self = text.into();
        document.header = header;
        document.base_header = header;
        document.ends = (1..=count).map(|index| header + index).collect();
        document.base_ends = document.ends.clone();
        for line in 0..header.min(document.styles.len()) {
            document.styles[line] = vec![Span {
                range: 0..document.logical[line].len(),
                role: if line == 0 { "heading" } else { "muted" }.into(),
            }];
        }
        document
    }

    pub fn blocks(blocks: Vec<String>) -> Self {
        let mut text = String::new();
        let mut ends = Vec::new();
        let mut lines = 0;
        for block in blocks {
            lines += block.lines().count();
            text.push_str(&block);
            ends.push(lines);
        }
        let mut document: Self = text.into();
        document.ends = ends.clone();
        document.base_ends = ends;
        document
    }

    pub fn visual_spans(&self, row: usize) -> Vec<Span> {
        let Some(mapped) = self.layout.get(row) else {
            return Vec::new();
        };
        crate::terminal::style::project(
            &self.logical[mapped.line],
            self.styles
                .get(mapped.line)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            mapped,
        )
    }

    pub fn colored(&self, enabled: bool) -> String {
        self.paint(&self.text, enabled)
    }

    fn paint(&self, output: &str, enabled: bool) -> String {
        let original = self.text.lines().collect::<Vec<_>>();
        output
            .split_inclusive('\n')
            .enumerate()
            .map(|(row, line)| {
                let text = line.strip_suffix('\n').unwrap_or(line);
                let spans = if original.get(row).copied() == Some(text) {
                    self.visual_spans(row)
                } else {
                    crate::terminal::style::plain(text)
                };
                format!(
                    "{}{}",
                    crate::terminal::style::paint(text, &spans, enabled),
                    if line.ends_with('\n') { "\n" } else { "" }
                )
            })
            .collect()
    }

    fn count(&self) -> (usize, &'static str) {
        if self.ends.is_empty() {
            (self.text.lines().count(), "lines")
        } else {
            (self.ends.len(), "items")
        }
    }

    /// Convert physical lines into screen rows, preserving record boundaries.
    pub fn detail_styles(
        &mut self,
        context: &crate::execution::context::Context,
        kind: &crate::config::kind::Kind,
        records: &[crate::storage::store::Record],
        columns: &[String],
        display: bool,
    ) -> Result<()> {
        for (line, text) in self.logical.iter().enumerate() {
            let Some((label, body)) = text.split_once(": ") else {
                continue;
            };
            let record_index = self
                .base_ends
                .iter()
                .filter(|end| **end <= line)
                .count()
                .min(records.len().saturating_sub(1));
            let Some(record) = records.get(record_index) else {
                continue;
            };
            let Some(field) = kind.fields.iter().find(|field| {
                columns.contains(&field.name)
                    && (label == field.name || (display && label == field.display))
            }) else {
                continue;
            };
            let raw = record.get(&field.name).map(String::as_str).unwrap_or("");
            let role = field
                .terminal_style
                .as_ref()
                .and_then(|style| style.role(raw))
                .or_else(|| kind.key.contains(&field.name).then_some("identifier"));
            if let Some(role) = role {
                let padding = body.len() - body.trim_start().len();
                self.styles[line].push(Span {
                    range: label.len() + 2 + padding..text.len(),
                    role: role.into(),
                });
            }
        }
        let _ = context;
        Ok(())
    }

    pub fn reading_width(&self, width: usize) -> Self {
        self.screen(width)
    }

    fn screen(&self, width: usize) -> Self {
        if self.projected
            && self
                .text
                .lines()
                .all(|line| text::display_width(line) <= width)
        {
            return self.clone();
        }
        let source = if self.logical.is_empty() {
            String::new()
        } else {
            format!("{}\n", self.logical.join("\n"))
        };
        let (text, logical, layout) = Reading::new(Some(width)).mapped(&source);
        let mut offsets = vec![0; logical.len() + 1];
        for row in &layout {
            offsets[row.line + 1] += 1;
        }
        for line in 1..offsets.len() {
            offsets[line] += offsets[line - 1];
        }
        Self {
            projected: false,
            text,
            logical,
            layout,
            styles: self.styles.clone(),
            identities: self.identities.clone(),
            header: offsets[self.base_header.min(offsets.len() - 1)],
            ends: self
                .base_ends
                .iter()
                .map(|end| offsets[(*end).min(offsets.len() - 1)])
                .collect(),
            base_header: self.base_header,
            base_ends: self.base_ends.clone(),
        }
    }

    fn range(&self, top: usize, page: usize) -> String {
        if self.ends.is_empty() {
            let total = self.text.lines().count().saturating_sub(self.header);
            format!(
                "{}-{}/{} lines",
                (top + 1).min(total),
                (top + page).min(total),
                total
            )
        } else {
            let first = self
                .ends
                .iter()
                .filter(|end| **end <= self.header + top)
                .count();
            let last = self
                .ends
                .iter()
                .filter(|end| **end < self.header + top + page)
                .count()
                + 1;
            format!(
                "{}-{}/{} items",
                (first + 1).min(self.ends.len()),
                last.min(self.ends.len()),
                self.ends.len()
            )
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Full,
    Truncate,
    More,
    Pager,
}

fn mode(value: Option<&str>, machine: bool, output_tty: bool, input_tty: bool) -> Mode {
    if machine || !output_tty {
        return Mode::Full;
    }
    match value {
        Some("full") => Mode::Full,
        Some("more") if input_tty => Mode::More,
        Some("more") => Mode::Full,
        Some("pager") if input_tty => Mode::Pager,
        Some("pager") => Mode::Full,
        Some("truncate") => Mode::Truncate,
        _ => Mode::Full,
    }
}

pub fn validate(args: &Args) -> Result<()> {
    crate::interface::command::validate_requirements(&args.values)
}

fn dimensions() -> (usize, usize) {
    terminal_size::terminal_size_of(io::stdout())
        .map(
            |(terminal_size::Width(width), terminal_size::Height(height))| {
                (usize::from(width).max(1), usize::from(height).max(1))
            },
        )
        .unwrap_or((80, 24))
}

/// Only record projections provide this optional control; plain text has no sorting.
#[derive(Clone)]
pub struct Sorting {
    pub keys: Vec<String>,
    pub desc: bool,
    pub candidates: Vec<String>,
}

pub fn present(
    args: &Args,
    machine: bool,
    mut render: impl FnMut(Option<usize>) -> Result<Document>,
) -> Result<()> {
    controlled(args, machine, None, |width, _| render(width))
}

/// The closure can rebuild the projection after a window resize, without rereading data.
pub fn controlled(
    args: &Args,
    machine: bool,
    sorting: Option<Sorting>,
    mut render: impl FnMut(Option<usize>, Option<&Sorting>) -> Result<Document>,
) -> Result<()> {
    validate(args)?;
    let color = crate::terminal::style::enabled(args, machine);
    let output_tty = io::stdout().is_terminal();
    let mut selected = mode(
        args.value("overflow"),
        machine,
        output_tty,
        io::stdin().is_terminal(),
    );
    if args.value("paging") == Some("never") && matches!(selected, Mode::Pager | Mode::More) {
        selected = Mode::Full;
    }
    let (width, height) = dimensions();
    let requested = args
        .value("width")
        .and_then(|value| value.parse::<usize>().ok());
    let width_for = |actual| {
        if args.switch("plain") {
            None
        } else {
            requested
                .map(|width| width.min(actual))
                .or(output_tty.then_some(actual))
        }
    };
    // A redirected stream receives the exact full projection, including its explicit width.
    let document = render(
        if output_tty {
            width_for(width)
        } else {
            requested
        },
        sorting.as_ref(),
    )?;
    let output = match selected {
        Mode::Full => document.paint(&document.text, color),
        Mode::Truncate => document.paint(&truncate(&document, width, height), color),
        Mode::Pager | Mode::More => {
            if args.value("paging") != Some("always")
                && document.text.lines().count() <= height.saturating_sub(2)
                && document
                    .text
                    .lines()
                    .all(|line| text::display_width(line) <= width)
            {
                document.paint(&document.text, color)
            } else if selected == Mode::More {
                return more(
                    document,
                    width,
                    height,
                    args.value("paging") == Some("always"),
                    color,
                );
            } else {
                return pager(document, args, color, sorting, |width, sort| {
                    render(width_for(width), sort)
                });
            }
        }
    };
    io::stdout()
        .write_all(output.as_bytes())
        .map_err(|error| Error::io(format!("cannot display output: {error}")))
}

pub fn plain(args: &Args, text: &str) -> Result<()> {
    present(args, false, |width| {
        Ok(Document::reading(text.to_string(), width))
    })
}

/// Help is dispatched before normal command default resolution.
pub fn help(
    context: Option<&crate::execution::context::Context>,
    args: &Args,
    page: &crate::terminal::help::Page,
) -> Result<()> {
    let mut args = args.clone();
    if let Some(context) = context {
        crate::interface::cli::apply_defaults(&mut args, &context.command_defaults("help", ""));
    }
    present(&args, false, |width| {
        Ok(page.render(width, args.switch("compact")))
    })
}

fn truncate(document: &Document, width: usize, height: usize) -> String {
    let lines = document.text.lines().collect::<Vec<_>>();
    // Count visual rows even when --plain requested unrestricted horizontal output.
    let cost = |line: &str| text::display_width(line).max(1).div_ceil(width.max(1));
    if lines.iter().map(|line| cost(line)).sum::<usize>() <= height.saturating_sub(2) {
        return document.text.clone();
    }
    let (total, noun) = document.count();
    let reading = Reading::new(Some(width));
    let footer = |shown, partial| {
        let count = format!(
            "{shown}/{total} {noun}{}",
            if partial { " (partial)" } else { "" }
        );
        let line = format!("{count}; --overflow full or --overflow pager");
        if text::display_width(&line) <= width {
            format!("{line}\n")
        } else {
            format!(
                "{}--overflow full\n--overflow pager\n",
                reading.render(&count)
            )
        }
    };
    let reserve = footer(total, true).lines().count() + 2;
    let budget = height.saturating_sub(reserve);
    let mut used = 0;
    let mut end = 0;
    for line in &lines {
        if used + cost(line) > budget {
            break;
        }
        used += cost(line);
        end += 1;
    }
    let mut partial = false;
    if !document.ends.is_empty() {
        let whole = document
            .ends
            .iter()
            .copied()
            .filter(|boundary| *boundary <= end)
            .max();
        if let Some(boundary) = whole {
            end = boundary;
        } else {
            partial = end > document.header;
        }
    }
    let shown = if document.ends.is_empty() {
        end
    } else {
        document
            .ends
            .iter()
            .filter(|boundary| **boundary <= end)
            .count()
    };
    let mut out = String::new();
    for line in &lines[..end] {
        out.push_str(line);
        out.push('\n');
    }
    let used = lines[..end].iter().map(|line| cost(line)).sum::<usize>();
    let remaining = height.saturating_sub(2 + used);
    // Extremely small terminals must still stay bounded.
    for line in footer(shown, partial).lines().take(remaining) {
        out.push_str(&text::truncate(line, width));
        out.push('\n');
    }
    out
}

/// Restores the main screen and console mode on normal exit and every error path.
struct Screen;

impl Screen {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let screen = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            Hide,
            EnableBracketedPaste
        )?;
        Ok(screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            EndSynchronizedUpdate,
            Show,
            LeaveAlternateScreen,
            DisableBracketedPaste
        );
        let _ = terminal::disable_raw_mode();
    }
}

#[derive(Default)]
struct Position {
    top: usize,
}

impl Position {
    fn key(&mut self, code: KeyCode, page: usize, total: usize, _lines: &[String]) -> bool {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Up => self.top = self.top.saturating_sub(1),
            KeyCode::Down | KeyCode::Enter => self.top = self.top.saturating_add(1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.top = self.top.saturating_add(page),
            KeyCode::PageUp => self.top = self.top.saturating_sub(page),
            KeyCode::Home => self.top = 0,
            KeyCode::End => self.top = total.saturating_sub(page),
            _ => {}
        }
        self.top = self.top.min(total.saturating_sub(page));
        false
    }
}

/// Front and back frames share terminal-cell geometry, independent of content.
struct Frame {
    width: usize,
    rows: Vec<String>,
    styles: Vec<Vec<Span>>,
    color: bool,
}

impl Frame {
    /// Encode one complete update in memory, without issuing console calls.
    fn diff(&self, previous: Option<&Self>) -> String {
        let full = previous
            .is_none_or(|front| front.width != self.width || front.rows.len() != self.rows.len());
        let mut update = String::new();
        for (row, line) in self.rows.iter().enumerate() {
            if !full
                && previous.is_some_and(|front| {
                    front.rows[row] == *line
                        && front.color == self.color
                        && (!self.color || front.styles.get(row) == self.styles.get(row))
                })
            {
                continue;
            }
            update.push_str(&format!(
                "{}{}",
                MoveTo(0, row as u16),
                Print(crate::terminal::style::paint(
                    line,
                    self.styles.get(row).map(Vec::as_slice).unwrap_or_default(),
                    self.color
                ))
            ));
            // Clearing at a full-width right margin would erase the last cell.
            if text::display_width(line) < self.width {
                update.push_str(&Clear(ClearType::UntilNewLine).to_string());
            }
        }
        if update.is_empty() {
            update
        } else {
            let mut frame = String::new();
            BeginSynchronizedUpdate
                .write_ansi(&mut frame)
                .expect("write into String");
            frame.push_str(&update);
            EndSynchronizedUpdate
                .write_ansi(&mut frame)
                .expect("write into String");
            frame
        }
    }
}

fn pager(
    mut document: Document,
    args: &Args,
    color: bool,
    mut sorting: Option<Sorting>,
    mut render: impl FnMut(usize, Option<&Sorting>) -> Result<Document>,
) -> Result<()> {
    let mut run = || -> Result<()> {
        let _screen = Screen::enter().map_err(|error| {
            Error::io(format!("cannot open pager: {error}; use --overflow full"))
        })?;
        let mut position = Position::default();
        let mut front = None;
        let mut search = crate::terminal::search::Search::new(args);
        let mut sort_input = crate::terminal::editor::Editor::default();
        let mut sort_editing = false;
        let mut sort_error = String::new();
        let initial_sort = sorting.clone();
        let mut completion: Option<(String, usize)> = None;
        document = document.screen(dimensions().0);
        loop {
            let (width, height) = dimensions();
            let lines = document
                .text
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>();
            let header = document
                .header
                .min(lines.len())
                .min(height.saturating_sub(2));
            let body = &lines[document.header.min(lines.len())..];
            let reserved = if sort_editing || search.editing { 2 } else { 1 };
            let page = height.saturating_sub(header + reserved).max(1);
            position.top = position.top.min(body.len().saturating_sub(page));
            let mut back = Frame {
                width,
                rows: vec![String::new(); height],
                styles: vec![Vec::new(); height],
                color,
            };
            for (row, line) in lines[..header]
                .iter()
                .chain(body.iter().skip(position.top).take(page))
                .take(height.saturating_sub(1))
                .enumerate()
            {
                back.rows[row] = text::truncate(&text::visible(line, 0), width);
                let source_row = if row < header {
                    row
                } else {
                    document.header + position.top + row - header
                };
                back.styles[row] = document.visual_spans(source_row);
                back.styles[row].extend(search.spans(&document, source_row));
            }
            let search_status = search.status(&document, position.top, page);
            let status = if !search_status.is_empty() {
                format!(
                    "Esc clear | {search_status} | {} | Enter/n next N previous | q quit",
                    document.range(position.top, page)
                )
            } else {
                format!(
                    "{} | q/Esc quit | / search | arrows PgUp/PgDn Home/End",
                    document.range(position.top, page)
                )
            };
            back.rows[height.saturating_sub(1)] = text::truncate(&status, width);
            back.styles[height.saturating_sub(1)] = vec![Span {
                range: 0..back.rows[height.saturating_sub(1)].len(),
                role: "heading".into(),
            }];
            if let Some(sort) = &sorting {
                let status_row = height.saturating_sub(1);
                back.rows[status_row] = text::truncate(
                    &if search_status.is_empty() {
                        format!(
                            "{} | Sort {}{} | s sort r reverse R reset | / search | q/Esc quit",
                            document.range(position.top, page),
                            sort.keys.join(","),
                            if sort.desc { " desc" } else { "" }
                        )
                    } else {
                        format!(
                            "Esc clear | {search_status} | Enter/n next N previous | s sort | q quit"
                        )
                    },
                    width,
                );
                back.styles[status_row] = vec![Span {
                    range: 0..back.rows[status_row].len(),
                    role: "heading".into(),
                }];
            }
            let editing = sort_editing || search.editing;
            let mut cursor = None;
            if editing {
                let (input, column) = if sort_editing {
                    sort_input.view("Sort: ", width)
                } else {
                    search.input.view("Search: ", width)
                };
                let row = height.saturating_sub(1);
                if row > 0 {
                    back.rows[row - 1] = text::truncate(
                        &if sort_editing {
                            if sort_error.is_empty() {
                                "Enter apply | Esc cancel | Tab fields | Ctrl+u clear".into()
                            } else {
                                sort_error.clone()
                            }
                        } else {
                            format!(
                                "Enter apply | Esc cancel | Ctrl+u clear | {}",
                                search.status(&document, position.top, page)
                            )
                        },
                        width,
                    );
                    back.styles[row - 1] = vec![Span {
                        range: 0..back.rows[row - 1].len(),
                        role: if sort_error.is_empty() {
                            "heading"
                        } else {
                            "warning"
                        }
                        .into(),
                    }];
                }
                back.rows[row] = input;
                back.styles[row] = vec![Span {
                    range: 0..back.rows[row].len(),
                    role: "heading".into(),
                }];
                cursor = Some((column, row));
            }
            let update = back.diff(front.as_ref());
            if !update.is_empty() {
                let mut out = io::stdout().lock();
                out.write_all(update.as_bytes()).map_err(reading_error)?;
                out.flush().map_err(reading_error)?;
            }
            if let Some((column, row)) = cursor {
                execute!(io::stdout(), MoveTo(column as u16, row as u16), Show)
                    .map_err(reading_error)?;
            } else {
                execute!(io::stdout(), Hide).map_err(reading_error)?;
            }
            front = Some(back);
            match event::read().map_err(reading_error)? {
                Event::Resize(new_width, _) => {
                    let width = usize::from(new_width).max(1);
                    document = render(width, sorting.as_ref())?.screen(width);
                    search.rebuild(&document);
                }
                Event::Paste(value) => {
                    if sort_editing {
                        sort_input.insert(&value);
                    } else if search.editing {
                        search.input.insert(&value);
                        search.rebuild(&document);
                    }
                }
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        break;
                    }
                    if sort_editing {
                        match key.code {
                            KeyCode::Esc => {
                                sort_editing = false;
                                sort_error.clear();
                            }
                            KeyCode::Enter => {
                                if let Some(sort) = sorting.as_ref() {
                                    let mut next = sort.clone();
                                    next.keys = sort_input
                                        .text
                                        .split(',')
                                        .map(str::trim)
                                        .filter(|v| !v.is_empty())
                                        .map(str::to_string)
                                        .collect();
                                    if next.keys.is_empty() {
                                        sort_error = "Enter at least one sort field; R resets the initial order".into();
                                        continue;
                                    }
                                    match render(width, Some(&next)) {
                                        Ok(next_doc) => {
                                            let anchor = document.anchor(position.top);
                                            document = next_doc.screen(width);
                                            position.top = document.anchor_top(anchor.as_deref());
                                            sorting = Some(next);
                                            sort_editing = false;
                                            sort_error.clear();
                                            search.rebuild(&document);
                                        }
                                        Err(error) => sort_error = error.to_string(),
                                    }
                                }
                            }
                            KeyCode::Tab => {
                                if let Some(sort) = &sorting {
                                    let (prefix, index) = completion
                                        .get_or_insert_with(|| (sort_input.text.clone(), 0));
                                    let (head, token) = prefix
                                        .rsplit_once(',')
                                        .map(|(head, token)| (format!("{head},"), token.trim()))
                                        .unwrap_or((String::new(), prefix.as_str()));
                                    let hits = sort
                                        .candidates
                                        .iter()
                                        .filter(|field| field.starts_with(token))
                                        .collect::<Vec<_>>();
                                    if hits.is_empty() {
                                        sort_error =
                                            format!("Fields: {}", sort.candidates.join(","));
                                    } else {
                                        sort_input
                                            .set(format!("{head}{}", hits[*index % hits.len()]));
                                        *index += 1;
                                    }
                                }
                            }
                            _ => {
                                completion = None;
                                sort_error.clear();
                                sort_input.key(key);
                            }
                        }
                        continue;
                    }
                    if !search.editing
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && sorting.is_some()
                        && matches!(key.code, KeyCode::Char('s' | 'r' | 'R'))
                    {
                        if key.code == KeyCode::Char('s') {
                            sort_input.set(sorting.as_ref().unwrap().keys.join(","));
                            sort_editing = true;
                            completion = None;
                        } else {
                            let mut next = if key.code == KeyCode::Char('R') {
                                initial_sort.clone().unwrap()
                            } else {
                                sorting.clone().unwrap()
                            };
                            if key.code == KeyCode::Char('r') {
                                next.desc = !next.desc;
                            }
                            let anchor = document.anchor(position.top);
                            document = render(width, Some(&next))?.screen(width);
                            sorting = Some(next);
                            search.rebuild(&document);
                            position.top = document.anchor_top(anchor.as_deref());
                        }
                        continue;
                    }
                    if let Some(jump) = search.key(key, &document) {
                        if jump && let Some(row) = search.row(&document) {
                            position.top = row
                                .saturating_sub(document.header)
                                .min(body.len().saturating_sub(page));
                        }
                        continue;
                    }
                    if position.key(key.code, page, body.len(), body) {
                        break;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    };
    run()
}

/// Pending screen rows retain completion markers across width changes.
struct MoreRows {
    pending_styles: VecDeque<Vec<Span>>,
    color: bool,
    pending: VecDeque<(String, Option<usize>)>,
    completed: usize,
}

impl MoreRows {
    fn new(document: &Document, width: usize) -> Self {
        let mut rows = Self {
            pending_styles: (0..document.text.lines().count())
                .map(|row| document.visual_spans(row))
                .collect(),
            color: false,
            pending: document
                .text
                .lines()
                .enumerate()
                .map(|(index, line)| (line.into(), Some(index + 1)))
                .collect(),
            completed: 0,
        };
        rows.reflow(width);
        rows
    }

    fn reflow(&mut self, width: usize) {
        let mut old_styles = std::mem::take(&mut self.pending_styles);
        for (line, end) in std::mem::take(&mut self.pending) {
            let styles = old_styles.pop_front().unwrap_or_default();
            let (wrapped, _, layout) =
                Reading::new(Some(width)).mapped(&format!("{}\n", text::visible(&line, 0)));
            let lines = wrapped.lines().collect::<Vec<_>>();
            for (index, rendered) in lines.iter().enumerate() {
                let mapped = &layout[index];
                self.pending_styles
                    .push_back(crate::terminal::style::project(&line, &styles, mapped));
                self.pending.push_back((
                    (*rendered).into(),
                    if index + 1 == lines.len() { end } else { None },
                ));
            }
        }
    }

    fn take(&mut self, count: usize) -> String {
        let mut output = String::new();
        for _ in 0..count {
            let Some((line, end)) = self.pending.pop_front() else {
                break;
            };
            let styles = self.pending_styles.pop_front().unwrap_or_default();
            output.push_str(&crate::terminal::style::paint(&line, &styles, self.color));
            // Raw input mode requires explicit carriage returns on Unix too.
            output.push_str("\r\n");
            if let Some(end) = end {
                self.completed = end;
            }
        }
        output
    }

    fn progress(&self, document: &Document) -> String {
        let (total, noun) = document.count();
        let shown = if document.ends.is_empty() {
            self.completed
        } else {
            document
                .ends
                .iter()
                .filter(|end| **end <= self.completed)
                .count()
        };
        format!("{shown}/{total} {noun}")
    }
}

struct MoreInput;

impl Drop for MoreInput {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), MoveToColumn(0), Clear(ClearType::CurrentLine));
        let _ = terminal::disable_raw_mode();
    }
}

fn more(
    document: Document,
    mut width: usize,
    mut height: usize,
    always: bool,
    color: bool,
) -> Result<()> {
    terminal::enable_raw_mode().map_err(reading_error)?;
    let _input = MoreInput;
    let mut rows = MoreRows::new(&document, width);
    rows.color = color;
    let mut count = height.saturating_sub(2).max(1);
    let mut out = io::stdout().lock();
    loop {
        let content = rows.take(count);
        let prompt = if rows.pending.is_empty() && !always {
            String::new()
        } else {
            text::truncate(
                &format!(
                    "{} | q quit Enter line Space page",
                    rows.progress(&document)
                ),
                width,
            )
        };
        let update = format!(
            "{}{}{content}{prompt}",
            MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        );
        out.write_all(update.as_bytes()).map_err(reading_error)?;
        out.flush().map_err(reading_error)?;
        if rows.pending.is_empty() && !always {
            return Ok(());
        }
        loop {
            match event::read().map_err(reading_error)? {
                Event::Resize(columns, lines) => {
                    width = usize::from(columns).max(1);
                    height = usize::from(lines).max(1);
                    rows.reflow(width);
                    count = 0;
                    break;
                }
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        return Ok(());
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Enter | KeyCode::Down => {
                            if rows.pending.is_empty() {
                                return Ok(());
                            }
                            count = 1;
                            break;
                        }
                        KeyCode::Char(' ') | KeyCode::PageDown => {
                            if rows.pending.is_empty() {
                                return Ok(());
                            }
                            count = height.saturating_sub(2).max(1);
                            break;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

fn reading_error(error: io::Error) -> Error {
    Error::io(format!(
        "terminal output failed: {error}; use --overflow full"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_gutters_keep_their_style_across_readers_and_resizes() {
        let source = "  8 | Version: a long source value with Unicode \u{4e2d}\u{6587} and more text\n  9 | \n";
        let document = Document::reading(source.into(), Some(24));
        let assert_gutters = |output: &str| {
            let ansi = regex::Regex::new("\x1b\\[[0-9;]*m").unwrap();
            for line in output.lines() {
                let text = ansi.replace_all(line, "");
                assert!(
                    text.contains('|'),
                    "reflow lost the literal gutter: {line:?}"
                );
                if text.contains('|') {
                    let bar = line.find('|').unwrap();
                    assert!(line[..bar].contains("\x1b[90m"), "{line:?}");
                }
            }
            assert!(
                !output.contains("\x1b[1m"),
                "verbatim colon became a heading"
            );
        };
        assert_gutters(&document.colored(true));
        assert_gutters(&document.reading_width(18).colored(true));
        assert!(!document.colored(false).contains('\x1b'));
        let mut rows = MoreRows::new(&document, 20);
        rows.color = true;
        assert_gutters(&rows.take(1));
        rows.reflow(16);
        assert_gutters(&rows.take(100));
    }

    #[test]
    fn more_advances_exactly_requested_rows_and_reflows_only_pending_content() {
        let document = Document::blocks(vec![
            "first\n\nlong pending value\n".into(),
            "last\n".into(),
        ]);
        let mut rows = MoreRows::new(&document, 80);
        assert_eq!(rows.take(1), "first\r\n");
        assert_eq!(rows.progress(&document), "0/2 items");
        assert_eq!(rows.take(1), "\r\n");
        rows.reflow(10);
        let rest = rows.take(100);
        assert!(!rest.contains("first"));
        assert!(rest.lines().all(|line| text::display_width(line) <= 10));
        assert_eq!(
            rest.split_whitespace().collect::<Vec<_>>().join(" "),
            "long pending value last"
        );
        assert_eq!(rows.progress(&document), "2/2 items");
        assert!(rows.pending.is_empty());
        assert_eq!(document.screen(80).text, document.text);
    }

    #[test]
    fn more_requires_interactive_input_and_tracks_partial_source_lines() {
        assert_eq!(mode(Some("more"), false, true, true), Mode::More);
        assert_eq!(mode(Some("more"), false, true, false), Mode::Full);
        let document: Document = "abcdefghijklmnopqrst\nend\n".to_string().into();
        let mut rows = MoreRows::new(&document, 10);
        assert_eq!(rows.take(1), "abcdefghij\r\n");
        assert_eq!(rows.progress(&document), "0/2 lines");
        rows.reflow(5);
        let mut tail = String::new();
        while rows.completed == 0 {
            tail.push_str(rows.take(1).trim());
        }
        assert_eq!(tail, "klmnopqrst");
        assert_eq!(rows.progress(&document), "1/2 lines");
        assert_eq!(rows.take(1), "end\r\n");
        assert_eq!(rows.progress(&document), "2/2 lines");
    }

    fn frame(width: usize, rows: &[&str]) -> Frame {
        Frame {
            width,
            rows: rows.iter().map(|line| (*line).into()).collect(),
            styles: Vec::new(),
            color: false,
        }
    }

    #[test]
    fn frame_diffs_keep_unchanged_rows_and_skip_identical_frames() {
        let front = frame(20, &["id title", "1 first", "q quit"]);
        assert!(front.diff(Some(&front)).is_empty());
        let back = frame(20, &["id title", "2 second", "q quit"]);
        let encoded = back.diff(Some(&front));
        assert!(!encoded.contains("id title") && !encoded.contains("q quit"));
        assert!(encoded.contains("\x1b[2;1H2 second\x1b[K"));
        assert!(encoded.starts_with("\x1b[?2026h"));
        assert!(encoded.ends_with("\x1b[?2026l"));
        assert!(!encoded.contains("\x1b[2J"));
    }

    #[test]
    fn frame_style_changes_repaint_without_changing_visible_geometry() {
        let mut front = frame(20, &["中文 Clerk"]);
        front.color = true;
        front.styles = vec![vec![Span {
            range: 0..6,
            role: "identifier".into(),
        }]];
        let mut back = frame(20, &["中文 Clerk"]);
        back.color = true;
        back.styles = vec![vec![Span {
            range: 7..12,
            role: "current".into(),
        }]];
        let output = back.diff(Some(&front));
        assert!(output.contains("\x1b[1;7mClerk"));
        assert_eq!(text::display_width(&back.rows[0]), 10);
        assert!(!output.contains("\x1b[2J"));
    }

    #[test]
    fn frame_diffs_clear_shortened_and_empty_rows_without_erasing_right_margin() {
        let front = frame(10, &["long value", "old", "old"]);
        let back = frame(10, &["中文", "", "0123456789"]);
        let encoded = back.diff(Some(&front));
        assert!(encoded.contains("\x1b[1;1H中文\x1b[K"));
        assert!(encoded.contains("\x1b[2;1H\x1b[K"));
        assert!(encoded.contains("\x1b[3;1H0123456789"));
        assert!(!encoded.contains("0123456789\x1b[K"));
    }

    #[test]
    fn frame_resize_invalidates_the_front_without_clearing_the_screen() {
        let front = frame(20, &["header", "body", "status"]);
        for back in [
            frame(10, &["header", "body", "status"]),
            frame(20, &["header", "body", "", "status"]),
        ] {
            let encoded = back.diff(Some(&front));
            assert!(encoded.contains("\x1b[1;1Hheader"));
            assert!(encoded.contains("body") && encoded.contains("status"));
            assert!(!encoded.contains("\x1b[2J"));
            assert_eq!(encoded, back.diff(None));
        }
    }

    #[test]
    fn redirected_and_machine_streams_never_page_or_truncate() {
        for value in [
            None,
            Some("truncate"),
            Some("more"),
            Some("pager"),
            Some("full"),
        ] {
            assert_eq!(mode(value, true, true, true), Mode::Full);
            assert_eq!(mode(value, false, false, true), Mode::Full);
        }
        assert_eq!(mode(None, false, true, true), Mode::Full);
        assert_eq!(mode(Some("pager"), false, true, false), Mode::Full);
        assert_eq!(mode(Some("pager"), false, true, true), Mode::Pager);
    }

    fn table() -> Document {
        Document::table(
            format!(
                "id title\n-- -----\n{}",
                (1..=100)
                    .map(|n| format!("{n} 中文任务\n"))
                    .collect::<String>()
            ),
            2,
            100,
        )
    }

    #[test]
    fn truncated_tables_preserve_headers_counts_and_screen_budget() {
        for width in [20, 40, 80, 140] {
            for height in [10, 24, 40] {
                let output = truncate(&table(), width, height);
                assert!(output.starts_with("id title\n-- -----\n"), "{output}");
                assert!(output.contains("/100 items"), "{output}");
                assert!(output.contains("--overflow full"), "{output}");
                assert!(output.contains("--overflow pager"), "{output}");
                assert!(
                    output
                        .lines()
                        .map(|line| text::display_width(line).max(1).div_ceil(width))
                        .sum::<usize>()
                        <= height - 2,
                    "{output}"
                );
            }
        }
        let short: Document = "a\nb\n".to_string().into();
        assert_eq!(truncate(&short, 80, 24), short.text);
        for height in 1..10 {
            assert!(truncate(&table(), 20, height).lines().count() <= height.saturating_sub(2));
        }
    }

    #[test]
    fn detail_truncation_prefers_whole_blocks_and_marks_oversized_first_block() {
        let blocks = Document::blocks(vec!["a\nb\nc\n\n".into(); 10]);
        let output = truncate(&blocks, 80, 10);
        assert!(output.contains("1/10 items;"), "{output}");
        assert!(!output.contains("partial"), "{output}");
        let huge = Document::blocks(vec!["field: value\n".repeat(100)]);
        let output = truncate(&huge, 80, 12);
        assert!(output.contains("0/1 items (partial)"), "{output}");
    }

    #[test]
    fn screen_rows_keep_alignment_source_gutters_and_item_boundaries() {
        let document = Document::blocks(vec![
            "key:   中文内容内容内容\n  12| 长长长长长长长长\n".into(),
            "end\n".into(),
        ]);
        let narrow = document.screen(20);
        assert!(narrow.text.contains("key:   中文"));
        assert!(narrow.text.contains("    | "));
        assert!(narrow.ends[0] > document.ends[0]);
        assert!(
            narrow
                .text
                .lines()
                .all(|line| text::display_width(line) <= 20)
        );
        assert_eq!(table().range(0, 10), "1-10/100 items");
        assert_eq!(table().range(90, 10), "91-100/100 items");
        assert_eq!(table().screen(80).header, 2);
        assert_eq!(table().screen(80).text.lines().next(), Some("id title"));
    }

    #[test]
    fn pager_navigation_search_wrap_and_quit_are_bounded() {
        let lines = (0..100).map(|n| format!("task {n}")).collect::<Vec<_>>();
        let mut position = Position::default();
        position.key(KeyCode::PageDown, 10, 100, &lines);
        assert_eq!(position.top, 10);
        position.key(KeyCode::End, 10, 100, &lines);
        assert_eq!(position.top, 90);
        position.key(KeyCode::Down, 10, 100, &lines);
        assert_eq!(position.top, 90);
        position.key(KeyCode::Home, 10, 100, &lines);
        assert!(position.key(KeyCode::Char('q'), 10, 100, &lines));
        position.key(KeyCode::PageUp, 200, 100, &lines);
        assert_eq!(position.top, 0);
    }
}
