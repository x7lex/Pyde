//! Running the document with Pyodide, and the console output it produces.

use std::time::Duration;

use leptos::prelude::*;

use super::checker::Checker;
use super::debounce::Debounce;
use super::document::Document;
use super::status::Status;
use crate::clients::python::{PythonClient, PythonEvent};

/// The console keeps this many lines; older ones are dropped.
pub const MAX_LINES: usize = 5000;
/// Output can far outpace rendering, so it reaches the console this often.
const OUTPUT_INTERVAL: Duration = Duration::from_millis(100);
/// How long typing must pause before the packages it imports are loaded.
const PREFETCH_DELAY: Duration = Duration::from_millis(800);

#[derive(Clone)]
pub struct OutputLine {
    pub id: u64,
    pub text: String,
    pub is_error: bool,
}

#[derive(Clone, Copy)]
pub struct Runner {
    document: Document,
    checker: Checker,
    status: Status,
    pub output: RwSignal<Vec<OutputLine>>,
    pub truncated: RwSignal<bool>,
    /// What the program printed before asking for `input()`.
    pub prompt: RwSignal<Option<String>>,
    pub running: RwSignal<bool>,
    client: StoredValue<Option<PythonClient>, LocalStorage>,
    loaded: RwSignal<bool>,
    load_failed: RwSignal<bool>,
    /// Run pressed before Pyodide was ready: `(name, text)` to run once it is.
    queued: StoredValue<Option<(String, String)>>,
    /// Lines waiting for the next batch to the console.
    pending: StoredValue<Vec<(String, bool)>>,
    next_line_id: StoredValue<u64>,
    prefetch_timer: Debounce,
}

impl Runner {
    pub fn new(document: Document, checker: Checker, status: Status) -> Self {
        let this = Self {
            document,
            checker,
            status,
            output: RwSignal::new(Vec::new()),
            truncated: RwSignal::new(false),
            prompt: RwSignal::new(None),
            running: RwSignal::new(false),
            client: StoredValue::new_local(None),
            loaded: RwSignal::new(false),
            load_failed: RwSignal::new(false),
            queued: StoredValue::new(None),
            pending: StoredValue::new(Vec::new()),
            next_line_id: StoredValue::new(0),
            prefetch_timer: Debounce::default(),
        };
        this.client
            .set_value(Some(PythonClient::new(move |event| this.on_event(event))));

        Effect::new(move || {
            if this.loaded.get()
                && let Some((name, text)) = this.queued.try_update_value(Option::take).flatten()
            {
                this.with_client(|client| client.run(&name, &text));
            }
        });

        // Once typing pauses, load the packages the code imports so the type
        // checker knows them. Not while a program runs: Pyodide is busy, and a
        // run loads them anyway.
        Effect::new(move || {
            let text = document.code.get();
            if this.loaded.get() && !this.running.get() {
                this.prefetch_timer.call(PREFETCH_DELAY, move || {
                    this.with_client(|client| client.prefetch(&text))
                });
            } else {
                this.prefetch_timer.cancel();
            }
        });
        this
    }

    /// What Python is up to, for the header; empty when there is nothing to say.
    pub fn status_text(&self) -> &'static str {
        match (
            self.load_failed.get(),
            self.loaded.get(),
            self.running.get(),
        ) {
            (true, _, _) => "Python failed to load",
            (_, false, _) => "Loading Python…",
            (_, true, true) => "Running…",
            (_, true, false) => "",
        }
    }

    pub fn toggle(&self) {
        if self.running.get_untracked() {
            self.stop();
        } else {
            self.run();
        }
    }

    /// Runs the document, or queues it until Python has loaded.
    pub fn run(&self) {
        self.pending.update_value(Vec::clear);
        self.output.update(Vec::clear);
        self.truncated.set(false);
        self.running.set(true);
        let name = self.document.name.get_untracked();
        let text = self.document.code.get_untracked();
        self.with_client(|client| {
            if self.load_failed.get_untracked() {
                self.load_failed.set(false);
                client.restart();
            }
            if self.loaded.get_untracked() {
                client.run(&name, &text);
            } else {
                self.queued.set_value(Some((name, text)));
            }
        });
    }

    /// Kills the program by restarting Pyodide, which then loads again.
    pub fn stop(&self) {
        self.status.finish_loading();
        self.with_client(PythonClient::restart);
        self.queued.set_value(None);
        self.prompt.set(None);
        self.loaded.set(false);
        self.running.set(false);
        self.print_error("Stopped.");
    }

    /// Passes the reply to a program waiting in `input()`, and echoes it.
    pub fn answer(&self, text: String) {
        let asked = self.prompt.get_untracked().unwrap_or_default();
        self.prompt.set(None);
        self.with_client(|client| client.answer(&text));
        self.print_lines(vec![(format!("{asked}{text}"), false)]);
    }

    pub fn print_error(&self, text: impl Into<String>) {
        self.print_lines(vec![(text.into(), true)]);
    }

    fn on_event(&self, event: PythonEvent) {
        match event {
            PythonEvent::Ready => self.loaded.set(true),
            PythonEvent::Output(lines) => self.print_lines(lines),
            PythonEvent::Prefetching(names) => self.status.start_loading(&names),
            PythonEvent::Prefetched { json, failed } => {
                self.status.finish_loading();
                if failed {
                    self.status
                        .show_note("Couldn't load some packages for type checking");
                } else if json != "[]" {
                    self.checker.add_packages(json);
                }
            }
            PythonEvent::Input { prompt } => {
                self.flush();
                self.prompt.set(Some(prompt));
            }
            PythonEvent::Done => {
                self.flush();
                self.running.set(false);
            }
            PythonEvent::Failed(message) => {
                self.load_failed.set(true);
                self.queued.set_value(None);
                self.running.set(false);
                self.print_error(format!("Could not load Python: {message}"));
            }
        }
    }

    /// Queues lines for the console, which shows them in batches.
    fn print_lines(&self, lines: Vec<(String, bool)>) {
        let first = self.pending.with_value(Vec::is_empty);
        self.pending.update_value(|pending| pending.extend(lines));
        if first {
            let this = *self;
            set_timeout(move || this.flush(), OUTPUT_INTERVAL);
        }
    }

    fn flush(&self) {
        let mut lines = self
            .pending
            .try_update_value(std::mem::take)
            .unwrap_or_default();
        if lines.is_empty() {
            return;
        }
        lines.drain(..lines.len().saturating_sub(MAX_LINES));
        if self.output.with_untracked(Vec::len) + lines.len() > MAX_LINES {
            self.truncated.set(true);
        }
        self.output.update(|output| {
            output.extend(lines.into_iter().map(|(text, is_error)| {
                let id = self.next_line_id.get_value();
                self.next_line_id.set_value(id + 1);
                OutputLine { id, text, is_error }
            }));
            output.drain(..output.len().saturating_sub(MAX_LINES));
        });
    }

    fn with_client(&self, action: impl FnOnce(&PythonClient)) {
        let _ = self.client.try_with_value(|client| {
            if let Some(client) = client {
                action(client);
            }
        });
    }
}
