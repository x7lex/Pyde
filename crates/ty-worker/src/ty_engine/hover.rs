//! Hover information for the symbol at an offset, with keyword docs as the
//! fallback.

use pyde_analysis::protocol::HoverInfo;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::find_node::covering_node;
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_core::definition::DefinitionKind;
use ty_python_semantic::types::ide_support::{
    ImportAliasResolution, ResolvedDefinition, definitions_for_attribute, definitions_for_name,
    inlay_hint_return_type,
};
use ty_python_semantic::{Db as SemanticDb, HasType, SemanticModel};

use super::TyEngine;
use super::docs::{clean_docstring, docstring_of, keyword_doc};
use super::signature::{format_hover_signature, present_async};

/// Hover for tokens that Ty has no node for: keywords, plus builtin names
/// when the file is too broken for Ty to answer.
fn keyword_or_token_hover(code: &str, offset: usize) -> Option<HoverInfo> {
    use pyde_analysis::highlight::Kind;

    let tokens = pyde_analysis::highlight::tokenize(code);
    let target = TextSize::try_from(offset).ok()?;
    let token = tokens.iter().find(|t| t.range.contains(target))?;
    let start = token.range.start().to_usize();
    let end = token.range.end().to_usize();
    let text = token.text;
    let info = |signature: String, docstring: Option<String>| HoverInfo {
        signature,
        docstring,
        start_byte: start,
        end_byte: end,
    };

    match token.kind {
        Kind::Control | Kind::Keyword => {
            let doc = keyword_doc(text)?;
            Some(info(format!("(keyword) {text}"), Some(doc.to_string())))
        }
        Kind::Builtin => Some(info(format!("(built-in function) {text}"), None)),
        Kind::Type if pyde_analysis::highlight::BUILTIN_TYPES.contains(&text) => {
            Some(info(format!("class {text}"), None))
        }
        _ => None,
    }
}

fn text_of<'a>(source: &'a str, node: &impl Ranged) -> &'a str {
    source.get(node.range().to_std_range()).unwrap_or("")
}

