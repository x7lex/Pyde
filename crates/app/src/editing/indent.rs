//! The editor's smart keys as pure text edits: Enter keeps and grows the
//! indentation, Backspace removes a whole level, and Tab and Shift+Tab move to
//! the next stop or shift whole lines. Indentation is always four spaces.
//!
//! Everything here works on byte offsets. `layout` converts to the UTF-16 offsets
//! the DOM uses.

use pyde_analysis::highlight::{INDENT, Kind, tokenize};

/// Replaces `start..end` of the text with `insert`, then selects `select`, in
/// byte offsets into the edited text.
#[derive(Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub insert: String,
    pub select: (usize, usize),
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i)
}

fn leading_whitespace(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

fn dedent_once(indent: &str) -> &str {
    if let Some(shorter) = indent.strip_suffix('\t') {
        return shorter;
    }
    let trailing = indent.len() - indent.trim_end_matches(' ').len();
    &indent[..indent.len() - trailing.min(INDENT.len())]
}

/// The column the caret reaches after `text`, with tabs advancing to the next stop.
fn visual_column(text: &str) -> usize {
    text.chars().fold(0, |col, c| {
        if c == '\t' {
            col + INDENT.len() - col % INDENT.len()
        } else {
            col + 1
        }
    })
}

/// Whether the next line should be indented further: the line ends in a `:` that
/// isn't inside brackets, or in an opening bracket. Comments don't count.
fn opens_block(line: &str) -> bool {
    let tokens = tokenize(line);
    let significant = tokens
        .iter()
        .filter(|t| !t.text.trim().is_empty() && t.kind != Kind::Comment);
    let mut depth = 0i32;
    let mut last = None;
    for token in significant {
        if token.kind == Kind::Plain {
            match token.text {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        last = Some(token);
    }
    match last {
        Some(t) if t.kind == Kind::Plain => match t.text {
            ":" => depth <= 0,
            "(" | "[" | "{" => true,
            _ => false,
        },
        _ => false,
    }
}

/// A statement after which the next line belongs one level further out.
fn ends_block(line: &str) -> bool {
    let word = line
        .trim_start()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or("");
    matches!(word, "return" | "pass" | "break" | "continue" | "raise")
}

/// The edit for Enter, replacing the selection `start..end`.
pub fn enter(text: &str, start: usize, end: usize) -> Edit {
    let line_begin = line_start(text, start);
    let before = &text[line_begin..start];
    let indent = leading_whitespace(before);

    let between_brackets = matches!(
        (before.chars().last(), text[end..].chars().next()),
        (Some('('), Some(')')) | (Some('['), Some(']')) | (Some('{'), Some('}'))
    );
    // A line of only whitespace doesn't keep its trailing spaces.
    let blank = !before.is_empty() && before.trim().is_empty();
    let edit_start = if blank { line_begin } else { start };

    if between_brackets {
        let inner = format!("{indent}{INDENT}");
        let caret = edit_start + 1 + inner.len();
        return Edit {
            start: edit_start,
            end,
            insert: format!("\n{inner}\n{indent}"),
            select: (caret, caret),
        };
    }

    let next_indent = if opens_block(before) {
        format!("{indent}{INDENT}")
    } else if ends_block(before) {
        dedent_once(indent).to_string()
    } else {
        indent.to_string()
    };
    let insert = format!("\n{next_indent}");
    let caret = edit_start + insert.len();
    Edit {
        start: edit_start,
        end,
        insert,
        select: (caret, caret),
    }
}

/// The edit for Backspace with the caret at `start..end`, or `None` when the
/// browser's own Backspace is right: there is a selection, or the caret isn't in
/// indentation.
pub fn backspace(text: &str, start: usize, end: usize) -> Option<Edit> {
    if start != end {
        return None;
    }
    let before = &text[line_start(text, start)..start];
    if before.is_empty() || !before.chars().all(|c| c == ' ') {
        return None;
    }
    let remove = match before.len() % INDENT.len() {
        0 => INDENT.len(),
        n => n,
    };
    if remove < 2 {
        return None;
    }
    let at = start - remove;
    Some(Edit {
        start: at,
        end: start,
        insert: String::new(),
        select: (at, at),
    })
}

/// The edit for Tab, or Shift+Tab when `shift`, over the selection `start..end`.
pub fn tab(text: &str, start: usize, end: usize, shift: bool) -> Edit {
    // A selection is never replaced by spaces, which would delete it. Any selection
    // indents the lines it touches instead.
    if start == end && !shift {
        let col = visual_column(&text[line_start(text, start)..start]);
        let spaces = INDENT.len() - col % INDENT.len();
        let caret = start + spaces;
        return Edit {
            start,
            end,
            insert: " ".repeat(spaces),
            select: (caret, caret),
        };
    }
    shift_lines(text, start, end, shift)
}

fn shift_lines(text: &str, start: usize, end: usize, dedent: bool) -> Edit {
    let first = line_start(text, start);
    // A selection that ends at the very start of a line doesn't include it.
    let reaches = if end > start && end == line_start(text, end) {
        end - 1
    } else {
        end
    };
    let last = line_end(text, reaches.max(first));

    struct Line {
        old_start: usize,
        new_start: usize,
        change: isize,
    }
    let mut lines = Vec::new();
    let mut shifted = String::new();
    let mut old_start = first;
    for (i, line) in text[first..last].split('\n').enumerate() {
        if i > 0 {
            shifted.push('\n');
        }
        let new_start = first + shifted.len();
        let change = if dedent {
            let removed = if line.starts_with('\t') {
                1
            } else {
                line.chars()
                    .take(INDENT.len())
                    .take_while(|c| *c == ' ')
                    .count()
            };
            shifted.push_str(&line[removed..]);
            -(removed as isize)
        } else if line.trim().is_empty() {
            shifted.push_str(line);
            0
        } else {
            shifted.push_str(INDENT);
            shifted.push_str(line);
            INDENT.len() as isize
        };
        lines.push(Line {
            old_start,
            new_start,
            change,
        });
        old_start += line.len() + 1;
    }

    let new_end = first + shifted.len();
    let map = |pos: usize, is_selection_start: bool| {
        if pos > last {
            return new_end + (pos - last);
        }
        let line = lines
            .iter()
            .rev()
            .find(|l| l.old_start <= pos)
            .unwrap_or(&lines[0]);
        let col = pos - line.old_start;
        let new_col = match line.change {
            c if c >= 0 && col == 0 && is_selection_start => 0,
            c if c >= 0 => col + c as usize,
            c => col.saturating_sub((-c) as usize),
        };
        line.new_start + new_col
    };
    Edit {
        start: first,
        end: last,
        insert: shifted,
        select: (map(start, true), map(end, false)),
    }
}
