//! Tokenising source into coloured tokens that cover every byte of it.

use ruff_python_ast::token::TokenKind;
use ruff_python_parser::Mode;
use ruff_python_parser::lexer::lex;
use ruff_text_size::{TextRange, TextSize};

use super::kinds::Kind;
use super::semantics::{Semantics, classify_name};
use super::words::{CONTROL_KEYWORDS, OTHER_KEYWORDS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightToken<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub range: TextRange,
}

fn is_trivia(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Comment | TokenKind::NonLogicalNewline)
}

/// Tokenises `code` into semantic tokens. Source that fails to parse still
/// gets semantic colours, from whatever the parser recovered.
pub fn tokenize(code: &str) -> Vec<HighlightToken<'_>> {
    let semantics = Semantics::extract(code);

    let mut raw: Vec<(TokenKind, TextRange)> = Vec::new();
    let mut lexer = lex(code, Mode::Module);
    loop {
        let kind = lexer.next_token();
        let range = lexer.current_range();
        if kind == TokenKind::EndOfFile || range.start().to_usize() >= code.len() {
            break;
        }
        raw.push((kind, range));
    }

    let at = |offset: usize| TextSize::try_from(offset).unwrap_or_default();
    let mut tokens = Vec::with_capacity(raw.len() * 2);
    let mut last_end = 0usize;

    for (i, &(kind, range)) in raw.iter().enumerate() {
        let start = range.start().to_usize();
        let end = range.end().to_usize().min(code.len());
        if end <= start || start < last_end {
            continue;
        }
        if start > last_end {
            tokens.push(HighlightToken {
                kind: Kind::Plain,
                text: &code[last_end..start],
                range: TextRange::new(at(last_end), at(start)),
            });
        }

        // The lexer reports `match`, `case` and `type` as keywords everywhere.
        let kind = if matches!(kind, TokenKind::Match | TokenKind::Case | TokenKind::Type)
            && !semantics.soft_keywords.contains(&start)
        {
            TokenKind::Identifier
        } else {
            kind
        };

        let text = &code[start..end];
        let token_kind = if kind == TokenKind::Comment {
            Kind::Comment
        } else if matches!(
            kind,
            TokenKind::String
                | TokenKind::FStringStart
                | TokenKind::FStringMiddle
                | TokenKind::FStringEnd
        ) {
            Kind::String
        } else if matches!(kind, TokenKind::Int | TokenKind::Float | TokenKind::Complex) {
            Kind::Number
        } else if CONTROL_KEYWORDS.contains(&kind) {
            Kind::Control
        } else if OTHER_KEYWORDS.contains(&kind) {
            Kind::Keyword
        } else if kind == TokenKind::Identifier {
            let prev = raw[..i].iter().rev().map(|r| r.0).find(|k| !is_trivia(*k));
            let next = raw[i + 1..].iter().map(|r| r.0).find(|k| !is_trivia(*k));
            if prev == Some(TokenKind::Dot) {
                if next == Some(TokenKind::Lpar) {
                    Kind::Function
                } else {
                    Kind::Variable
                }
            } else {
                classify_name(&semantics, text, range)
            }
        } else {
            Kind::Plain
        };

        tokens.push(HighlightToken {
            kind: token_kind,
            text,
            range,
        });
        last_end = end;
    }

    if last_end < code.len() {
        tokens.push(HighlightToken {
            kind: Kind::Plain,
            text: &code[last_end..],
            range: TextRange::new(at(last_end), at(code.len())),
        });
    }

    tokens
}
