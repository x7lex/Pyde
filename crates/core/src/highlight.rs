//! Python syntax colouring. Splits source into tokens and renders them as HTML,
//! with colours coming from the `.tok-*` classes in `main.css`.

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Plain,
    Comment,
    String,
    Number,
    /// Keywords that change the flow of a program: `if`, `return`, `import`...
    Control,
    /// Every other keyword: `def`, `class`, `None`, `and`...
    Keyword,
    Function,
    Type,
    Variable,
}

impl Kind {
    fn class(self) -> Option<&'static str> {
        match self {
            Kind::Plain => None,
            Kind::Comment => Some("tok-comment"),
            Kind::String => Some("tok-string"),
            Kind::Number => Some("tok-number"),
            Kind::Control => Some("tok-control"),
            Kind::Keyword => Some("tok-keyword"),
            Kind::Function => Some("tok-function"),
            Kind::Type => Some("tok-type"),
            Kind::Variable => Some("tok-variable"),
        }
    }
}

const CONTROL: &[&str] = &[
    "assert", "await", "break", "continue", "elif", "else", "except", "finally", "for", "from",
    "if", "import", "pass", "raise", "return", "try", "while", "with", "yield",
];
const KEYWORDS: &[&str] = &[
    "and", "as", "async", "class", "def", "del", "False", "global", "in", "is", "lambda", "None",
    "nonlocal", "not", "or", "True",
];
const TYPES: &[&str] = &[
    "bool",
    "bytearray",
    "bytes",
    "complex",
    "dict",
    "float",
    "frozenset",
    "int",
    "list",
    "object",
    "range",
    "set",
    "str",
    "tuple",
    "type",
];

/// Renders `code` as HTML with one `<span>` per coloured token.
pub fn to_html(code: &str) -> String {
    let mut html = String::with_capacity(code.len() * 2);
    for (kind, text) in tokenize(code) {
        match kind.class() {
            Some(class) => {
                html.push_str("<span class=\"");
                html.push_str(class);
                html.push_str("\">");
                escape_into(&mut html, text);
                html.push_str("</span>");
            }
            None => escape_into(&mut html, text),
        }
    }
    html
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

/// Splits `code` into tokens. Concatenating them always gives back `code`.
fn tokenize(code: &str) -> Vec<(Kind, &str)> {
    let mut tokens: Vec<(Kind, &str)> = Vec::new();
    let mut previous_word = "";
    let mut at = 0;
    while let Some(c) = code[at..].chars().next() {
        let rest = &code[at..];
        let (kind, length) = if c == '#' {
            (Kind::Comment, rest.find('\n').unwrap_or(rest.len()))
        } else if c == '"' || c == '\'' {
            (Kind::String, string_length(rest))
        } else if c.is_ascii_digit() || (c == '.' && starts_with_digit(&rest[1..])) {
            (Kind::Number, number_length(rest))
        } else if is_word_start(c) {
            let length = word_length(rest);
            let word = &rest[..length];
            let after = &rest[length..];
            if is_string_prefix(word) && after.starts_with(['"', '\'']) {
                (Kind::String, length + string_length(after))
            } else {
                let kind = word_kind(word, previous_word, after);
                previous_word = word;
                (kind, length)
            }
        } else if c == '@' && word_length(&rest[1..]) > 0 && is_line_start(&code[..at]) {
            // A decorator: `@` and its name, including dots.
            let name = &rest[1..];
            let end = name
                .find(|c: char| !is_word_char(c) && c != '.')
                .unwrap_or(name.len());
            (Kind::Function, 1 + end)
        } else {
            (Kind::Plain, c.len_utf8())
        };

        // Runs of plain characters join up, so they don't cost a token each.
        let text = &rest[..length];
        match tokens.last_mut() {
            Some((Kind::Plain, joined)) if kind == Kind::Plain => {
                let start = at - joined.len();
                *joined = &code[start..at + length];
            }
            _ => tokens.push((kind, text)),
        }
        at += length;
    }
    tokens
}

fn word_kind(word: &str, previous_word: &str, after: &str) -> Kind {
    if CONTROL.contains(&word) {
        Kind::Control
    } else if KEYWORDS.contains(&word) {
        Kind::Keyword
    } else if previous_word == "class" || TYPES.contains(&word) {
        Kind::Type
    } else if previous_word == "def" || after.trim_start_matches([' ', '\t']).starts_with('(') {
        Kind::Function
    } else {
        Kind::Variable
    }
}

/// Length of the string literal at the start of `text`, which begins with its
/// quote. A single-quoted string with no closing quote stops at the line end, and
/// a triple-quoted one runs to the end of the file.
fn string_length(text: &str) -> usize {
    let quote = &text[..1];
    let triple = quote.repeat(3);
    let (opening, closing, multiline) = if text.starts_with(&triple) {
        (3, triple.as_str(), true)
    } else {
        (1, quote, false)
    };
    let mut chars = text[opening..].char_indices();
    while let Some((i, c)) = chars.next() {
        let here = opening + i;
        if c == '\\' {
            chars.next();
        } else if text[here..].starts_with(closing) {
            return here + closing.len();
        } else if c == '\n' && !multiline {
            return here;
        }
    }
    text.len()
}

fn number_length(text: &str) -> usize {
    let hex = text.starts_with("0x") || text.starts_with("0X");
    let mut previous = '\0';
    for (i, c) in text.char_indices() {
        let exponent_sign = !hex && matches!(c, '+' | '-') && matches!(previous, 'e' | 'E');
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || exponent_sign) {
            return i;
        }
        previous = c;
    }
    text.len()
}

