//! Type checking of the document, through the type-checker worker.

use std::time::Duration;

use leptos::prelude::*;
use pyde_analysis::protocol::{DeadCodeRange, DiagnosticInfo, HoverInfo};

use super::debounce::Debounce;
use super::document::Document;
use crate::clients::ty::{TyClient, TyEvent};

/// Typing pauses this long before the type checker runs.
const CHECK_DELAY: Duration = Duration::from_millis(150);

/// Type checking of the document, done by a worker. One check is in flight at a
/// time, and whatever the editor holds by the time it finishes is checked next.
#[derive(Clone, Copy)]
pub struct Checker {
    document: Document,
    pub diagnostics: RwSignal<Vec<DiagnosticInfo>>,
    pub dead_code: RwSignal<Vec<DeadCodeRange>>,
    client: StoredValue<Option<TyClient>, LocalStorage>,
    ready: StoredValue<bool>,
    /// The `(name, text)` to check as soon as nothing is in flight.
    wanted: StoredValue<Option<(String, String)>>,
    in_flight: StoredValue<Option<(u64, String)>>,
    next_id: StoredValue<u64>,
    /// The text, and the check id, that `diagnostics` and hovers refer to.
    checked: StoredValue<String>,
    checked_id: StoredValue<u64>,
    /// What the checker was told about installed packages, in case it restarts
    /// and forgets.
    packages: StoredValue<Vec<String>>,
    timer: Debounce,
}

impl Checker {
    pub fn new(document: Document) -> Self {
        let this = Self {
            document,
            diagnostics: RwSignal::new(Vec::new()),
            dead_code: RwSignal::new(Vec::new()),
            client: StoredValue::new_local(None),
            ready: StoredValue::new(false),
            wanted: StoredValue::new(None),
            in_flight: StoredValue::new(None),
            next_id: StoredValue::new(0),
            checked: StoredValue::new(String::new()),
            checked_id: StoredValue::new(0),
            packages: StoredValue::new(Vec::new()),
            timer: Debounce::default(),
        };
        this.client
            .set_value(Some(TyClient::new(move |event| this.on_event(event))));

        Effect::new(move || {
            let (name, text) = (document.name.get(), document.code.get());
            this.timer.call(CHECK_DELAY, move || this.want(name, text));
        });
        this
    }

    /// Asks for hover info at a byte `offset`, and passes the answer to `respond`
    /// (`None` when there is nothing to say). Offsets only mean something against
    /// the text they were computed from, so while the editor is ahead of the last
    /// check the answer is always `None`.
    pub fn hover(&self, offset: usize, respond: impl FnOnce(Option<HoverInfo>) + 'static) {
        let current = self
            .document
            .code
            .with_untracked(|text| self.checked.with_value(|seen| text == seen));
        if !current {
            return respond(None);
        }
        let version = self.checked_id.get_value();
        let mut respond = Some(respond);
        self.with_client(|client| {
            if let Some(respond) = respond.take() {
                client.hover(version, offset, respond);
            }
        });
        if let Some(respond) = respond {
            respond(None);
        }
    }

    /// Tells the checker about the installed packages in `json` (a
    /// `Vec<PackageFiles>`), then checks again so their imports resolve.
    pub fn add_packages(&self, json: String) {
        let is_new = self
            .packages
            .try_update_value(|all| {
                let is_new = !all.contains(&json);
                if is_new {
                    all.push(json.clone());
                }
                is_new
            })
            .unwrap_or(false);
        if is_new && self.ready.get_value() {
            self.with_client(|client| client.add_packages(&json));
            self.want(
                self.document.name.get_untracked(),
                self.document.code.get_untracked(),
            );
        }
    }

    fn on_event(&self, event: TyEvent) {
        match event {
            TyEvent::Ready => {
                self.ready.set_value(true);
                self.packages.with_value(|all| {
                    self.with_client(|client| {
                        all.iter().for_each(|json| client.add_packages(json))
                    });
                });
                self.pump();
            }
            TyEvent::Checked { id, result } => {
                let sent = self
                    .in_flight
                    .try_update_value(|flight| match flight {
                        Some((sent, _)) if *sent == id => flight.take(),
                        _ => None,
                    })
                    .flatten();
                if let Some((_, text)) = sent {
                    self.checked.set_value(text);
                    self.checked_id.set_value(id);
                    self.diagnostics.set(result.diagnostics);
                    self.dead_code.set(result.dead_code);
                }
                self.pump();
            }
            TyEvent::Crashed => {
                self.ready.set_value(false);
                self.in_flight.set_value(None);
                self.checked.set_value(String::new());
                self.checked_id.set_value(0);
                self.diagnostics.set(Vec::new());
                self.dead_code.set(Vec::new());
                self.with_client(TyClient::restart);
            }
        }
    }

    fn want(&self, name: String, text: String) {
        self.wanted.set_value(Some((name, text)));
        self.pump();
    }

    /// Sends the wanted check, if the checker is free to take it.
    fn pump(&self) {
        if !self.ready.get_value() || self.in_flight.with_value(Option::is_some) {
            return;
        }
        let Some((name, text)) = self.wanted.try_update_value(Option::take).flatten() else {
            return;
        };
        let id = self.next_id.get_value() + 1;
        self.next_id.set_value(id);
        self.with_client(|client| client.check(id, &name, &text));
        self.in_flight.set_value(Some((id, text)));
    }

    fn with_client(&self, action: impl FnOnce(&TyClient)) {
        let _ = self.client.try_with_value(|client| {
            if let Some(client) = client {
                action(client);
            }
        });
    }
}
