use ruff_python_ast::visitor::{walk_stmt, Visitor};
use ruff_python_ast::{self, Expr, ExprAttribute, ExprName, Stmt, StmtIf};
use ruff_text_size::TextRange;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct ImportVisitor {
    imports: HashMap<String, Vec<TextRange>>,
    import_module_names: HashSet<String>,
}

impl ImportVisitor {
    pub fn new() -> Self {
        Self {
            imports: HashMap::new(),
            import_module_names: HashSet::new(),
        }
    }

    pub fn get_imports(self) -> HashMap<String, Vec<TextRange>> {
        self.imports
    }
}

impl<'a> Visitor<'a> for ImportVisitor {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(import_stmt) => {
                for alias in &import_stmt.names {
                    let top_level_module = get_top_level_module_name(alias.name.as_str());
                    self.imports
                        .entry(top_level_module)
                        .or_default()
                        .push(alias.range);

                    if alias.name.as_str() == "importlib" {
                        let name = alias
                            .asname
                            .as_ref()
                            .map(|id| id.as_str())
                            .unwrap_or("importlib");
                        self.import_module_names
                            .insert(format!("{}.import_module", name));
                    }
                }
            }
            Stmt::ImportFrom(import_from_stmt) => {
                if let Some(module) = &import_from_stmt.module {
                    if import_from_stmt.level == 0 {
                        let module_name = module.as_str();
                        self.imports
                            .entry(get_top_level_module_name(module_name))
                            .or_default()
                            .push(import_from_stmt.range);

                        if module_name == "importlib" {
                            for alias in &import_from_stmt.names {
                                if alias.name.as_str() == "import_module" {
                                    let name = alias
                                        .asname
                                        .as_ref()
                                        .map(|id| id.as_str())
                                        .unwrap_or("import_module");
                                    self.import_module_names.insert(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Stmt::Expr(expr_stmt) => {
                if let Expr::Call(call_expr) = expr_stmt.value.as_ref() {
                    let is_import_module = match call_expr.func.as_ref() {
                        Expr::Attribute(attr_expr) => {
                            if let Expr::Name(name) = attr_expr.value.as_ref() {
                                self.import_module_names
                                    .contains(&format!("{}.import_module", name.id.as_str()))
                            } else {
                                false
                            }
                        }
                        Expr::Name(name) => self.import_module_names.contains(name.id.as_str()),
                        _ => false,
                    };

                    if is_import_module {
                        if let Some(Expr::StringLiteral(string_literal)) =
                            call_expr.arguments.args.first()
                        {
                            let top_level_module =
                                get_top_level_module_name(&string_literal.value.to_string());
                            self.imports
                                .entry(top_level_module)
                                .or_default()
                                .push(expr_stmt.range);
                        }
                    }
                }
            }
            Stmt::If(if_stmt) if is_guarded_by_type_checking(if_stmt) => {
                // Avoid parsing imports that are only evaluated by type checkers.
            }
            _ => walk_stmt(self, stmt), // Delegate other statements to walk_stmt
        }
    }
}

/// Extracts the top-level module name from a potentially nested module path.
/// e.g. when a `module_name` is `foo.bar`, this returns `foo`.
fn get_top_level_module_name(module_name: &str) -> String {
    module_name
        .split('.')
        .next()
        .unwrap_or(module_name)
        .to_owned()
}

/// Checks if we are in a block guarded by `typing.TYPE_CHECKING`.
fn is_guarded_by_type_checking(if_stmt: &StmtIf) -> bool {
    match &if_stmt.test.as_ref() {
        Expr::Attribute(ExprAttribute { value, attr, .. }) => {
            if let Expr::Name(ExprName { id, .. }) = value.as_ref() {
                if id.as_str() == "typing" && attr.as_str() == "TYPE_CHECKING" {
                    return true;
                }
            }
        }
        Expr::Name(ExprName { id, .. }) => {
            if id == "TYPE_CHECKING" {
                return true;
            }
        }
        _ => (),
    }

    false
}
