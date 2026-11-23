//! Mapping between mouse positions, text offsets and pixels in the editor's
//! monospace textarea, and between Rust's UTF-8 offsets and the DOM's UTF-16
//! offsets.

use leptos::prelude::document;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, HtmlTextAreaElement};

/// Font metrics of the textarea, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub char_width: f64,
    pub line_height: f64,
    pub pad_x: f64,
    pub pad_y: f64,
    pub tab_size: usize,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            char_width: 8.4,
            line_height: 21.0,
            pad_x: 12.0,
            pad_y: 12.0,
            tab_size: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Byte offset into the source.
    pub offset: usize,
    pub line: usize,
    pub is_whitespace: bool,
}

/// How many monospace cells `c` takes, given it starts at visual column `col`.
fn cells(c: char, col: usize, tab_size: usize) -> usize {
    match c {
        '\t' => tab_size - col % tab_size,
        '\u{0300}'..='\u{036F}' | '\u{200B}'..='\u{200D}' | '\u{FE00}'..='\u{FE0F}' => 0,
        '\u{1100}'..='\u{115F}'
        | '\u{2E80}'..='\u{A4CF}'
        | '\u{AC00}'..='\u{D7A3}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FE30}'..='\u{FE6F}'
        | '\u{FF00}'..='\u{FF60}'
        | '\u{FFE0}'..='\u{FFE6}'
        | '\u{1F300}'..='\u{1F64F}'
        | '\u{1F900}'..='\u{1F9FF}'
        | '\u{20000}'..='\u{3FFFD}' => 2,
        _ => 1,
    }
}

/// The text of each line with its starting byte offset. A trailing `\r` is not
/// part of the line.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut start = 0;
    text.split('\n').map(move |line| {
        let at = start;
        start += line.len() + 1;
        (at, line.strip_suffix('\r').unwrap_or(line))
    })
}

/// Finds the character at content coordinates `(x, y)`, measured from the
/// textarea's top-left corner including scroll, padding and all.
pub fn locate(text: &str, x: f64, y: f64, m: &Metrics) -> Option<Hit> {
    let (x, y) = (x - m.pad_x, y - m.pad_y);
    if x < 0.0 || y < 0.0 {
        return None;
    }
    let row = (y / m.line_height) as usize;
    let target = (x / m.char_width) as usize;
    let (start, line) = lines(text).nth(row)?;

    let mut col = 0;
    for (byte, c) in line.char_indices() {
        let width = cells(c, col, m.tab_size);
        if target < col + width.max(1) {
            return Some(Hit {
                offset: start + byte,
                line: row,
                is_whitespace: c.is_whitespace(),
            });
        }
        col += width;
    }
    None
}

/// Content coordinates of the top-left corner of the character at `offset`.
pub fn position_of(text: &str, offset: usize, m: &Metrics) -> (f64, f64) {
    let offset = offset.min(text.len());
    let (row, (start, line)) = lines(text)
        .enumerate()
        .take_while(|(_, (start, _))| *start <= offset)
        .last()
        .unwrap_or((0, (0, "")));

    let mut col = 0;
    for (byte, c) in line.char_indices() {
        if start + byte >= offset {
            break;
        }
        col += cells(c, col, m.tab_size);
    }
    (
        m.pad_x + col as f64 * m.char_width,
        m.pad_y + row as f64 * m.line_height,
    )
}

/// Byte range of the identifier-like word around `offset`, or of just the one
/// character there if it isn't part of a word. Cheap enough for every mouse
/// move, unlike asking the checker.
pub fn word_range(text: &str, offset: usize) -> (usize, usize) {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let Some(c) = text.get(offset..).and_then(|rest| rest.chars().next()) else {
        return (offset, offset);
    };
    if !is_word(c) {
        return (offset, offset + c.len_utf8());
    }
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(offset, |(i, _)| i);
    let end = text[offset..]
        .char_indices()
        .find(|(_, c)| !is_word(*c))
        .map_or(text.len(), |(i, _)| offset + i);
    (start, end)
}

/// Byte offset in `text` of the UTF-16 offset `units`, as the DOM reports a caret.
pub fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (byte, c) in text.char_indices() {
        if seen >= units {
            return byte;
        }
        seen += c.len_utf16();
    }
    text.len()
}

/// UTF-16 offset of the byte offset `byte` in `text`.
pub fn byte_to_utf16(text: &str, byte: usize) -> u32 {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].encode_utf16().count() as u32
}

/// Offset in UTF-16 code units of the character at zero-indexed `line` and
/// character column `col`, as `set_selection_range` wants it.
pub fn utf16_offset(text: &str, line: usize, col: usize) -> u32 {
    let mut offset = 0;
    for (i, l) in text.split('\n').enumerate() {
        if i == line {
            offset += l.chars().take(col).map(char::len_utf16).sum::<usize>();
            break;
        }
        offset += l.encode_utf16().count() + 1;
    }
    offset as u32
}

/// Reads the textarea's real font metrics by laying out a probe in its font.
pub fn measure(area: &HtmlTextAreaElement) -> Option<Metrics> {
    const PROBE_CHARS: usize = 64;

    let style = leptos::prelude::window().get_computed_style(area).ok()??;
    let px = |name: &str| {
        style
            .get_property_value(name)
            .ok()
            .and_then(|v| v.trim().trim_end_matches("px").parse::<f64>().ok())
    };

    let probe: HtmlElement = document().create_element("span").ok()?.unchecked_into();
    let probe_style = probe.style();
    for name in [
        "font-family",
        "font-size",
        "font-weight",
        "font-style",
        "line-height",
        "letter-spacing",
        "tab-size",
    ] {
        let value = style.get_property_value(name).ok()?;
        let _ = probe_style.set_property(name, &value);
    }
    let _ = probe_style.set_property("position", "absolute");
    let _ = probe_style.set_property("visibility", "hidden");
    let _ = probe_style.set_property("white-space", "pre");
    probe.set_text_content(Some(&"M".repeat(PROBE_CHARS)));
    area.parent_element()?.append_child(&probe).ok()?;
    let rect = probe.get_bounding_client_rect();
    probe.remove();

    let defaults = Metrics::default();
    Some(Metrics {
        char_width: if rect.width() > 0.0 {
            rect.width() / PROBE_CHARS as f64
        } else {
            defaults.char_width
        },
        line_height: if rect.height() > 0.0 {
            rect.height()
        } else {
            defaults.line_height
        },
        pad_x: px("padding-left").unwrap_or(defaults.pad_x),
        pad_y: px("padding-top").unwrap_or(defaults.pad_y),
        tab_size: style
            .get_property_value("tab-size")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .filter(|n| *n > 0)
            .unwrap_or(defaults.tab_size),
    })
}
