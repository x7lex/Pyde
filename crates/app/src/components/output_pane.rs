//! The right pane: the Output and Problems tabs, and the status text.

use leptos::prelude::*;

use super::console::Console;
use super::problems::Problems;
use crate::state::{Checker, Runner, Status};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Output,
    Problems,
}

#[component]
pub fn OutputPane(
    checker: Checker,
    runner: Runner,
    status: Status,
    /// Called with a zero-based `(line, column)` when a problem is clicked.
    on_jump: Callback<(usize, usize)>,
) -> impl IntoView {
    let tab = RwSignal::new(Tab::Output);

    let problems_label = move || match checker.diagnostics.get().len() {
        0 => "Problems".to_string(),
        count => format!("Problems ({count})"),
    };
    // What Python is doing outranks the editor's notes.
    let status_text = move || match runner.status_text() {
        "" => status.text().unwrap_or_default(),
        python => python.to_string(),
    };

    view! {
        <section class="pane">
            <header class="pane-header">
                <div class="tabs">
                    <button
                        class="tab"
                        class=("is-active", move || tab.get() == Tab::Output)
                        on:click=move |_| tab.set(Tab::Output)
                    >
                        "Output"
                    </button>
                    <button
                        class="tab"
                        class=("is-active", move || tab.get() == Tab::Problems)
                        on:click=move |_| tab.set(Tab::Problems)
                    >
                        {problems_label}
                    </button>
                </div>
                <span>{status_text}</span>
            </header>
            {move || match tab.get() {
                Tab::Output => view! { <Console runner=runner /> }.into_any(),
                Tab::Problems => {
                    view! { <Problems diagnostics=checker.diagnostics.into() on_jump=on_jump /> }
                        .into_any()
                }
            }}
        </section>
    }
}
