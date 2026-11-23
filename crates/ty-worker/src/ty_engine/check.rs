//! Checking a file for diagnostics and dead code.

use std::cell::OnceCell;

use pyde_analysis::protocol::{DeadCodeRange, DiagnosticInfo, DiagnosticSeverity};
use ruff_db::diagnostic::Severity;
use ruff_db::files::File;
use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPathBuf};
use ruff_source_file::LineIndex;
use ruff_text_size::{TextRange, TextSize};
use ty_python_semantic::Db as SemanticDb;
use ty_python_semantic::types::ide_support::{unreachable_ranges, unused_bindings};

use super::TyEngine;

/// Moves a diagnostic that covers no visible text onto the nearest token, so there is
/// something to underline.
fn adjust_diagnostic_range<'a>(
    code: &'a str,
    tokens: &OnceCell<Vec<pyde_analysis::highlight::HighlightToken<'a>>>,
    mut start: usize,
    mut end: usize,
) -> (usize, usize) {
    if code.is_empty() {
        return (0, 0);
    }

    start = start.min(code.len());
    end = end.min(code.len());

    // A diagnostic that already covers visible text is kept as-is.
    if start < end
        && let Some(slice) = code.get(start..end)
        && !slice.trim().is_empty()
    {
        return (start, end);
    }

    // Otherwise it snaps to the token at, or just before, the diagnostic.
    let tokens = tokens.get_or_init(|| pyde_analysis::highlight::tokenize(code));
    let target_size = TextSize::try_from(start).unwrap_or_default();
    let visible = |t: &&pyde_analysis::highlight::HighlightToken<'_>| !t.text.trim().is_empty();

    if (start >= code.len() || code.get(start..).is_none_or(|rest| rest.trim().is_empty()))
        && let Some(tok) = tokens.iter().rev().find(visible)
    {
        return (tok.range.start().to_usize(), tok.range.end().to_usize());
    }

    if let Some(tok) = tokens
        .iter()
        .filter(visible)
        .take_while(|t| t.range.start() <= target_size)
        .last()
    {
        return (tok.range.start().to_usize(), tok.range.end().to_usize());
    }

    let before = code.get(..start).unwrap_or("");
    if let Some((idx, c)) = before.char_indices().rfind(|(_, c)| !c.is_whitespace()) {
        return (idx, idx + c.len_utf8());
    }

    (start, end)
}

impl TyEngine {
    /// Checks `code` as the file `filename`, returning its diagnostics and
    /// dead-code ranges.
    pub fn check(
        &mut self,
        filename: &str,
        code: &str,
    ) -> (Vec<DiagnosticInfo>, Vec<DeadCodeRange>) {
        let path = if filename.starts_with('/') {
            SystemPathBuf::from(filename)
        } else {
            SystemPathBuf::from(format!("/{filename}"))
        };

        // A renamed buffer would otherwise leave its old file importable.
        if let Some(old) = self.current_path.take()
            && old != path
        {
            if let Ok(old_file) = ruff_db::files::system_path_to_file(&self.db, &old) {
                self.db.open_files.remove(&old_file);
            }
            let _ = self.db.test_system().memory_file_system().remove_file(&old);
            File::sync_path(&mut self.db, &old);
        }

        if self.db.write_file(&path, code).is_err() {
            return (Vec::new(), Vec::new());
        }
        self.current_path = Some(path.clone());

        let file = match ruff_db::files::system_path_to_file(&self.db, &path) {
            Ok(file) => file,
            Err(_) => return (Vec::new(), Vec::new()),
        };

        self.db.open_files.insert(file);
        self.current_file = Some(file);

        let diagnostics = self.db.check_file(file);
        let line_idx = LineIndex::from_source_text(code);
        let tokens = OnceCell::new();

        let diags = diagnostics
            .into_iter()
            .map(|diag| {
                let severity = match diag.severity() {
                    Severity::Fatal | Severity::Error => DiagnosticSeverity::Error,
                    Severity::Warning => DiagnosticSeverity::Warning,
                    Severity::Info | Severity::Hint => DiagnosticSeverity::Info,
                };

                let span = diag.primary_span();
                let range = span.and_then(|s| s.range()).unwrap_or_else(|| {
                    TextRange::new(TextSize::from(0), TextSize::from(code.len() as u32))
                });

                let (start_byte, end_byte) = adjust_diagnostic_range(
                    code,
                    &tokens,
                    range.start().to_usize(),
                    range.end().to_usize(),
                );

                let start_size = TextSize::try_from(start_byte).unwrap_or(range.start());
                let end_size = TextSize::try_from(end_byte).unwrap_or(range.end());
                let start_loc = line_idx.line_column(start_size, code);
                let end_loc = line_idx.line_column(end_size, code);

                DiagnosticInfo {
                    id: diag.id().as_str().to_string(),
                    rule: diag.secondary_code_or_id().to_string(),
                    message: diag.headline_message().to_string(),
                    severity,
                    start_byte,
                    end_byte,
                    start_line: start_loc.line.to_zero_indexed(),
                    start_col: start_loc.column.to_zero_indexed(),
                    end_line: end_loc.line.to_zero_indexed(),
                    end_col: end_loc.column.to_zero_indexed(),
                    help: diag.first_help_text().map(str::to_string),
                }
            })
            .collect();

        let program_file = self.db.program_file(file);
        let unreachable = unreachable_ranges(&self.db, program_file);
        let unused = unused_bindings(&self.db, program_file);

        let mut dead_ranges = Vec::with_capacity(unreachable.len() + unused.len());
        for u in unreachable.iter() {
            dead_ranges.push(DeadCodeRange {
                start_byte: u.range.start().to_usize(),
                end_byte: u.range.end().to_usize(),
                is_unreachable: true,
            });
        }
        for b in unused.iter() {
            dead_ranges.push(DeadCodeRange {
                start_byte: b.range.start().to_usize(),
                end_byte: b.range.end().to_usize(),
                is_unreachable: false,
            });
        }

        (diags, dead_ranges)
    }
}
