//! Notes and loading messages for the header.

use std::time::Duration;

use leptos::prelude::*;

use super::debounce::Debounce;

const NOTE_DURATION: Duration = Duration::from_secs(4);

/// Short messages for the right-hand header: one-off notes about the editor,
/// such as why formatting didn't happen, and what is being loaded.
#[derive(Clone, Copy, Default)]
pub struct Status {
    note: RwSignal<Option<String>>,
    loading: RwSignal<Option<String>>,
    note_timer: Debounce,
}

impl Status {
    /// Shows `text` for a few seconds, replacing any earlier note.
    pub fn show_note(&self, text: impl Into<String>) {
        self.note.set(Some(text.into()));
        let note = self.note;
        self.note_timer.call(NOTE_DURATION, move || note.set(None));
    }

    /// Shows that packages are loading, until `finish_loading`.
    pub fn start_loading(&self, names: &str) {
        self.loading.set(Some(format!("Loading {names}…")));
    }

    /// Clears the loading message, whether the packages arrived or not.
    pub fn finish_loading(&self) {
        self.loading.set(None);
    }

    /// The note if there is one, otherwise the loading message.
    pub fn text(&self) -> Option<String> {
        self.note.get().or_else(|| self.loading.get())
    }
}
