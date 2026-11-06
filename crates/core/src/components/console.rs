use leptos::prelude::*;

#[derive(Clone)]
pub struct Line {
    pub id: u64,
    pub text: String,
    pub is_error: bool,
}

/// Read-only output pane that follows the newest line.
#[component]
pub fn Console(lines: RwSignal<Vec<Line>>) -> impl IntoView {
    let pane = NodeRef::<leptos::html::Pre>::new();

    Effect::new(move || {
        lines.track();
        if let Some(pane) = pane.get() {
            pane.set_scroll_top(pane.scroll_height());
        }
    });

    view! {
        <pre class="console" node_ref=pane>
            <For
                each=move || lines.get()
                key=|line| line.id
                children=|line| view! {
                    <span class:error=line.is_error>{line.text}"\n"</span>
                }
            />
        </pre>
    }
}
