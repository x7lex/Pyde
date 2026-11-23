//! The root component, which creates the state and lays out the two panes.

use leptos::{ev, prelude::*};

use crate::components::{editor_pane::EditorPane, output_pane::OutputPane};
use crate::editing::textarea;
use crate::state::{Checker, Document, Runner, Status};

const STARTER: &str = "print(\"Hello, Pyde!\")\n";

#[component]
pub fn App() -> impl IntoView {
    let document = Document::new(STARTER);
    let status = Status::default();
    let checker = Checker::new(document);
    let runner = Runner::new(document, checker, status);
    let textarea_ref = NodeRef::<leptos::html::Textarea>::new();

    let open = move || document.open(move |message| runner.print_error(message));
    let save = move || document.save(move |message| runner.print_error(message));
    let format = move || {
        let note = textarea_ref.get().and_then(|area| document.format(&area));
        if let Some(note) = note {
            status.show_note(note);
        }
    };
    let jump_to = move |(line, col): (usize, usize)| {
        if let Some(area) = textarea_ref.get() {
            document
                .code
                .with_untracked(|text| textarea::reveal(&area, text, line, col));
        }
    };

    let shortcuts = window_event_listener(ev::keydown, move |e| {
        if e.shift_key() && e.alt_key() && e.code() == "KeyF" {
            e.prevent_default();
            return format();
        }
        if !(e.meta_key() || e.ctrl_key()) {
            return;
        }
        match e.key().as_str() {
            "s" => {
                e.prevent_default();
                save();
            }
            "o" => {
                e.prevent_default();
                open();
            }
            _ => {}
        }
    });
    on_cleanup(move || shortcuts.remove());

    view! {
        <main class="split">
            <EditorPane
                document=document
                checker=checker
                runner=runner
                status=status
                textarea_ref=textarea_ref
                on_open=Callback::new(move |_| open())
                on_save=Callback::new(move |_| save())
                on_format=Callback::new(move |_| format())
            />
            <OutputPane
                checker=checker
                runner=runner
                status=status
                on_jump=Callback::new(jump_to)
            />
        </main>
    }
}
