use std::time::Duration;

use leptos::{ev, prelude::*, task::spawn_local};
use web_sys::FileSystemFileHandle;

use crate::components::{
    console::{Console, Line},
    editor::Editor,
};
use crate::files;
use crate::python::{Event, Python};

const STARTER: &str = "print(\"Hello, Pyde!\")\n";
const MAX_LINES: usize = 5000;
const OUTPUT_INTERVAL: Duration = Duration::from_millis(100);

/// Root component: editor on the left, output on the right.
#[component]
pub fn App() -> impl IntoView {
    let code = RwSignal::new(STARTER.to_string());
    let saved_code = RwSignal::new(STARTER.to_string());
    let file_name = RwSignal::new("main.py".to_string());
    let file_handle = StoredValue::new_local(None::<FileSystemFileHandle>);

    let output = RwSignal::new(Vec::<Line>::new());
    let loaded = RwSignal::new(false);
    let load_failed = RwSignal::new(false);
    // Run pressed before Pyodide is ready: (name, code) to run once it is.
    let queued_run = StoredValue::new(None::<(String, String)>);
    let truncated = RwSignal::new(false);
    let prompt = RwSignal::new(None::<String>);
    let running = RwSignal::new(false);

    // Output can far outpace rendering, so lines collect in `pending` and reach
    // the console in batches.
    let pending = StoredValue::new(Vec::<(String, bool)>::new());
    let next_id = StoredValue::new(0u64);
    let flush = move || {
        let mut new_lines = pending.try_update_value(std::mem::take).unwrap_or_default();
        if new_lines.is_empty() {
            return;
        }
        new_lines.drain(..new_lines.len().saturating_sub(MAX_LINES));
        if output.with_untracked(Vec::len) + new_lines.len() > MAX_LINES {
            truncated.set(true);
        }
        output.update(|lines| {
            lines.extend(new_lines.into_iter().map(|(text, is_error)| {
                let id = next_id.get_value();
                next_id.set_value(id + 1);
                Line { id, text, is_error }
            }));
            lines.drain(..lines.len().saturating_sub(MAX_LINES));
        });
    };
    let print_lines = move |new_lines: Vec<(String, bool)>| {
        let first = pending.with_value(Vec::is_empty);
        pending.update_value(|lines| lines.extend(new_lines));
        if first {
            set_timeout(flush, OUTPUT_INTERVAL);
        }
    };
    let print_error = move |text: String| print_lines(vec![(text, true)]);

    let python = StoredValue::new_local(Python::new(move |event| match event {
        Event::Ready => {
            loaded.set(true);
        }
        Event::Output(lines) => print_lines(lines),
        Event::Input { prompt: text } => {
            flush();
            prompt.set(Some(text));
        }
        Event::Done => {
            flush();
            running.set(false);
        }
        Event::Failed(message) => {
            load_failed.set(true);
            queued_run.set_value(None);
            running.set(false);
            print_error(format!("Could not load Python: {message}"));
        }
    }));

    Effect::new(move || {
        if loaded.get()
            && let Some((name, text)) = queued_run.try_update_value(Option::take).flatten()
        {
            python.with_value(|python| python.run(&name, &text));
        }
    });

    let run = move || {
        pending.update_value(Vec::clear);
        output.update(Vec::clear);
        truncated.set(false);
        running.set(true);
        let name = file_name.get_untracked();
        let text = code.get_untracked();
        python.with_value(|python| {
            if load_failed.get_untracked() {
                load_failed.set(false);
                python.restart();
            }
            if loaded.get_untracked() {
                python.run(&name, &text);
            } else {
                queued_run.set_value(Some((name, text)));
            }
        });
    };

    let answer = Callback::new(move |text: String| {
        let asked = prompt.get_untracked().unwrap_or_default();
        prompt.set(None);
        python.with_value(|python| python.answer(&text));
        print_lines(vec![(format!("{asked}{text}"), false)]);
    });

    let stop = move || {
        python.with_value(Python::restart);
        queued_run.set_value(None);
        prompt.set(None);
        loaded.set(false);
        running.set(false);
        print_error("Stopped.".to_string());
    };

    let open = move || {
        spawn_local(async move {
            let opened = match files::open().await {
                Ok(Some(opened)) => opened,
                Ok(None) => return,
                Err(message) => return print_error(format!("Could not open file: {message}")),
            };
            // Asked after picking, because the picker needs the click's user
            // activation and a dialog could outlast it.
            if code.get_untracked() != saved_code.get_untracked() {
                let question = format!("Discard unsaved changes and open {}?", opened.name);
                if !window().confirm_with_message(&question).unwrap_or(false) {
                    return;
                }
            }
            file_handle.set_value(opened.handle);
            file_name.set(opened.name);
            saved_code.set(opened.text.clone());
            code.set(opened.text);
        });
    };

    let save = move || {
        let text = code.get_untracked();
        let name = file_name.get_untracked();
        let handle = file_handle.get_value();
        spawn_local(async move {
            match files::save(handle, name, text.clone()).await {
                Ok(Some(saved)) => {
                    file_handle.set_value(saved.handle);
                    file_name.set(saved.name);
                    saved_code.set(text);
                }
                Ok(None) => {}
                Err(message) => print_error(format!("Could not save file: {message}")),
            }
        });
    };

    // Without these the browser opens its own open and save-page dialogs.
    let shortcuts = window_event_listener(ev::keydown, move |e| {
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

    let title = move || {
        let marker = if code.get() == saved_code.get() {
            ""
        } else {
            " •"
        };
        format!("{}{marker}", file_name.get())
    };
    let status = move || match (load_failed.get(), loaded.get(), running.get()) {
        (true, _, _) => "Python failed to load",
        (_, false, _) => "Loading Python…",
        (_, true, true) => "Running…",
        (_, true, false) => "",
    };

    view! {
        <main class="split">
            <section class="pane">
                <header class="pane-header">
                    <span>{title}</span>
                    <div class="actions">
                        <button class="button" on:click=move |_| open()>"Open"</button>
                        <button class="button" on:click=move |_| save()>"Save"</button>
                        <button
                            class="button"
                            class=("run", move || !running.get())
                            class=("stop", move || running.get())
                            on:click=move |_| if running.get_untracked() { stop() } else { run() }
                        >
                            {move || if running.get() { "Stop" } else { "Run" }}
                        </button>
                    </div>
                </header>
                <Editor code=code />
            </section>
            <section class="pane">
                <header class="pane-header">
                    <span>"Output"</span>
                    <span>{status}</span>
                </header>
                <Console
                    lines=output
                    max_lines=MAX_LINES
                    truncated=truncated
                    prompt=prompt
                    on_answer=answer
                />
            </section>
        </main>
    }
}
