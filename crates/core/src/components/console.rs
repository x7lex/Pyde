use leptos::prelude::*;

/// Read-only output pane, one entry per line of program output.
#[component]
pub fn Console(lines: RwSignal<Vec<String>>) -> impl IntoView {
    view! {
        <pre class="console">
            {move || lines.get().join("\n")}
        </pre>
    }
}
