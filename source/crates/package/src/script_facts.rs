use std::ptr;

use campfire_capabilities::{ApiOwner, MemberKind, NameArgs, NameKind, ScriptApi};
use campfire_script::rhai::{AST, ASTNode, Expr, FnCallExpr, Stmt};

use crate::error::{CtxMisuse, LoadProblem, Place};

/// The variable every script API call goes through, by design 08's convention.
const CTX: &str = "ctx";
/// The function pointer calls, through which a value reaches a function under any name.
const POINTER_CALLS: [&str; 2] = ["call", "curry"];

/// What the package load checks read from a script: its functions, the names it uses on `ctx`,
/// and the string literals it gives the arguments the registry marks as names.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ScriptFacts {
    /// Sorted by name, then by parameter count.
    pub(crate) functions: Vec<Function>,
    pub(crate) ctx_names: Vec<CtxUse>,
    /// Each `ctx.p.<name>` it reads.
    pub(crate) params: Vec<String>,
    /// Each literal it gives an argument the registry marks as a name, in the script's order.
    pub(crate) names: Vec<ScriptName>,
    /// Each field or method it reads on a value other than `ctx`, but the names after `p`,
    /// `state` and `params`, which name data; and the keys of its object-map literals.
    pub(crate) members: Vec<MemberUse>,
    pub(crate) map_keys: Vec<String>,
    /// The first use of `ctx` that breaks the convention, if any.
    pub(crate) ctx_misuse: Option<CtxMisuse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) params: usize,
    /// Whether its first parameter is named `ctx`.
    pub(crate) ctx_first: bool,
}

/// A name used on `ctx`, as a value or as a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CtxUse {
    pub(crate) name: String,
    pub(crate) kind: MemberKind,
}

/// A literal a script gives an argument that names something of `kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptName {
    pub(crate) kind: NameKind,
    pub(crate) name: String,
}

impl ScriptName {
    /// The problem of it, which nothing of its kind has, in a script at `at`.
    pub(crate) fn unknown(&self, at: &Place) -> LoadProblem {
        LoadProblem::Unknown {
            of: self.kind,
            at: at.clone(),
            name: self.name.clone(),
        }
    }
}

/// A name read on a value, as a field or as a method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberUse {
    pub(crate) name: String,
    pub(crate) kind: MemberKind,
}

/// The names after which a property names data, not a member: `ctx.p.<param>`,
/// `ctx.state.<field>`, `m.state.<field>`, `unit.params.<param>`.
const DATA_ACCESSORS: [&str; 3] = ["p", "state", "params"];

impl ScriptFacts {
    /// The facts of `ast`. `ctx` is the variable of that name, as every hook's first parameter
    /// is by convention and every helper passes it on; a use that breaks the convention is
    /// recorded, so the facts see every use of `ctx` in a script that keeps it.
    pub(crate) fn read(ast: &AST, api: &ScriptApi) -> ScriptFacts {
        let mut facts = ScriptFacts {
            functions: ast
                .iter_functions()
                .map(|function| Function {
                    name: function.name.to_owned(),
                    params: function.params.len(),
                    ctx_first: function.params.first() == Some(&CTX),
                })
                .collect(),
            ..ScriptFacts::default()
        };
        facts
            .functions
            .sort_unstable_by(|a, b| (&a.name, a.params).cmp(&(&b.name, b.params)));
        ast.walk(&mut |path: &[ASTNode<'_>]| {
            match path.last() {
                Some(ASTNode::Expr(Expr::Map(map, _))) => {
                    let keys = map.0.iter().map(|(key, _)| key.name.to_string());
                    facts.map_keys.extend(keys);
                }
                Some(ASTNode::Expr(Expr::Dot(dot, ..))) => {
                    let link = matches!(dot.lhs, Expr::Property(..) | Expr::MethodCall(..));
                    let on_ctx = !link && variable(&dot.lhs) == Some(CTX);
                    if !link {
                        if on_ctx {
                            facts.read_ctx(&dot.rhs, api);
                        }
                        facts.read_chain(&dot.rhs, on_ctx, false);
                    }
                    if let Expr::MethodCall(call, _) = &dot.rhs
                        && !on_ctx
                    {
                        facts.read_names(api.method_names(&call.name), call);
                    }
                }
                Some(ASTNode::Expr(expr)) if variable(expr) == Some(CTX) => {
                    let parent = path.len().checked_sub(2).map(|at| &path[at]);
                    if let Some(misuse) = ctx_use(ast, expr, parent) {
                        facts.ctx_misuse.get_or_insert(misuse);
                    }
                }
                Some(ASTNode::Stmt(Stmt::Var(var, ..))) if var.0.name == CTX => {
                    facts.ctx_misuse.get_or_insert(CtxMisuse::Bound);
                }
                Some(ASTNode::Stmt(Stmt::For(each, _)))
                    if each.0.name == CTX || each.1.as_ref().is_some_and(|at| at.name == CTX) =>
                {
                    facts.ctx_misuse.get_or_insert(CtxMisuse::Bound);
                }
                _ => {}
            }
            true
        });
        facts
    }