fn word_length(text: &str) -> usize {
    match text.chars().next() {
        Some(first) if is_word_start(first) => {
            text.find(|c| !is_word_char(c)).unwrap_or(text.len())
        }
        _ => 0,
    }
}

fn is_word_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

fn is_string_prefix(word: &str) -> bool {
    word.len() <= 2 && word.chars().all(|c| "rRbBfFuU".contains(c))
}

fn starts_with_digit(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_digit())
}

/// Whether only indentation sits between the last line break and the end of
/// `before`. This tells a decorator from matrix multiplication.
fn is_line_start(before: &str) -> bool {
    let line = before.rsplit('\n').next().unwrap_or("");
    line.chars().all(|c| c == ' ' || c == '\t')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coloured(code: &str) -> Vec<(Kind, &str)> {
        tokenize(code)
            .into_iter()
            .filter(|(kind, _)| *kind != Kind::Plain)
            .collect()
    }

    #[test]
    fn tokens_rebuild_the_source() {
        let code = "é = 'x\n\"\"\"a\nb\"\"\" @ 1.5e-3 # ✓\n\n@a.b\ndef f(): pass\n'\\";
        let rebuilt: String = tokenize(code).into_iter().map(|(_, text)| text).collect();
        assert_eq!(rebuilt, code);
    }

    #[test]
    fn keywords_functions_and_types() {
        assert_eq!(
            coloured("def run(x): return str(x)"),
            vec![
                (Kind::Keyword, "def"),
                (Kind::Function, "run"),
                (Kind::Variable, "x"),
                (Kind::Control, "return"),
                (Kind::Type, "str"),
                (Kind::Variable, "x"),
            ]
        );
        assert_eq!(
            coloured("class Foo: pass"),
            vec![
                (Kind::Keyword, "class"),
                (Kind::Type, "Foo"),
                (Kind::Control, "pass"),
            ]
        );
    }

    #[test]
    fn strings() {
        assert_eq!(coloured(r#"'a#b'"#), vec![(Kind::String, r#"'a#b'"#)]);
        assert_eq!(coloured(r#"'a\'b'"#), vec![(Kind::String, r#"'a\'b'"#)]);
        assert_eq!(coloured("rb'x'"), vec![(Kind::String, "rb'x'")]);
        assert_eq!(coloured("f\"x\""), vec![(Kind::String, "f\"x\"")]);
        assert_eq!(
            coloured("\"\"\"a\n'b\"\"\""),
            vec![(Kind::String, "\"\"\"a\n'b\"\"\"")]
        );
        // Unterminated strings stop at the line end, or the file end if triple.
        assert_eq!(
            coloured("'a\nb"),
            vec![(Kind::String, "'a"), (Kind::Variable, "b")]
        );
        assert_eq!(coloured("'''a\nb"), vec![(Kind::String, "'''a\nb")]);
    }

    #[test]
    fn comments_numbers_and_decorators() {
        assert_eq!(
            coloured("1 # 'not a string'"),
            vec![(Kind::Number, "1"), (Kind::Comment, "# 'not a string'")]
        );
        assert_eq!(coloured("1.5e-3"), vec![(Kind::Number, "1.5e-3")]);
        assert_eq!(
            coloured("0xE-1"),
            vec![(Kind::Number, "0xE"), (Kind::Number, "1")]
        );
        assert_eq!(coloured("  @a.b"), vec![(Kind::Function, "@a.b")]);
        assert_eq!(
            coloured("a @ b"),
            vec![(Kind::Variable, "a"), (Kind::Variable, "b")]
        );
    }

    #[test]
    fn html_is_escaped() {
        assert_eq!(
            to_html("a<b # <&>"),
            "<span class=\"tok-variable\">a</span>&lt;<span class=\"tok-variable\">b</span> \
             <span class=\"tok-comment\"># &lt;&amp;&gt;</span>"
        );
    }
}
