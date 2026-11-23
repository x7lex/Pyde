//! Formatting with Ruff's defaults: a four-space indent, double quotes, a line
//! width of 88 and magic trailing commas.

use ruff_python_formatter::{FormatModuleError, PyFormatOptions, format_module_source};

use crate::highlight::tokenize;

/// Formatted code, with what is needed to carry a caret across the change.
pub struct Formatted {
    pub code: String,
    source: String,
}

/// How far ahead a token may have moved when the formatter adds or removes
/// tokens (commas, parentheses) between two that it keeps.
const ALIGN_LOOKAHEAD: usize = 12;

type Span = (usize, usize);

/// The tokens that are not whitespace, each with the text it covers.
fn significant_tokens(code: &str) -> Vec<(Span, String)> {
    tokenize(code)
        .into_iter()
        .filter(|t| !t.text.trim().is_empty())
        .map(|t| {
            let range = (t.range.start().to_usize(), t.range.end().to_usize());
            // The formatter may swap quote style, which doesn't change the token.
            (range, t.text.trim_matches(['"', '\'']).to_string())
        })
        .collect()
}

impl Formatted {
    /// Where the byte at `offset` of the original text ended up in `code`, so a
    /// caret can stay on the same token. The formatter only moves whitespace and
    /// adds or drops a few tokens, so the two token streams are lined up in order.
    pub fn map_offset(&self, offset: usize) -> usize {
        let before = significant_tokens(&self.source);
        let after = significant_tokens(&self.code);

        let mut aligned: Vec<Option<usize>> = vec![None; before.len()];
        let mut next = 0;
        for (i, (_, text)) in before.iter().enumerate() {
            let window_end = (next + ALIGN_LOOKAHEAD).min(after.len());
            if let Some(k) = (next..window_end).find(|&k| after[k].1 == *text) {
                aligned[i] = Some(k);
                next = k + 1;
            }
        }

        let clamp = |mapped: usize| {
            let mut mapped = mapped.min(self.code.len());
            while !self.code.is_char_boundary(mapped) {
                mapped -= 1;
            }
            mapped
        };
        let end_of = |i: usize| aligned[i].map(|k| after[k].0.1);

        // Inside a token: same place inside its counterpart.
        if let Some(i) = before
            .iter()
            .position(|((start, end), _)| (*start..*end).contains(&offset))
        {
            return match aligned[i] {
                Some(k) => clamp(after[k].0.0 + (offset - before[i].0.0)),
                None => (0..i).rev().find_map(end_of).map_or(0, clamp),
            };
        }

        // Between tokens: stay with the token before on the same line, otherwise
        // move to the start of the next one.
        let prev = before.iter().rposition(|((_, end), _)| *end <= offset);
        let next_token = before.iter().position(|((start, _), _)| *start > offset);
        if let Some(p) = prev {
            let gap = self.source.get(before[p].0.1..offset).unwrap_or("");
            if !gap.contains('\n')
                && let Some(end) = (0..=p).rev().find_map(end_of)
            {
                return clamp(end);
            }
        }
        match next_token {
            Some(n) => (n..before.len())
                .find_map(|i| aligned[i].map(|k| after[k].0.0))
                .map_or(self.code.len(), clamp),
            None => self.code.len(),
        }
    }
}

/// Formats `code`. Fails, with a message that says which line, if it doesn't parse.
pub fn format(code: &str) -> Result<Formatted, String> {
    let printed =
        format_module_source(code, PyFormatOptions::default()).map_err(|error| match &error {
            FormatModuleError::ParseError(parse) => {
                let at = parse.location.start().to_usize().min(code.len());
                let line = code
                    .get(..at)
                    .map_or(1, |head| head.matches('\n').count() + 1);
                format!("line {line}: {}", parse.error)
            }
            other => other.to_string(),
        })?;
    Ok(Formatted {
        code: printed.into_code(),
        source: code.to_string(),
    })
}
