use leptos::{ev, prelude::*};
use web_sys::HtmlTextAreaElement;

const INDENT: &str = "    ";

/// Plain textarea editor bound to `code`.
#[component]
pub fn Editor(code: RwSignal<String>) -> impl IntoView {
    // Tab inserts spaces instead of moving focus out of the editor.
    let on_keydown = move |e: ev::KeyboardEvent| {
        if e.key() != "Tab" {
            return;
        }
        e.prevent_default();
        let area = event_target::<HtmlTextAreaElement>(&e);
        let start = area.selection_start().ok().flatten().unwrap_or(0);
        let end = area.selection_end().ok().flatten().unwrap_or(start);
        let _ = area.set_range_text_with_start_and_end(INDENT, start, end);
        let cursor = start + INDENT.len() as u32;
        let _ = area.set_selection_range(cursor, cursor);
        code.set(area.value());
    };

    view! {
        <textarea
            class="editor"
            spellcheck="false"
            prop:value=move || code.get()
            on:input=move |e| code.set(event_target_value(&e))
            on:keydown=on_keydown
        ></textarea>
    }
}
