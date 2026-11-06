use leptos::{ev, prelude::*};

#[derive(Clone)]
pub struct Line {
    pub id: u64,
    pub text: String,
    pub is_error: bool,
}

/// Read-only output pane that follows the newest line. While a program waits in
/// `input()`, `prompt` is set and a field for the reply appears after the output.
#[component]
pub fn Console(
    lines: RwSignal<Vec<Line>>,
    max_lines: usize,
    truncated: RwSignal<bool>,
    prompt: RwSignal<Option<String>>,
    on_answer: Callback<String>,
) -> impl IntoView {
    let pane = NodeRef::<leptos::html::Div>::new();
    let field = NodeRef::<leptos::html::Input>::new();

    Effect::new(move || {
        lines.track();
        prompt.track();
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
            on_answer.run(event_target_value(&e));
        }
    };

    view! {
        <div class="console" node_ref=pane>
            {move || truncated.get().then(|| view! {
                <p class="console-notice">{format!("Older output hidden: showing the last {max_lines} lines.")}</p>
            })}
            <pre class="console-output">
                <For
                    each=move || lines.get()
                    key=|line| line.id
                    children=|line| view! {
                        <span class:error=line.is_error>{line.text}"\n"</span>
                    }
                />
            </pre>
            {move || prompt.get().map(|prompt| view! {
                <label class="console-input">
                    <span>{prompt}</span>
                    <input node_ref=field type="text" spellcheck="false" autocomplete="off" on:keydown=on_keydown />
                </label>
            })}
        </div>
    }
}
