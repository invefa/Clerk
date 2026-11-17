//! Comma separated values.
//!
//! Tables are read and written as CSV so that every change stays reviewable in
//! a diff. The parser handles quoted cells, embedded separators, embedded line
//! breaks and both line ending styles.

use crate::foundation::error::{Error, Result};

/// Split a document into rows of cells.
pub fn parse(body: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = body.chars().peekable();

    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(character);
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            ',' => {
                row.push(std::mem::take(&mut cell));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            other => cell.push(other),
        }
    }
    if quoted {
        return Err(Error::validation("csv: unterminated quoted cell"));
    }
    row.push(cell);
    rows.push(row);

    // A trailing line break yields one empty cell in a new row; drop it.
    if let Some(last) = rows.last()
        && last.len() == 1
        && last[0].is_empty()
    {
        rows.pop();
    }
    Ok(rows)
}

/// Render rows back to a CSV document with a trailing line break.
pub fn serialize(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let mut first = true;
        for cell in row {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&escape(cell));
        }
        out.push('\n');
    }
    out
}

fn escape(cell: &str) -> String {
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_rows() {
        let rows = parse("a,b\n1,2\n").expect("parsed");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["1", "2"]]);
    }

    #[test]
    fn keeps_quoted_separator_and_break() {
        let rows = parse("a,\"x,y\"\n\"line\nbreak\",2\n").expect("parsed");
        assert_eq!(rows[0][1], "x,y");
        assert_eq!(rows[1][0], "line\nbreak");
    }

    #[test]
    fn round_trips_quotes() {
        let rows = vec![vec!["a\"b".to_string(), "c,d".to_string()]];
        let text = serialize(&rows);
        assert_eq!(parse(&text).expect("parsed"), rows);
    }

    #[test]
    fn accepts_windows_line_endings() {
        let rows = parse("a,b\r\n1,2\r\n").expect("parsed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1][1], "2");
    }
}
