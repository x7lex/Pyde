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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_survive_json() {
        let result = CheckResult {
            diagnostics: vec![DiagnosticInfo {
                id: "x".into(),
                rule: "invalid-syntax".into(),
                message: "a \"quoted\" é".into(),
                severity: DiagnosticSeverity::Warning,
                start_byte: 1,
                end_byte: 2,
                start_line: 0,
                start_col: 1,
                end_line: 0,
                end_col: 2,
                help: Some("tip".into()),
            }],
            dead_code: vec![DeadCodeRange {
                start_byte: 3,
                end_byte: 4,
                is_unreachable: true,
            }],
        };
        assert_eq!(from_json::<CheckResult>(&to_json(&result)), Some(result));
    }

    #[test]
    fn hover_option_survives_json() {
        let hover = Some(HoverInfo {
            signature: "(variable) x: int".into(),
            docstring: None,
            start_byte: 0,
            end_byte: 1,
        });
        assert_eq!(
            from_json::<Option<HoverInfo>>(&to_json(&hover)),
            Some(hover)
        );
        assert_eq!(
            from_json::<Option<HoverInfo>>(&to_json(&None::<HoverInfo>)),
            Some(None)
        );
    }
}
