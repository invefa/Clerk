//! Shared grapheme-aware single-line input for reader controls.

use crate::terminal::text::display_width;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Default)]
pub struct Editor {
    pub text: String,
    cursor: usize,
}

impl Editor {
    pub fn set(&mut self, value: String) {
        self.text = value.chars().filter(|c| !c.is_control()).collect();
        self.cursor = self.text.len();
    }
    fn left(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
    fn right(&self) -> usize {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map(|g| self.cursor + g.len())
            .unwrap_or(self.cursor)
    }
    pub fn insert(&mut self, value: &str) {
        let clean = value
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>();
        self.text.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
        // Inserting a combining mark may merge with the next grapheme.
        self.cursor = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(self.text.len()))
            .find(|i| *i >= self.cursor)
            .unwrap_or(self.text.len());
    }
    pub fn key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => self.cursor = self.left(),
            KeyCode::Right => self.cursor = self.right(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            KeyCode::Backspace => {
                let start = self.left();
                self.text.replace_range(start..self.cursor, "");
                self.cursor = start;
            }
            KeyCode::Delete => {
                let end = self.right();
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => self.cursor = 0,
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = self.text.len()
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text.clear();
                self.cursor = 0;
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(&c.to_string())
            }
            _ => {}
        }
    }
    /// Keep the insertion cursor visible, reserving one cell even at the right edge.
    pub fn view(&self, label: &str, width: usize) -> (String, usize) {
        let label = crate::terminal::text::truncate(label, width.saturating_sub(2));
        let budget = width
            .saturating_sub(display_width(&label))
            .saturating_sub(1);
        let mut start = 0;
        while start < self.cursor
            && display_width(&self.text[start..self.cursor]) + usize::from(start > 0) > budget
        {
            start += self.text[start..]
                .graphemes(true)
                .next()
                .map(str::len)
                .unwrap_or(1);
        }
        let marker = if start > 0 { "<" } else { "" };
        let cursor =
            display_width(&label) + marker.len() + display_width(&self.text[start..self.cursor]);
        let mut visible = format!("{label}{marker}");
        for g in self.text[start..].graphemes(true) {
            if display_width(&visible) + display_width(g) >= width {
                break;
            }
            visible.push_str(g);
        }
        (visible, cursor.min(width.saturating_sub(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_uses_graphemes_and_a_scrolling_cursor() {
        let mut input = Editor::default();
        input.set("a中文e\u{301}".into());
        input.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        input.key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
        assert_eq!(input.text, "a中文");
        input.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        input.insert("X\n");
        assert_eq!(input.text, "a中X文");
        input.set("中文".repeat(40));
        let (shown, cursor) = input.view("Search: ", 24);
        assert!(shown.contains('<') && cursor < 24 && display_width(&shown) < 24);
        input.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        assert_eq!(input.view("Search: ", 24).1, 8);
    }
}
