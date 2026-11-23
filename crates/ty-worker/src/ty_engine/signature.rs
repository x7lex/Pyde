//! Hover headlines: Ty's printed types turned into signatures that read like
//! source.

/// Splits on commas that are outside brackets and quotes.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts.retain(|p| !p.is_empty());
    parts
}

/// Overloads past this many are summarised.
const MAX_SHOWN_OVERLOADS: usize = 3;

/// Writes a bare signature as a `def` of `name`.
fn as_def(name: &str, signature: &str) -> String {
    if signature.starts_with("def ") || signature.starts_with("async def ") {
        signature.to_string()
    } else if signature.starts_with('(') {
        format!("def {name}{signature}")
    } else {
        signature.to_string()
    }
}

/// Turns the type text Ty prints (`def f(x: int) -> int`, `<class 'Foo'>`,
/// `<module 'math'>`, `bound method Foo.m(..)`, `Overload[..]`) into a hover
/// headline. `stdlib` marks symbols defined in the vendored typeshed.
pub(super) fn format_hover_signature(name: &str, ty: Option<&str>, stdlib: bool) -> String {
    let Some(t) = ty else {
        return format!("(variable) {name}");
    };
    let function = if stdlib {
        "(built-in function)"
    } else {
        "(function)"
    };

    if let Some(module) = t
        .strip_prefix("<module '")
        .and_then(|r| r.strip_suffix("'>"))
    {
        return format!("(module) {module}");
    }
    if let Some(class) = t
        .strip_prefix("<class '")
        .and_then(|r| r.strip_suffix("'>"))
    {
        return format!("class {class}");
    }
    if let Some(rest) = t.strip_prefix("bound method ")
        && let Some(open) = rest.find('(')
    {
        return format!("(method) def {name}{}", &rest[open..]);
    }
    if let Some(inner) = t
        .strip_prefix("Overload[")
        .and_then(|r| r.strip_suffix(']'))
    {
        let parts = split_top_level(inner);
        let mut lines: Vec<String> = parts
            .iter()
            .take(MAX_SHOWN_OVERLOADS)
            .map(|p| as_def(name, p))
            .collect();
        if parts.len() > MAX_SHOWN_OVERLOADS {
            lines.push(format!(
                "# +{} more overloads",
                parts.len() - MAX_SHOWN_OVERLOADS
            ));
        }
        return format!("{function} {}", lines.join("\n"));
    }
    if t.starts_with("def ") || t.starts_with("async def ") {
        return format!("{function} {t}");
    }
    format!("(variable) {name}: {t}")
}

/// Ty shows an `async def` as an ordinary def returning a coroutine. Source
/// reads better: `async def f() -> R`.
pub(super) fn present_async(signature: &str) -> String {
    let signature = signature.replacen(" def ", " async def ", 1);
    let Some(at) = signature.rfind(" -> CoroutineType[Any, Any, ") else {
        return signature;
    };
    let head = &signature[..at];
    let result = signature[at + " -> CoroutineType[Any, Any, ".len()..].strip_suffix(']');
    match result {
        Some("Unknown") | None => head.to_string(),
        Some(result) => format!("{head} -> {result}"),
    }
}
