//! How a diagnostic's severity looks, shared by the hover card and the Problems tab.

use pyde_analysis::protocol::DiagnosticSeverity;

pub fn icon(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "✕",
        DiagnosticSeverity::Warning => "⚠",
        DiagnosticSeverity::Info => "ℹ",
    }
}

/// The state class that colours an element for `severity`.
pub fn state_class(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "is-error",
        DiagnosticSeverity::Warning => "is-warning",
        DiagnosticSeverity::Info => "is-info",
    }
}
