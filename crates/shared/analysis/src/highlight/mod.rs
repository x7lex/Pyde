//! Python syntax colouring from Ruff's lexer and AST, rendered as HTML with
//! classes styled in `main.css`, and optionally marked with diagnostics and dead
//! code.

mod guides;
mod html;
mod kinds;
mod semantics;
mod tokens;
mod words;

pub use guides::INDENT;
pub use html::to_html;
pub use kinds::Kind;
pub use tokens::{HighlightToken, tokenize};
pub use words::BUILTIN_TYPES;
