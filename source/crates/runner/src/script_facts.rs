use campfire_capabilities::CtxKind;
use campfire_script::rhai::{AST, ASTNode, Expr, FnCallExpr};

/// What the package load checks read from a script: its functions, the names it uses on `ctx`,
/// and the string literals it gives the calls that take a name.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ScriptFacts {
    /// Sorted by name, then by parameter count.
    pub(crate) functions: Vec<Function>,
    pub(crate) ctx_names: Vec<CtxUse>,
    /// Each `ctx.p.<name>` it reads.
    pub(crate) params: Vec<String>,
    /// The ids `ctx.add_modifier` and `unit.has_modifier` take.
    pub(crate) modifiers: Vec<String>,
    /// The names `unit.stat` takes.
    pub(crate) stats: Vec<String>,
    /// The filters the queries take.
    pub(crate) filters: Vec<String>,
    /// The kinds `ctx.damage` takes.
    pub(crate) damage_kinds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) params: usize,
}

/// A name used on `ctx`, as a value or as a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CtxUse {
    pub(crate) name: String,
    pub(crate) kind: CtxKind,
}

impl ScriptFacts {
    /// The facts of `ast`. `ctx` is the variable of that name, as every hook's first parameter
    /// is by convention and every helper passes it on.
    pub(crate) fn read(ast: &AST) -> ScriptFacts {
        let mut facts = ScriptFacts {
            functions: ast
                .iter_functions()
                .map(|function| Function {
                    name: function.name.to_owned(),
                    params: function.params.len(),
                })
                .collect(),
            ..ScriptFacts::default()
        };
        facts
            .functions
            .sort_unstable_by(|a, b| (&a.name, a.params).cmp(&(&b.name, b.params)));
        ast.walk(&mut |path: &[ASTNode<'_>]| {
            if let Some(ASTNode::Expr(Expr::Dot(dot, ..))) = path.last() {
                if variable(&dot.lhs) == Some("ctx") {
                    facts.read_ctx(&dot.rhs);
                }
                if let Expr::MethodCall(call, _) = &dot.rhs {
                    facts.read_method(call);
                }
            }
            true
        });
        facts
    }

    /// What `ctx.<rhs>` uses.
    fn read_ctx(&mut self, rhs: &Expr) {
        match rhs {
            Expr::MethodCall(call, _) => {
                self.ctx_names.push(CtxUse {
                    name: call.name.to_string(),
                    kind: CtxKind::Call,
                });
                let literal = |at: usize| call.args.get(at).and_then(string);
                let (list, at) = match call.name.as_str() {
                    "add_modifier" => (&mut self.modifiers, 1),
                    "find" | "find_visible" => (&mut self.filters, 3),
                    "nearest_visible" => (&mut self.filters, 2),
                    "damage" => (&mut self.damage_kinds, 2),
                    _ => return,
                };
                list.extend(literal(at));
            }
            Expr::Dot(inner, ..) | Expr::Index(inner, ..) => {
                let Some(name) = property(&inner.lhs) else {
                    return;
                };
                self.value(name);
                if name == "p" {
                    let param = match &inner.rhs {
                        Expr::Dot(next, ..) | Expr::Index(next, ..) => property(&next.lhs),
                        next => property(next),
                    };
                    self.params.extend(param.map(str::to_owned));
                }
            }
            _ => self.value(property(rhs).unwrap_or_default()),
        }
    }

    /// What a method called on any value uses: `has_modifier` and `stat` take names.
    fn read_method(&mut self, call: &FnCallExpr) {
        let list = match call.name.as_str() {
            "has_modifier" => &mut self.modifiers,
            "stat" => &mut self.stats,
            _ => return,
        };
        list.extend(call.args.first().and_then(string));
    }

    fn value(&mut self, name: &str) {
        if !name.is_empty() {
            self.ctx_names.push(CtxUse {
                name: name.to_owned(),
                kind: CtxKind::Value,
            });
        }
    }
}

/// The name of a variable.
fn variable(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Variable(variable, ..) => Some(&variable.1),
        _ => None,
    }
}

/// The name of a property access.
fn property(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Property(property, _) => Some(&property.2),
        _ => None,
    }
}

fn string(expr: &Expr) -> Option<String> {
    match expr {
        Expr::StringConstant(text, _) => Some(text.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use campfire_script::ScriptHost;

    use super::*;

    #[test]
    fn the_facts_are_the_names_a_script_uses_on_ctx_and_gives_the_api() {
        let source = r#"
fn on_cast(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies:hero") {
        ctx.damage(unit, ctx.p.damage * unit.stat("armor"), "magic");
        if !unit.has_modifier("kindle") {
            ctx.add_modifier(unit, "kindle", 100);
        }
    }
    let first = ctx.teams[0];
    ctx.state.phase = ctx.nearest_visible(caster, 5, name);
    helper(ctx, caster.params.gold);
}

fn helper(ctx, gold) {}
"#;
        let ast = ScriptHost::new(1000).parse(source).unwrap();
        let facts = ScriptFacts::read(&ast);
        let function = |name: &str, params| Function {
            name: name.to_owned(),
            params,
        };
        assert_eq!(
            facts.functions,
            [function("helper", 2), function("on_cast", 3)]
        );
        let names: Vec<_> = facts
            .ctx_names
            .iter()
            .map(|used| (used.name.as_str(), used.kind))
            .collect();
        let (call, value) = (CtxKind::Call, CtxKind::Value);
        assert_eq!(
            names,
            [
                ("find", call),
                ("p", value),
                ("damage", call),
                ("p", value),
                ("add_modifier", call),
                ("teams", value),
                ("state", value),
                ("nearest_visible", call),
            ]
        );
        assert_eq!(facts.params, ["radius", "damage"]);
        // A literal counts; a variable, as the filter `name`, cannot be read at load.
        assert_eq!(facts.modifiers, ["kindle", "kindle"]);
        assert_eq!(facts.stats, ["armor"]);
        assert_eq!(facts.filters, ["enemies:hero"]);
        assert_eq!(facts.damage_kinds, ["magic"]);
    }
}