    /// What `ctx.<rhs>` uses.
    fn read_ctx(&mut self, rhs: &Expr, api: &ScriptApi) {
        match rhs {
            Expr::MethodCall(call, _) => {
                self.ctx_names.push(CtxUse {
                    name: call.name.to_string(),
                    kind: MemberKind::Call,
                });
                let member = api.member(ApiOwner::Ctx, &call.name);
                let member = member.filter(|member| member.kind == MemberKind::Call);
                self.read_names(member.map(|member| member.names), call);
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

    /// The members a chain of links reads, from `link`: its first is a name of `ctx` when
    /// `on_ctx`, and data when `data`, as after `p`; neither is a member.
    fn read_chain(&mut self, link: &Expr, on_ctx: bool, data: bool) {
        let read = |facts: &mut ScriptFacts, link: &Expr| -> bool {
            let (name, kind) = match link {
                Expr::Property(property, _) => (property.2.as_str(), MemberKind::Field),
                Expr::MethodCall(call, _) => (call.name.as_str(), MemberKind::Method),
                _ => return false,
            };
            if !on_ctx && !data {
                facts.members.push(MemberUse {
                    name: name.to_owned(),
                    kind,
                });
            }
            kind == MemberKind::Field && DATA_ACCESSORS.contains(&name)
        };
        match link {
            Expr::Dot(next, ..) => {
                let next_data = read(self, &next.lhs);
                self.read_chain(&next.rhs, false, next_data);
            }
            Expr::Index(next, ..) => {
                read(self, &next.lhs);
                if let Expr::Dot(element, ..) = &next.rhs {
                    self.read_chain(&element.rhs, false, false);
                }
            }
            link => drop(read(self, link)),
        }
    }

    /// The names of `kind` it gives the API.
    pub(crate) fn names_of(&self, kind: NameKind) -> impl Iterator<Item = &str> {
        self.names
            .iter()
            .filter(move |named| named.kind == kind)
            .map(|named| named.name.as_str())
    }

    /// The literals `call` gives the arguments that `names` marks as names.
    fn read_names(&mut self, names: Option<NameArgs>, call: &FnCallExpr) {
        for (at, kind) in names.into_iter().flatten().enumerate() {
            let literal = call.args.get(at).and_then(string);
            if let (Some(kind), Some(name)) = (kind, literal) {
                self.names.push(ScriptName { kind, name });
            }
        }
    }

    fn value(&mut self, name: &str) {
        if !name.is_empty() {
            self.ctx_names.push(CtxUse {
                name: name.to_owned(),
                kind: MemberKind::Value,
            });
        }
    }
}

/// What breaks the convention in the use `ctx` of the variable, under `parent`: `None` for
/// `ctx.<name>`, or a whole argument of a call that keeps its name.
fn ctx_use(ast: &AST, ctx: &Expr, parent: Option<&ASTNode<'_>>) -> Option<CtxMisuse> {
    let call = match parent {
        Some(ASTNode::Expr(Expr::Dot(dot, ..))) if ptr::eq(&raw const dot.lhs, ctx) => return None,
        Some(
            ASTNode::Expr(Expr::FnCall(call, _) | Expr::MethodCall(call, _))
            | ASTNode::Stmt(Stmt::FnCall(call, _)),
        ) => call,
        _ => return Some(CtxMisuse::Stray),
    };
    let Some(at) = call.args.iter().position(|arg| ptr::eq(arg, ctx)) else {
        return Some(CtxMisuse::Stray);
    };
    if call.is_operator_call() || POINTER_CALLS.contains(&call.name.as_str()) {
        return Some(CtxMisuse::Stray);
    }
    let own = ast
        .iter_functions()
        .find(|function| function.name == call.name && function.params.len() == call.args.len());
    match own {
        Some(function) if function.params[at] != CTX => Some(CtxMisuse::Renamed {
            function: function.name.to_owned(),
        }),
        _ => None,
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
fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies:avatar") {
        ctx.damage(unit, ctx.p.damage * unit.stat("armor"), "magic");
        if !unit.has_modifier("kindle") {
            ctx.add_modifier(unit, "kindle", 100);
        }
    }
    ctx.restore(caster, "mana", caster.pool("energy") + caster.pool_max("health"));
    for camp in ctx.map.markers("camp") {}
    ctx.grant(caster, "spell", ctx.chosen(0, "spells"));
    ctx.add_resource(caster.owner, "gold", 5);
    ctx.spawn_group("north", "mid", "start", ctx.units_tagged("core"));
    if caster.has_tag("slowed") { ctx.add_xp(caster, "level", 1); }
    let first = ctx.teams[0];
    ctx.state.phase = ctx.nearest_visible(caster, 5, name);
    helper(ctx, caster.params.gold);
}

fn helper(ctx, gold) {}
"#;
        let ast = ScriptHost::new(1000).parse(source).unwrap();
        let facts = ScriptFacts::read(&ast, &ScriptApi::release());
        let function = |name: &str, params| Function {
            name: name.to_owned(),
            params,
            ctx_first: true,
        };
        assert_eq!(
            facts.functions,
            [function("helper", 2), function("on_resolve", 3)]
        );
        assert_eq!(facts.ctx_misuse, None);
        let names: Vec<_> = facts
            .ctx_names
            .iter()
            .map(|used| (used.name.as_str(), used.kind))
            .collect();
        let (call, value) = (MemberKind::Call, MemberKind::Value);
        assert_eq!(
            names,
            [
                ("find", call),
                ("p", value),
                ("damage", call),
                ("p", value),
                ("add_modifier", call),
                ("restore", call),
                ("map", value),
                ("grant", call),
                ("chosen", call),
                ("add_resource", call),
                ("spawn_group", call),
                ("units_tagged", call),
                ("add_xp", call),
                ("teams", value),
                ("state", value),
                ("nearest_visible", call),
            ]
        );
        assert_eq!(facts.params, ["radius", "damage"]);
        // A literal counts; a variable, as the filter `name`, cannot be read at load.
        let given: Vec<_> = facts
            .names
            .iter()
            .map(|named| (named.kind, named.name.as_str()))
            .collect();
        assert_eq!(
            given,
            [
                (NameKind::Filter, "enemies:avatar"),
                (NameKind::DamageKind, "magic"),
                (NameKind::Stat, "armor"),
                (NameKind::Modifier, "kindle"),
                (NameKind::Modifier, "kindle"),
                (NameKind::Pool, "mana"),
                (NameKind::Pool, "energy"),
                (NameKind::Pool, "health"),
                (NameKind::MarkerTag, "camp"),
                (NameKind::SlotKind, "spell"),
                (NameKind::Choice, "spells"),
                (NameKind::Resource, "gold"),
                (NameKind::Team, "north"),
                (NameKind::Path, "mid"),
                (NameKind::Tag, "core"),
                (NameKind::Tag, "slowed"),
                (NameKind::Track, "level"),
            ]
        );
    }

    #[test]
    fn every_use_of_ctx_but_a_name_on_it_or_a_call_argument_breaks_the_convention() {
        let renamed = |function: &str| {
            Some(CtxMisuse::Renamed {
                function: function.to_owned(),
            })
        };
        let cases = [
            ("fn on_x(ctx) { ctx.find(1); api(ctx, 2); }", None),
            ("fn on_x(ctx) { let c = ctx; }", Some(CtxMisuse::Stray)),
            ("fn on_x(ctx) { c = ctx; }", Some(CtxMisuse::Stray)),
            ("fn on_x(ctx) { return ctx; }", Some(CtxMisuse::Stray)),
            ("fn on_x(ctx) { [ctx] }", Some(CtxMisuse::Stray)),
            ("fn on_x(ctx) { #{ c: ctx } }", Some(CtxMisuse::Stray)),
            ("fn on_x(ctx) { ctx[0] }", Some(CtxMisuse::Stray)),
            ("fn on_x(ctx) { ctx == 1 }", Some(CtxMisuse::Stray)),
            (
                r#"fn on_x(ctx) { Fn("h").call(ctx) } fn h(c) {}"#,
                Some(CtxMisuse::Stray),
            ),
            (
                "fn on_x(ctx) { let f = |c| c; f.call(ctx) }",
                Some(CtxMisuse::Stray),
            ),
            ("fn on_x(ctx) { h(1, ctx); } fn h(a, c) {}", renamed("h")),
            ("fn on_x(ctx) { 1.h(ctx); } fn h(c) {}", renamed("h")),
            ("fn on_x(ctx) { h(ctx); } fn h(ctx) { ctx.find(1); }", None),
            ("fn on_x(ctx) { let ctx = 1; }", Some(CtxMisuse::Bound)),
            ("fn on_x(ctx) { const ctx = 1; }", Some(CtxMisuse::Bound)),
            // A loop with an empty body is optimized away, so these loops have a body.
            (
                "fn on_x(c) { for ctx in [1] { c.f(1); } }",
                Some(CtxMisuse::Bound),
            ),
            (
                "fn on_x(c) { for (x, ctx) in [1] { c.f(1); } }",
                Some(CtxMisuse::Bound),
            ),
        ];
        let host = ScriptHost::new(1000);
        let api = ScriptApi::release();
        for (source, misuse) in cases {
            let facts = ScriptFacts::read(&host.parse(source).unwrap(), &api);
            assert_eq!(facts.ctx_misuse, misuse, "{source}");
        }
        let facts = ScriptFacts::read(&host.parse("fn on_x(c, ctx) {}").unwrap(), &api);
        assert!(!facts.functions[0].ctx_first);
    }
}
