//! The file being edited: its text and name, and opening, saving and formatting it.

use leptos::{prelude::*, task::spawn_local};
use pyde_analysis::format;
use web_sys::{FileSystemFileHandle, HtmlTextAreaElement};

use crate::editing::indent::Edit;
use crate::editing::layout::utf16_to_byte;
use crate::editing::textarea;
use crate::files;

#[derive(Clone, Copy)]
pub struct Document {
    pub code: RwSignal<String>,
    pub name: RwSignal<String>,
    /// The text as last opened or saved, to tell whether there are unsaved changes.
    saved: RwSignal<String>,
    handle: StoredValue<Option<FileSystemFileHandle>, LocalStorage>,
    /// The textarea always holds `\n`; a file that used `\r\n` gets it back on save.
    crlf: StoredValue<bool>,
}

impl Document {
    pub fn new(starter: &str) -> Self {
        Self {
            code: RwSignal::new(starter.to_string()),
            name: RwSignal::new("main.py".to_string()),
            saved: RwSignal::new(starter.to_string()),
            handle: StoredValue::new_local(None),
            crlf: StoredValue::new(false),
        }
    }

    pub fn is_modified(&self) -> bool {
        self.code.get() != self.saved.get()
    }

    /// Renames the file, if `input` is an acceptable name. The file on disk is not
    /// touched, so the next save asks where to put it under the new name.
    pub fn rename(&self, input: &str) -> Result<(), String> {
        let name = normalize_name(input)?;
        if name != self.name.get_untracked() {
            self.handle.set_value(None);
            self.name.set(name);
        }
        Ok(())
    }

    /// Asks for a file and loads it. Failures go to `report`.
    pub fn open(self, report: impl Fn(String) + 'static) {
        spawn_local(async move {
            let opened = match files::open().await {
                Ok(Some(opened)) => opened,
                Ok(None) => return,
                Err(message) => return report(format!("Could not open file: {message}")),
            };
            if self.code.get_untracked() != self.saved.get_untracked() {
                let question = format!("Discard unsaved changes and open {}?", opened.name);
                if !window().confirm_with_message(&question).unwrap_or(false) {
                    return;
                }
            }
            let text = opened.text.replace("\r\n", "\n");
            self.crlf.set_value(text.len() != opened.text.len());
            self.handle.set_value(opened.handle);
            self.name.set(opened.name);
            self.saved.set(text.clone());
            self.code.set(text);
        });
    }

    /// Writes the text back to its file, asking where first if it has none.
    /// Failures go to `report`.
    pub fn save(self, report: impl Fn(String) + 'static) {
        let text = self.code.get_untracked();
        let name = self.name.get_untracked();
        let handle = self.handle.get_value();
        spawn_local(async move {
            let on_disk = if self.crlf.get_value() {
                text.replace('\n', "\r\n")
            } else {
                text.clone()
            };
            match files::save(handle, name, on_disk).await {
                Ok(Some(saved)) => {
                    self.handle.set_value(saved.handle);
                    self.name.set(saved.name);
                    self.saved.set(text);
                }
                Ok(None) => {}
                Err(message) => report(format!("Could not save file: {message}")),
            }
        });
    }

    /// Formats the whole file with Ruff's defaults, keeping the caret on its
    /// token and the view where it was. Returns a note when there is nothing to
    /// change, or why it couldn't.
    pub fn format(&self, area: &HtmlTextAreaElement) -> Option<String> {
        let text = area.value();
        let formatted = match format::format(&text) {
            Ok(formatted) => formatted,
            Err(message) => return Some(format!("Can't format: {message}")),
        };
        if formatted.code == text {
            return Some("Already formatted".to_string());
        }
        let caret = area.selection_start().ok().flatten().unwrap_or(0) as usize;
        let caret = formatted.map_offset(utf16_to_byte(&text, caret));
        let (left, top) = (area.scroll_left(), area.scroll_top());

        let edit = Edit {
            start: 0,
            end: text.len(),
            insert: formatted.code,
            select: (caret, caret),
        };
        let _ = area.focus();
        let new_text = textarea::apply(area, &edit);
        if self.code.with_untracked(|current| *current != new_text) {
            self.code.set(new_text);
        }
        area.set_scroll_left(left);
        area.set_scroll_top(top);
        None
    }
}

/// A file name for the program: plain, and a Python file. A name with no
/// extension gets `.py`.
fn normalize_name(input: &str) -> Result<String, String> {
    let name = input.trim();
    if name.is_empty() {
        return Err("The file name can't be empty".to_string());
    }
    if name.contains(['/', '\\', '\0']) {
        return Err("The file name can't contain slashes".to_string());
    }
    if !name.contains('.') {
        return Ok(format!("{name}.py"));
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.eq_ignore_ascii_case("py") => {
            Ok(name.to_string())
        }
        _ => Err("Python files end in .py".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_name;

    #[test]
    fn names_without_an_extension_become_python_files() {
        assert_eq!(normalize_name("script").unwrap(), "script.py");
        assert_eq!(normalize_name("  main.py \n").unwrap(), "main.py");
        assert_eq!(normalize_name("a.b.py").unwrap(), "a.b.py");
        assert_eq!(normalize_name("Tool.PY").unwrap(), "Tool.PY");
    }

    #[test]
    fn other_names_are_refused() {
        assert!(normalize_name("").is_err());
        assert!(normalize_name("   ").is_err());
        assert!(normalize_name("dir/main.py").is_err());
        assert!(normalize_name("dir\\main.py").is_err());
        assert!(normalize_name("notes.txt").is_err());
        assert!(normalize_name(".py").is_err());
        assert!(normalize_name("trailing.").is_err());
    }
}
