//! Rendering tokens as HTML spans, with diagnostics and dead code wrapped around them.

use super::guides::{blank_line_levels, push_guides};
use super::tokens::tokenize;
use crate::protocol::{DeadCodeRange, DiagnosticInfo, DiagnosticSeverity};

/// Renders `code` as HTML: syntax colours, diagnostic squiggles and dead-code dimming.
///
/// Diagnostic and dead-code classes wrap a whole run of tokens in one outer span,
/// so a wavy underline stays continuous instead of restarting at every token.
/// Each token keeps its own colour span inside.
pub fn to_html(code: &str, diagnostics: &[DiagnosticInfo], dead_code: &[DeadCodeRange]) -> String {
    type Wrap = (Option<&'static str>, Option<&'static str>);

    let mut html = String::with_capacity(code.len() * 2);
    let mut open: Option<Wrap> = None;
    let mut at_line_start = true;
    let bridges = blank_line_levels(code);
    let mut line = 0usize;

    for token in tokenize(code) {
        let token_start = token.range.start().to_usize();
        let token_end = token.range.end().to_usize();

        let diag_class = diagnostics
            .iter()
            .find(|d| d.start_byte < token_end && d.end_byte > token_start)
            .map(|d| match d.severity {
                DiagnosticSeverity::Error => "mark-error",
                DiagnosticSeverity::Warning => "mark-warning",
                DiagnosticSeverity::Info => "mark-info",
            });

        let dead_class = dead_code
            .iter()
            .find(|d| d.start_byte < token_end && d.end_byte > token_start)
            .map(|d| {
                if d.is_unreachable {
                    "mark-unreachable"
                } else {
                    "mark-unused"
                }
            });

        let wrap: Wrap = (diag_class, dead_class);
        let current = (wrap != (None, None)).then_some(wrap);
        if current != open {
            if open.is_some() {
                html.push_str("</span>");
            }
            if let Some((diag, dead)) = current {
                let classes: Vec<&str> = [diag, dead].into_iter().flatten().collect();
                html.push_str("<span class=\"");
                html.push_str(&classes.join(" "));
                html.push_str("\">");
            }
            open = current;
        }

        match token.kind.class() {
            Some(class) => {
                html.push_str("<span class=\"");
                html.push_str(class);
                html.push_str("\">");
                escape_into(&mut html, token.text);
                html.push_str("</span>");
                at_line_start = token.text.ends_with('\n');
                line += token.text.matches('\n').count();
            }
            None => push_plain(
                &mut html,
                token.text,
                &mut at_line_start,
                &mut line,
                &bridges,
            ),
        }
    }

    if open.is_some() {
        html.push_str("</span>");
    }
    html
}

/// Uncoloured text. Whitespace at the start of a line gets indent guides.
fn push_plain(
    html: &mut String,
    text: &str,
    at_line_start: &mut bool,
    line: &mut usize,
    bridges: &[usize],
) {
    for piece in text.split_inclusive('\n') {
        if piece == "\n"
            && *at_line_start
            && let Some(levels) = bridges.get(*line).filter(|levels| **levels > 0)
        {
            html.push_str(&"<span class=\"indent-bridge\"></span>".repeat(*levels));
        }
        if piece.ends_with('\n') {
            *line += 1;
        }
        let lead_len = if *at_line_start {
            piece.len() - piece.trim_start_matches([' ', '\t']).len()
        } else {
            0
        };
        let (lead, rest) = piece.split_at(lead_len);
        push_guides(html, lead);
        escape_into(html, rest);
        // Whitespace alone leaves the line still waiting for its first token.
        *at_line_start = piece.ends_with('\n') || (*at_line_start && rest.is_empty());
    }
}

fn escape_into(html: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            _ => html.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_at(start_byte: usize, end_byte: usize) -> DiagnosticInfo {
        DiagnosticInfo {
            id: "x".to_string(),
            rule: "invalid-syntax".to_string(),
            message: "m".to_string(),
            severity: DiagnosticSeverity::Error,
            start_byte,
            end_byte,
            start_line: 0,
            start_col: start_byte,
            end_line: 0,
            end_col: end_byte,
            help: None,
        }
    }

    fn dead(start_byte: usize, end_byte: usize, is_unreachable: bool) -> DeadCodeRange {
        DeadCodeRange {
            start_byte,
            end_byte,
            is_unreachable,
        }
    }

    #[test]
    fn diagnostic_squiggle_rendering() {
        let html = to_html("x = 42\n", &[error_at(0, 1)], &[]);
        assert!(html.contains("mark-error"));
    }

    #[test]
    fn squiggles_are_one_continuous_span() {
        let html = to_html("1 + 'a'\n", &[error_at(0, 7)], &[]);
        assert_eq!(
            html,
            "<span class=\"mark-error\"><span class=\"token-number\">1</span> + <span class=\"token-string\">'a'</span></span>\n"
        );
    }

    #[test]
    fn dead_code_runs_close_properly() {
        let html = to_html("a b c\n", &[], &[dead(2, 3, false)]);
        assert_eq!(
            html.matches("<span").count(),
            html.matches("</span>").count(),
            "{html}"
        );
        assert!(html.contains("mark-unused"));
    }

    #[test]
    fn dead_code_dimming_rendering() {
        let html = to_html("print('dead')\n", &[], &[dead(0, 13, true)]);
        assert!(html.contains("mark-unreachable"));
    }
}
