//! What the program does with its names, from one error-tolerant parse, so a
//! name is coloured by what it refers to and not by its spelling.

use std::collections::HashSet;

use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{
    self as ast, ExceptHandler, Expr, ExprContext, MatchCase, PySourceType, Stmt,
};
use ruff_python_parser::parse_unchecked_source;
use ruff_text_size::{Ranged, TextRange};

use super::kinds::Kind;
use super::words::{BUILTIN_FUNCS, BUILTIN_TYPES, TYPING_TYPES};

#[derive(Default)]
pub(super) struct Semantics {
    classes: HashSet<String>,
    functions: HashSet<String>,
    /// Every name the program binds, which shadows the builtin of the same name.
    bound: HashSet<String>,
    imported: HashSet<String>,
    /// Parameter names, with the range of the def or lambda they belong to.
    params: Vec<(TextRange, String)>,
    /// Start offsets where `match`, `case` and `type` really are keywords.
    pub(super) soft_keywords: HashSet<usize>,
}

impl Semantics {
    /// Parses `code`, errors and all, and gathers the names it binds.
    pub(super) fn extract(code: &str) -> Self {
        let parsed = parse_unchecked_source(code, PySourceType::Python);
        let mut semantics = Self::default();
        semantics.visit_body(parsed.suite());
        semantics
    }

    fn add_params(&mut self, parameters: &ast::Parameters, scope: TextRange) {
        for param in parameters.iter() {
            let name = param.name().as_str().to_string();
            self.bound.insert(name.clone());
            self.params.push((scope, name));
        }
    }

    fn is_param(&self, name: &str, at: TextRange) -> bool {
        self.params
            .iter()
            .any(|(scope, param)| param == name && scope.contains_range(at))
    }
}

impl<'a> SourceOrderVisitor<'a> for Semantics {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(f) => {
                self.functions.insert(f.name.as_str().to_string());
                self.bound.insert(f.name.as_str().to_string());
                self.add_params(&f.parameters, f.range());
            }
            Stmt::ClassDef(c) => {
                self.classes.insert(c.name.as_str().to_string());
                self.bound.insert(c.name.as_str().to_string());
            }
            Stmt::Match(m) => {
                self.soft_keywords.insert(m.range().start().to_usize());
            }
            Stmt::TypeAlias(t) => {
                self.soft_keywords.insert(t.range().start().to_usize());
            }
            Stmt::Import(i) => {
                for alias in &i.names {
                    let name = alias
                        .asname
                        .as_ref()
                        .map(|a| a.as_str())
                        .unwrap_or_else(|| alias.name.split('.').next().unwrap_or(""));
                    self.bound.insert(name.to_string());
                    self.imported.insert(name.to_string());
                }
            }
            Stmt::ImportFrom(i) => {
                for alias in &i.names {
                    let name = alias.asname.as_ref().unwrap_or(&alias.name).as_str();
                    self.bound.insert(name.to_string());
                    self.imported.insert(name.to_string());
                }
            }
            _ => {}
        }
        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(n) if matches!(n.ctx, ExprContext::Store | ExprContext::Del) => {
                self.bound.insert(n.id.as_str().to_string());
            }
            Expr::Lambda(l) => {
                if let Some(parameters) = &l.parameters {
                    self.add_params(parameters, l.range());
                }
            }
            _ => {}
        }
        source_order::walk_expr(self, expr);
    }

    fn visit_match_case(&mut self, case: &'a MatchCase) {
        self.soft_keywords.insert(case.range().start().to_usize());
        source_order::walk_match_case(self, case);
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(h) = handler;
        if let Some(name) = &h.name {
            self.bound.insert(name.as_str().to_string());
        }
        source_order::walk_except_handler(self, handler);
    }
}

/// The kind of a name: what the program makes of it, then whether it is a builtin.
pub(super) fn classify_name(semantics: &Semantics, name: &str, range: TextRange) -> Kind {
    let shadowed = semantics.bound.contains(name);
    if semantics.is_param(name, range) {
        Kind::Param
    } else if semantics.classes.contains(name)
        || (!shadowed && BUILTIN_TYPES.contains(&name))
        || (semantics.imported.contains(name) && TYPING_TYPES.contains(&name))
    {
        Kind::Type
    } else if semantics.functions.contains(name) {
        Kind::Function
    } else if !shadowed && BUILTIN_FUNCS.contains(&name) {
        Kind::Builtin
    } else {
        Kind::Variable
    }
}