impl TyEngine {
    fn is_stdlib(&self, defs: &[ResolvedDefinition<'_>]) -> bool {
        defs.first().is_some_and(|d| {
            d.focus_range(&self.db)
                .file()
                .path(&self.db)
                .is_vendored_path()
        })
    }

    fn is_parameter(&self, defs: &[ResolvedDefinition<'_>]) -> bool {
        defs.first().and_then(|d| d.definition()).is_some_and(|d| {
            matches!(
                d.kind(&self.db),
                DefinitionKind::Parameter(_) | DefinitionKind::LambdaParameter(_)
            )
        })
    }

    /// Hover information for the symbol at byte `offset` of the last checked
    /// file, or `None` if there is none.
    pub fn hover(&self, offset: usize) -> Option<HoverInfo> {
        let file = self.current_file?;
        let program_file = self.db.program_file(file);
        let python_file = program_file.python_file(&self.db);
        let model = SemanticModel::new(&self.db, program_file);
        let env = model.program_environment();
        let parsed = ruff_db::parsed::parsed_module(&self.db, python_file);
        let parsed_ref = parsed.load(&self.db);
        let target_size = TextSize::try_from(offset).ok()?;

        let src = ruff_db::source::source_text(&self.db, file);
        let full_text = src.as_str();

        let covering = covering_node(
            AnyNodeRef::from(parsed_ref.syntax()),
            TextRange::empty(target_size),
        );

        let info = |signature: String, docstring: Option<String>, range: TextRange| HoverInfo {
            signature,
            docstring,
            start_byte: range.start().to_usize(),
            end_byte: range.end().to_usize(),
        };

        let mut ancestors = covering.ancestors();
        while let Some(node) = ancestors.next() {
            match node {
                AnyNodeRef::Parameter(param) => {
                    let ty = param
                        .inferred_type(&model)
                        .map(|t| t.display(&self.db, &env).to_string());
                    let sig = if let Some(ann) = &param.annotation {
                        format!(
                            "(parameter) {}: {}",
                            param.name.as_str(),
                            text_of(full_text, &**ann)
                        )
                    } else if let Some(ty) = ty {
                        format!("(parameter) {}: {ty}", param.name.as_str())
                    } else {
                        format!("(parameter) {}", param.name.as_str())
                    };
                    return Some(info(sig, None, param.range()));
                }
                AnyNodeRef::Alias(alias) => {
                    let ty = alias
                        .inferred_type(&model)
                        .map(|t| t.display(&self.db, &env).to_string());
                    let shown = alias.asname.as_ref().unwrap_or(&alias.name);
                    let stdlib = false;
                    let sig = format_hover_signature(shown.as_str(), ty.as_deref(), stdlib);
                    return Some(info(sig, None, alias.range()));
                }
                AnyNodeRef::ExprName(name) => {
                    let defs = definitions_for_name(
                        &model,
                        name.id.as_str(),
                        name.into(),
                        ImportAliasResolution::ResolveAliases,
                    );
                    let doc = defs
                        .iter()
                        .find_map(|d| d.docstring(&self.db))
                        .map(|s| clean_docstring(&s));
                    let ty = name
                        .inferred_type(&model)
                        .map(|t| t.display(&self.db, &env).to_string());

                    let sig = if self.is_parameter(&defs) {
                        match &ty {
                            Some(t) => format!("(parameter) {}: {t}", name.id),
                            None => format!("(parameter) {}", name.id),
                        }
                    } else {
                        format_hover_signature(
                            name.id.as_str(),
                            ty.as_deref(),
                            self.is_stdlib(&defs),
                        )
                    };
                    return Some(info(sig, doc, name.range()));
                }
                AnyNodeRef::ExprAttribute(attr) => {
                    if attr.attr.range().contains(target_size) {
                        let defs = definitions_for_attribute(&model, attr);
                        let doc = defs
                            .iter()
                            .find_map(|d| d.docstring(&self.db))
                            .map(|s| clean_docstring(&s));
                        let ty = attr
                            .inferred_type(&model)
                            .map(|t| t.display(&self.db, &env).to_string());
                        let sig = format_hover_signature(
                            attr.attr.as_str(),
                            ty.as_deref(),
                            self.is_stdlib(&defs),
                        );
                        return Some(info(sig, doc, attr.attr.range()));
                    }
                }
                AnyNodeRef::StmtFunctionDef(func) => {
                    if func.name.range().contains(target_size) {
                        let in_class =
                            matches!(ancestors.next(), Some(AnyNodeRef::StmtClassDef(_)));
                        let ty = func
                            .inferred_type(&model)
                            .map(|t| t.display(&self.db, &env).to_string());

                        let mut sig =
                            format_hover_signature(func.name.as_str(), ty.as_deref(), false);
                        // Ty prints `-> Unknown`, or nothing, when it can't see an unannotated
                        // return.
                        if func.returns.is_none()
                            && let Some(ret) = inlay_hint_return_type(&model, func)
                        {
                            let ret = ret.display(&self.db, &env).to_string();
                            let trimmed = sig.strip_suffix(" -> Unknown").unwrap_or(&sig);
                            sig = if trimmed.contains(" -> ") {
                                trimmed.to_string()
                            } else {
                                format!("{trimmed} -> {ret}")
                            };
                        }
                        if func.is_async {
                            sig = present_async(&sig);
                        }
                        if in_class {
                            sig = sig.replacen("(function)", "(method)", 1);
                        }
                        return Some(info(sig, docstring_of(&func.body), func.name.range()));
                    }
                }
                AnyNodeRef::StmtClassDef(class) => {
                    if class.name.range().contains(target_size) {
                        let bases = class
                            .arguments
                            .as_ref()
                            .map(|a| text_of(full_text, &**a))
                            .unwrap_or("");
                        let sig = format!("class {}{}", class.name.as_str(), bases);
                        return Some(info(sig, docstring_of(&class.body), class.name.range()));
                    }
                }
                AnyNodeRef::ExprBooleanLiteral(b) => {
                    let snippet = text_of(full_text, b);
                    return Some(info(
                        format!("(literal) {snippet}: bool"),
                        Some("The boolean value.".to_string()),
                        b.range(),
                    ));
                }
                AnyNodeRef::ExprNoneLiteral(n) => {
                    return Some(info(
                        "(literal) None: None".to_string(),
                        Some(
                            "The sole value of NoneType, representing the absence of a value."
                                .to_string(),
                        ),
                        n.range(),
                    ));
                }
                _ => {}
            }
        }

        keyword_or_token_hover(full_text, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_inferred_type() {
        let mut engine = TyEngine::new();
        let code = r#"
def calculate(x: int):
    return x * 2
"#;
        let _ = engine.check("test.py", code);
        // Offset 5 is the `c` of `calculate`.
        let hover = engine.hover(5);
        assert!(hover.is_some(), "Hover on calculate should return info");
        let h = hover.unwrap();
        assert!(h.signature.contains("calculate"));
        assert!(h.signature.contains("int"));
    }

    #[test]
    fn hover_builtin_print() {
        let mut engine = TyEngine::new();
        let code = r#"print("Hello, Pyde!")"#;
        let _ = engine.check("test.py", code);
        // Offset 2 is the `i` of `print`.
        let hover = engine.hover(2);
        assert!(hover.is_some(), "Hover on print should return info");
        let h = hover.unwrap();
        assert!(h.signature.contains("(built-in function) def print"));
        assert!(h.signature.contains("*values: object"));
        assert!(!h.signature.contains("Overload["));
        assert!(h.docstring.is_some());
        assert!(
            h.docstring
                .unwrap()
                .contains("Prints the values to a stream")
        );
    }

    #[test]
    fn hover_literal_string_and_keywords() {
        let mut engine = TyEngine::new();
        let code = "def foo():\n    return \"asdasd\"\nx = 42\n";
        let _ = engine.check("test.py", code);

        // Offset 1 is in `def`.
        let h_def = engine.hover(1).expect("Hover on def keyword");
        assert!(h_def.signature.contains("(keyword) def"));
        assert!(h_def.docstring.is_some());

        // Offset 5 is in `foo`.
        let h_foo = engine.hover(5).expect("Hover on function name");
        assert!(h_foo.signature.contains("def foo"));

        // Offset 17 is in `return`.
        let h_ret = engine.hover(17).expect("Hover on return keyword");
        assert!(h_ret.signature.contains("(keyword) return"));
    }

    #[test]
    fn hover_on_syntax_error_incomplete_code() {
        let mut engine = TyEngine::new();
        let code = "print(\"asdasd\"";
        let (diags, _) = engine.check("test.py", code);

        // The diagnostic spans the unclosed string.
        assert!(!diags.is_empty());
        let diag = &diags[0];
        assert_eq!(diag.rule, "invalid-syntax");
        assert!(
            diag.start_byte <= 6 && diag.end_byte >= 14,
            "Diag should span the unclosed token"
        );

        // Offset 2 is in `print`.
        let h_print = engine.hover(2).expect("Hover on print");
        assert!(h_print.signature.contains("(built-in function) def print"));
    }

    const SAMPLE: &str = "import math\nfrom typing import Optional\n\nclass Foo:\n    \"\"\"Doc.\n\n        indented more\n    \"\"\"\n    n: int = 1\n\n    def m(self, a, /, b, *args, c=1, **kw) -> None:\n        return None\n\ndef calculate(x: int) -> int:\n    \"\"\"Calc.\n\n    More.\n    \"\"\"\n    return x * 2\n\nasync def af(q: Optional[int] = None): pass\n\nsum = 3\nfoo = Foo()\nprint(calculate(5), math.pi, sum, foo.m, math.sin(1))\n";

    fn hover_at(code: &str, needle: &str, delta: usize) -> HoverInfo {
        let mut engine = TyEngine::new();
        engine.check("main.py", code);
        let offset = code.find(needle).expect("needle in code") + delta;
        engine
            .hover(offset)
            .unwrap_or_else(|| panic!("no hover for `{needle}`"))
    }

    fn hover_sample(needle: &str, delta: usize) -> HoverInfo {
        hover_at(SAMPLE, needle, delta)
    }

    #[test]
    fn hover_function_usage_is_a_function() {
        let h = hover_sample("calculate(5)", 1);
        assert_eq!(h.signature, "(function) def calculate(x: int) -> int");
        assert_eq!(h.docstring.as_deref(), Some("Calc.\n\nMore."));
    }

    #[test]
    fn hover_class_and_module_usage() {
        assert_eq!(hover_sample("Foo()", 1).signature, "class Foo");
        assert_eq!(hover_sample("math.pi", 1).signature, "(module) math");
        assert_eq!(hover_sample("import math", 8).signature, "(module) math");
    }

    #[test]
    fn hover_attribute_functions_and_methods() {
        let sin = hover_sample("sin(1)", 1).signature;
        assert!(sin.starts_with("(built-in function) def sin("), "{sin}");
        let m = hover_sample("foo.m", 4).signature;
        assert!(
            m.starts_with("(method) def m(a, /, b, *args, c=1, **kw)"),
            "{m}"
        );
    }

    #[test]
    fn hover_definition_keeps_separators_defaults_and_docs() {
        let h = hover_sample("m(self", 0);
        assert!(
            h.signature
                .starts_with("(method) def m(self, a, /, b, *args, c=1, **kw)"),
            "{}",
            h.signature
        );
        let h = hover_sample("Foo:", 0);
        assert_eq!(h.signature, "class Foo");
        assert_eq!(h.docstring.as_deref(), Some("Doc.\n\nindented more"));
        let h = hover_sample("calculate(x", 0);
        assert_eq!(h.docstring.as_deref(), Some("Calc.\n\nMore."));
        let h = hover_sample("af(", 0);
        assert!(
            h.signature.contains("q: int | None = None"),
            "{}",
            h.signature
        );
        assert!(
            h.signature.starts_with("(function) async def af("),
            "{}",
            h.signature
        );
        assert!(!h.signature.contains("CoroutineType"), "{}", h.signature);
    }

    #[test]
    fn hover_shadowed_builtin_is_a_variable() {
        let h = hover_sample("sum, foo", 0);
        assert!(h.signature.starts_with("(variable) sum"), "{}", h.signature);
        assert!(!h.signature.contains("built-in"));
    }

    #[test]
    fn hover_parameter_usage() {
        let h = hover_sample("x * 2", 0);
        assert_eq!(h.signature, "(parameter) x: int");
    }

    #[test]
    fn hover_soft_keyword_variable() {
        let code = "match = 1\nprint(match)\n";
        let h = hover_at(code, "match)", 0);
        assert!(
            h.signature.starts_with("(variable) match"),
            "{}",
            h.signature
        );
        let code = "match 1:\n    case 1:\n        pass\n";
        assert_eq!(hover_at(code, "match", 1).signature, "(keyword) match");
    }
}
