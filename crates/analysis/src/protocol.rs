//! The messages the type-checker worker sends the page, carried as JSON text
//! over `postMessage`.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    pub id: String,
    pub rule: String,
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub help: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadCodeRange {
    pub start_byte: usize,
    pub end_byte: usize,
    pub is_unreachable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoverInfo {
    pub signature: String,
    pub docstring: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub diagnostics: Vec<DiagnosticInfo>,
    pub dead_code: Vec<DeadCodeRange>,
}

/// The importable files of one installed package, as `(path, text)` pairs with
/// paths relative to `site-packages`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageFiles {
    pub name: String,
    pub files: Vec<(String, String)>,
}

/// Serialises `value` for `postMessage`.
pub fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// Reads what `to_json` wrote, or `None` if it is not that.
pub fn from_json<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_str(text).ok()
}
