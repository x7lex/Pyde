//! The Output tab: program output, kept scrolled to the newest line, and the
//! `input()` field.

use leptos::{ev, prelude::*};

use crate::state::{MAX_LINES, Runner};

#[component]
pub fn Console(runner: Runner) -> impl IntoView {
    let pane = NodeRef::<leptos::html::Div>::new();
    let field = NodeRef::<leptos::html::Input>::new();

    Effect::new(move || {
        runner.output.track();
        runner.prompt.track();
        if let Some(pane) = pane.get() {
            pane.set_scroll_top(pane.scroll_height());
        }
    });
    Effect::new(move || {
        if let Some(field) = field.get() {
            let _ = field.focus();
        }
    });

    let on_keydown = move |e: ev::KeyboardEvent| {
        if e.key() == "Enter" {
            runner.answer(event_target_value(&e));
        }
    };

    view! {
        <div class="console" node_ref=pane>
            {move || {
                runner
                    .truncated
                    .get()
                    .then(|| {
                        view! {
                            <p class="notice">
                                {format!(
                                    "Older output hidden: showing the last {MAX_LINES} lines.",
                                )}
                            </p>
                        }
                    })
            }}
            <pre class="console-output">
                <For
                    each=move || runner.output.get()
                    key=|line| line.id
                    children=|line| {
                        view! {
                            <span class="console-line" class=("is-error", line.is_error)>
                                {line.text}
                                "\n"
                            </span>
                        }
                    }
                />
            </pre>
            {move || {
                runner
                    .prompt
                    .get()
                    .map(|prompt| {
                        view! {
                            <label class="console-input">
                                <span>{prompt}</span>
                                <input
                                    node_ref=field
                                    type="text"
                                    spellcheck="false"
                                    autocomplete="off"
                                    on:keydown=on_keydown
                                />
                            </label>
                        }
                    })
            }}
        </div>
    }
}
