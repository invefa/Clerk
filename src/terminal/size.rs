//! Read the visible stdout terminal, never a different input or error stream.

use std::io::IsTerminal;

pub fn reading(plain: bool, columns: Option<usize>) -> crate::terminal::text::Reading {
    let width = if plain {
        None
    } else if columns.is_some() {
        columns
    } else if std::io::stdout().is_terminal() {
        terminal_size::terminal_size_of(std::io::stdout())
            .map(|(terminal_size::Width(width), _)| usize::from(width))
            .filter(|width| *width > 0)
            .or_else(|| {
                std::env::var("COLUMNS")
                    .ok()?
                    .parse::<usize>()
                    .ok()
                    .filter(|width| (20..=10000).contains(width))
            })
            .or(Some(80))
    } else {
        None
    };
    crate::terminal::text::Reading::new(width)
}
