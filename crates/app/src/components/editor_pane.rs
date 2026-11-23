//! The left pane: the file name and actions above the editor.

use leptos::prelude::*;

use super::editor::Editor;
use super::file_name::FileName;
use crate::state::{Checker, Document, Runner, Status};

#[component]
pub fn EditorPane(
    document: Document,
    checker: Checker,
    runner: Runner,
    status: Status,
    textarea_ref: NodeRef<leptos::html::Textarea>,
    on_open: Callback<()>,
    on_save: Callback<()>,
    on_format: Callback<()>,
) -> impl IntoView {
    view! {
        <section class="pane">
            <header class="pane-header">
                <FileName document=document status=status />
                <div class="pane-actions">
                    <button class="button" on:click=move |_| on_open.run(())>
                        "Open"
                    </button>
                    <button class="button" on:click=move |_| on_save.run(())>
                        "Save"
                    </button>
                    <button
                        class="button"
                        title="Format with Ruff (Shift+Alt+F)"
                        on:click=move |_| on_format.run(())
                    >
                        "Format"
                    </button>
                    <button
                        class="button"
                        class=("is-primary", move || !runner.running.get())
                        class=("is-danger", move || runner.running.get())
                        on:click=move |_| runner.toggle()
                    >
                        {move || if runner.running.get() { "Stop" } else { "Run" }}
                    </button>
                </div>
            </header>
            <Editor code=document.code checker=checker textarea_ref=textarea_ref />
        </section>
    }
}
