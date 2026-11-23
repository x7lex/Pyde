//! Applying edits to the editor's textarea.

use leptos::prelude::document;
use wasm_bindgen::JsCast;
use web_sys::{HtmlDocument, HtmlTextAreaElement};

use super::indent::Edit;
use super::layout::{byte_to_utf16, measure, utf16_offset};

/// Applies `edit`, then returns the textarea's new value.
///
/// It goes in as typed text (`insertText`) so that Ctrl+Z undoes it. Setting the
/// value or `set_range_text` would wipe the textarea's undo history. If the
/// browser refuses, it falls back to `set_range_text`.
pub fn apply(area: &HtmlTextAreaElement, edit: &Edit) -> String {
    let before = area.value();
    let (start, end) = (
        byte_to_utf16(&before, edit.start),
        byte_to_utf16(&before, edit.end),
    );
    let _ = area.set_selection_range(start, end);

    let typed = document()
        .unchecked_into::<HtmlDocument>()
        .exec_command_with_show_ui_and_value("insertText", false, &edit.insert)
        .unwrap_or(false);
    if !typed {
        let _ = area.set_range_text_with_start_and_end(&edit.insert, start, end);
    }

    let after = area.value();
    let (from, to) = edit.select;
    let _ = area.set_selection_range(byte_to_utf16(&after, from), byte_to_utf16(&after, to));
    after
}

/// Puts the caret at a zero-based `line` and `col` of `text`, and scrolls to it,
/// centring it if it was out of view.
pub fn reveal(area: &HtmlTextAreaElement, text: &str, line: usize, col: usize) {
    let offset = utf16_offset(text, line, col);
    let _ = area.focus();
    let _ = area.set_selection_range(offset, offset);
    let Some(metrics) = measure(area) else { return };
    let top = line as f64 * metrics.line_height;
    let visible = area.client_height() as f64;
    let scrolled = area.scroll_top() as f64;
    if top < scrolled || top + metrics.line_height > scrolled + visible {
        area.set_scroll_top((top - visible / 2.0).max(0.0) as i32);
    }
}
