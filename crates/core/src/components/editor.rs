use leptos::{ev, prelude::*};
use web_sys::HtmlTextAreaElement;

use crate::highlight;

const INDENT: &str = "    ";

/// Textarea editor bound to `code`. The textarea's own text is transparent, and a
/// coloured copy sits behind it and follows its scrolling.
#[component]
pub fn Editor(code: RwSignal<String>) -> impl IntoView {
    let scroll = RwSignal::new((0, 0));

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

    let on_scroll = move |e: ev::Event| {
        let area = event_target::<HtmlTextAreaElement>(&e);
        scroll.set((area.scroll_left(), area.scroll_top()));
    };

    view! {
        <div class="editor">
            <pre class="editor-colours" aria-hidden="true">
                <code
                    style:transform=move || {
                        let (left, top) = scroll.get();
                        format!("translate({}px, {}px)", -left, -top)
                    }
                    inner_html=move || highlight::to_html(&code.get())
                ></code>
            </pre>
            <textarea
                class="editor-input"
                spellcheck="false"
                wrap="off"
                prop:value=move || code.get()
                on:input=move |e| code.set(event_target_value(&e))
                on:keydown=on_keydown
                on:scroll=on_scroll
            ></textarea>
        </div>
    }
}
