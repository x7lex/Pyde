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

#[cfg(test)]
mod tests {
    use super::*;

    const M: Metrics = Metrics {
        char_width: 10.0,
        line_height: 20.0,
        pad_x: 0.0,
        pad_y: 0.0,
        tab_size: 4,
    };

    fn hit(text: &str, col: f64, row: f64) -> Option<Hit> {
        locate(text, col * 10.0 + 1.0, row * 20.0 + 1.0, &M)
    }

    #[test]
    fn locates_ascii() {
        let h = hit("ab\ncd", 1.0, 1.0).unwrap();
        assert_eq!((h.offset, h.line), (4, 1));
        assert!(
            hit("ab\ncd", 2.0, 0.0).is_none(),
            "past the end of the line"
        );
        assert!(
            hit("ab\ncd", 0.0, 2.0).is_none(),
            "past the end of the file"
        );
    }

    #[test]
    fn locates_after_multibyte_characters_in_bytes() {
        let text = "s = \"héllo\"; t";
        let col = text.chars().position(|c| c == 't').unwrap();
        let h = hit(text, col as f64, 0.0).unwrap();
        assert_eq!(&text[h.offset..], "t");
    }

    #[test]
    fn tabs_span_to_the_next_tab_stop() {
        let text = "\tx";
        assert_eq!(hit(text, 3.0, 0.0).unwrap().offset, 0);
        assert!(hit(text, 3.0, 0.0).unwrap().is_whitespace);
        assert_eq!(hit(text, 4.0, 0.0).unwrap().offset, 1);
        let text = "a\tx";
        assert_eq!(hit(text, 3.0, 0.0).unwrap().offset, 1);
        assert_eq!(hit(text, 4.0, 0.0).unwrap().offset, 2);
    }

    #[test]
    fn wide_characters_take_two_cells() {
        let text = "日本x";
        assert_eq!(hit(text, 1.0, 0.0).unwrap().offset, 0);
        assert_eq!(hit(text, 2.0, 0.0).unwrap().offset, "日".len());
        assert_eq!(&text[hit(text, 4.0, 0.0).unwrap().offset..], "x");
    }

    #[test]
    fn crlf_does_not_count_the_carriage_return() {
        let text = "ab\r\ncd";
        assert!(hit(text, 2.0, 0.0).is_none());
        assert_eq!(hit(text, 0.0, 1.0).unwrap().offset, 4);
    }

    #[test]
    fn padding_is_not_text() {
        let m = Metrics {
            pad_x: 12.0,
            pad_y: 12.0,
            ..M
        };
        assert!(locate("ab", 5.0, 20.0, &m).is_none());
        assert_eq!(locate("ab", 13.0, 13.0, &m).unwrap().offset, 0);
    }

    #[test]
    fn position_of_is_the_inverse_of_locate() {
        let text = "x = 1\n\tfoo(日本, bar)\n";
        for needle in ["x", "foo", "日", "bar"] {
            let offset = text.find(needle).unwrap();
            let (x, y) = position_of(text, offset, &M);
            let h = locate(text, x + 1.0, y + 1.0, &M).unwrap();
            assert_eq!(h.offset, offset, "{needle}");
        }
    }

    #[test]
    fn word_range_covers_identifiers_and_single_symbols() {
        let text = "foo_1.bar(é, x)";
        assert_eq!(word_range(text, 0), (0, 5));
        assert_eq!(word_range(text, 4), (0, 5));
        assert_eq!(word_range(text, 5), (5, 6));
        assert_eq!(word_range(text, 6), (6, 9));
        let e = text.find('é').unwrap();
        assert_eq!(word_range(text, e), (e, e + 2));
        assert_eq!(word_range(text, text.len()), (text.len(), text.len()));
        assert_eq!(word_range("x", 0), (0, 1));
    }

    #[test]
    fn utf16_and_byte_offsets_convert_both_ways() {
        let text = "a😀bñ";
        for (byte, units) in [(0, 0), (1, 1), (5, 3), (6, 4), (8, 5)] {
            assert_eq!(byte_to_utf16(text, byte), units);
            assert_eq!(utf16_to_byte(text, units as usize), byte);
        }
        assert_eq!(utf16_to_byte(text, 99), text.len());
        assert_eq!(byte_to_utf16(text, 2), 1, "inside a character rounds down");
    }

    #[test]
    fn utf16_offsets_count_code_units() {
        let text = "a😀b\nñx";
        assert_eq!(utf16_offset(text, 0, 2), 3);
        assert_eq!(utf16_offset(text, 1, 1), 5 + 1);
        assert_eq!(
            utf16_offset(text, 5, 0),
            text.encode_utf16().count() as u32 + 1
        );
    }
}
