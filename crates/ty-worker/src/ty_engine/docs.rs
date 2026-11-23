//! Hover text: docstrings cleaned like `inspect.cleandoc`, and what each keyword
//! does.

use ruff_python_ast::{Expr, Stmt};

fn leading_whitespace(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// Same algorithm as Python's `inspect.cleandoc`. Indentation is counted and
/// removed in characters, so multi-byte whitespace can't split a character.
pub(super) fn clean_docstring(raw: &str) -> String {
    let expanded = raw.replace('\t', "    ");
    let all: Vec<&str> = expanded.lines().collect();
    let Some(first) = all.iter().position(|l| !l.trim().is_empty()) else {
        return String::new();
    };
    let last = all
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .unwrap_or(first);
    let lines = &all[first..=last];

    let min_indent = lines
        .iter()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| leading_whitespace(l))
        .min()
        .unwrap_or(0);

    let mut out: Vec<String> = Vec::new();
    let mut prev_empty = false;
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            if !prev_empty && !out.is_empty() {
                out.push(String::new());
                prev_empty = true;
            }
            continue;
        }
        let dedented: String = if i == 0 {
            line.trim_start().to_string()
        } else {
            line.chars().skip(min_indent).collect()
        };
        out.push(dedented.trim_end().to_string());
        prev_empty = false;
    }
    out.join("\n")
}

pub(super) fn keyword_doc(word: &str) -> Option<&'static str> {
    Some(match word {
        "def" => {
            "Defines a function or method.\n\ndef function_name(parameters) -> return_type:\n    body"
        }
        "class" => "Defines a user-defined class.\n\nclass ClassName(BaseClass):\n    body",
        "return" => {
            "Leaves the current function call with the expression value (or None) as return value."
        }
        "import" => "Imports modules, packages, or specific attributes into the current namespace.",
        "from" => "Specifies the module from which to import specific attributes or functions.",
        "if" => {
            "Conditionally executes a block of statements based on whether a condition evaluates to True."
        }
        "elif" => {
            "Short for 'else if', conditionally executes if preceding if/elif branches were False."
        }
        "else" => {
            "Executes if preceding conditional branches evaluated to False, or if no exception was raised in a try statement."
        }
        "for" => "Iterates over items of an iterable sequence (such as a list, tuple, or range).",
        "while" => {
            "Repeatedly executes a block of statements as long as a condition expression remains True."
        }
        "try" => "Encloses a block of statements to catch and handle runtime exceptions.",
        "except" => {
            "Catches and handles one or more specific exception types raised in a try block."
        }
        "finally" => {
            "Specifies clean-up actions that are guaranteed to execute before leaving a try block."
        }
        "with" => "Wraps the execution of a block with methods defined by a context manager.",
        "as" => "Binds an import, context manager or exception to a local variable name.",
        "pass" => {
            "A null statement that does nothing; serves as a placeholder where code is syntactically required."
        }
        "break" => "Breaks out of the innermost enclosing for or while loop.",
        "continue" => {
            "Skips the remainder of the current loop iteration and advances to the next cycle."
        }
        "raise" => "Raises an exception explicitly to indicate an error condition.",
        "assert" => {
            "Asserts that an expression is True; raises AssertionError if the condition is false."
        }
        "yield" => {
            "Yields a value from a generator function and temporarily suspends its execution."
        }
        "async" => "Declares an asynchronous function, loop or context manager.",
        "await" => "Yields execution to the event loop until an awaitable object resolves.",
        "lambda" => "Defines an anonymous inline lambda function.",
        "in" => {
            "Membership operator testing whether a value exists within a sequence or collection."
        }
        "is" => {
            "Identity operator testing whether two expressions refer to the exact same object in memory."
        }
        "not" => "Boolean logical negation operator.",
        "and" => "Short-circuiting boolean logical AND operator.",
        "or" => "Short-circuiting boolean logical OR operator.",
        "del" => "Deletes a name binding, an item of a container, or an attribute.",
        "global" => {
            "Declares that a name refers to the module-level variable instead of a local one."
        }
        "nonlocal" => {
            "Declares that a name refers to a variable in the nearest enclosing function scope."
        }
        "match" => {
            "Starts a structural pattern matching statement, comparing a value against `case` patterns."
        }
        "case" => "A pattern branch of a `match` statement.",
        "type" => "Declares a type alias.\n\ntype Alias = existing_type",
        _ => return None,
    })
}

/// The docstring at the start of `body`, cleaned up for display.
pub(super) fn docstring_of(body: &[Stmt]) -> Option<String> {
    match body.first()? {
        Stmt::Expr(e) => match &*e.value {
            Expr::StringLiteral(sl) => Some(clean_docstring(sl.value.to_str())),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_docstring_dedent() {
        let raw = r#"
        Summary line.

        param1
            description of param1.
        param2
            description of param2.
        "#;
        let cleaned = clean_docstring(raw);
        assert_eq!(
            cleaned,
            "Summary line.\n\nparam1\n    description of param1.\nparam2\n    description of param2."
        );
    }

    #[test]
    fn clean_docstring_multibyte_whitespace_does_not_panic() {
        assert_eq!(
            clean_docstring("Sum\n\u{a0}\u{a0}a\n\u{a0}\u{a0}b"),
            "Sum\na\nb"
        );
        let _ = clean_docstring("Sum\n  a\n\u{3000}b");
    }
}
