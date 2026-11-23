//! The file name in the header, which becomes a field to rename it.

use leptos::{ev, prelude::*};

use crate::state::{Document, Status};

/// The file's name in the header, on one line and cut with `…` when it doesn't
/// fit. Clicking it turns it into a field as wide as the name: Enter or leaving
/// the field renames, Escape cancels, and the arrow keys move through the text.
/// A name that won't do is explained in the header's status text.
#[component]
pub fn FileName(document: Document, status: Status) -> impl IntoView {
    let editing = RwSignal::new(false);
    // What is typed so far, which the field's width follows.
    let draft = RwSignal::new(String::new());
    let field = NodeRef::<leptos::html::Input>::new();
    let button = NodeRef::<leptos::html::Button>::new();
    // Keyboard users are returned to the name once they have finished.
    let refocus = StoredValue::new(false);

    // Focus the field as it appears, with the name but not `.py` selected.
    Effect::new(move || {
        let Some(field) = field.get() else { return };
        let name = document.name.get_untracked();
        field.set_value(&name);
        draft.set(name.clone());
        let _ = field.focus();
        let stem = name.strip_suffix(".py").unwrap_or(&name);
        let _ = field.set_selection_range(0, stem.encode_utf16().count() as u32);
    });
    Effect::new(move || {
        if let Some(button) = button.get()
            && refocus.get_value()
        {
            refocus.set_value(false);
            let _ = button.focus();
        }
    });

    // Enter ends editing and the field then loses focus, so only the first of
    // the two counts.
    let finish = move |value: Option<String>, keyboard: bool| {
        if !editing.get_untracked() {
            return;
        }
        refocus.set_value(keyboard);
        editing.set(false);
        if let Some(Err(message)) = value.map(|value| document.rename(&value)) {
            status.show_note(message);
        }
    };
    let on_keydown = move |e: ev::KeyboardEvent| match e.key().as_str() {
        "Enter" => finish(Some(event_target_value(&e)), true),
        "Escape" => finish(None, true),
        _ => {}
    };

    view! {
        {move || {
            if editing.get() {
                view! {
                    <span class="file-name-field">
                        <span class="file-name-sizer" aria-hidden="true">
                            {move || draft.get()}
                        </span>
                        <input
                            node_ref=field
                            class="file-name-input"
                            type="text"
                            size="1"
                            spellcheck="false"
                            autocomplete="off"
                            on:input=move |e| draft.set(event_target_value(&e))
                            on:keydown=on_keydown
                            on:blur=move |e| finish(Some(event_target_value(&e)), false)
                        />
                    </span>
                }
                    .into_any()
            } else {
                view! {
                    <button
                        node_ref=button
                        class="file-name"
                        title=move || document.name.get()
                        aria-label="Rename file"
                        on:click=move |_| editing.set(true)
                    >
                        <span class="file-name-text">{move || document.name.get()}</span>
                        {move || {
                            document
                                .is_modified()
                                .then(|| {
                                    view! {
                                        <span class="file-name-marker" aria-label="Unsaved changes">
                                            "•"
                                        </span>
                                    }
                                })
                        }}
                    </button>
                }
                    .into_any()
            }
        }}
    }
}
