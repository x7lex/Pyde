use leptos::prelude::*;

use crate::components::{console::Console, editor::Editor};

const STARTER: &str = "print(\"Hello, Pyde!\")\n";

/// Root component: editor on the left, output on the right.
#[component]
pub fn App() -> impl IntoView {
    let code = RwSignal::new(STARTER.to_string());
    let output = RwSignal::new(Vec::<String>::new());

    // Wired to Pyodide in a later step; for now it just clears the output.
    let run = move |_| output.set(Vec::new());

    view! {
        <main class="split">
            <section class="pane">
                <header class="pane-header">
                    <span>"main.py"</span>
                    <button class="run" on:click=run>"Run"</button>
                </header>
                <Editor code=code />
            </section>
            <section class="pane">
                <header class="pane-header">
                    <span>"Output"</span>
                </header>
                <Console lines=output />
            </section>
        </main>
    }
}
