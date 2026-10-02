use std::{fmt, ptr};

use campfire_capabilities::{
    ApiOwner, Applies, EngineEnum, EnumArgs, MemberKind, NameArgs, NameKind, ScriptApi,
};
use campfire_script::rhai::{AST, ASTNode, Expr, FnCallExpr, Stmt};

use crate::error::{CtxMisuse, LoadProblem, Place};

/// The variable every script API call goes through, by design 08's convention.
const CTX: &str = "ctx";
/// Rhai's call that makes a function pointer of a function's name, which its optimizer folds
/// into a constant when the name is a literal, and the start of the name it gives each anonymous
/// function a script defines, which it does not export.
const FN_POINTER: &str = "Fn";
const ANONYMOUS: &str = "anon$";

/// What the package load checks read from a script: its functions, the names it uses on `ctx`,
/// and the string literals it gives the arguments the registry marks as names.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ScriptFacts {
    /// Sorted by name, then by parameter count.
    pub(crate) functions: Vec<Function>,
    pub(crate) ctx_names: Vec<CtxUse>,
    /// Each `ctx.p.<name>` it reads.
    pub(crate) params: Vec<String>,
    /// Each field it reads or writes after `.state`, of the mode, a modifier or a unit.
    pub(crate) state_fields: Vec<String>,
    /// Each literal it gives an argument the registry marks as a name, in the script's order.
    pub(crate) names: Vec<ScriptName>,
    /// Each string literal it gives an argument that takes an engine enum.
    pub(crate) enum_strings: Vec<EnumString>,
    /// Each `Module::name` it reads or calls, in the script's order.
    pub(crate) enum_paths: Vec<EnumPath>,
    /// Whether it makes a function pointer: a closure, an anonymous function or a call of `Fn`.
    pub(crate) function_pointer: bool,
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

/// A literal a script gives an argument that names something of `kind`, and, for a modifier,
/// how the call applies it; none when it only names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptName {
    pub(crate) kind: NameKind,
    pub(crate) name: String,
    pub(crate) applies: Option<Applies>,
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

/// A string literal a script gives `call`'s argument that takes a member of `engine_enum`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnumString {
    pub(crate) call: String,
    pub(crate) engine_enum: EngineEnum,
}

/// A name a script reads in a module, `Relation::Hostile`, as a value, or calls,
/// `PathEnd::named(text)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnumPath {
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) kind: MemberKind,
}

/// As the script writes it: `Relation::Hostile`.
impl fmt::Display for EnumPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.module, self.name)
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
        facts.function_pointer = facts
            .functions
            .iter()
            .any(|function| function.name.starts_with(ANONYMOUS));
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
                        facts.read_names(api.method_names(&call.name), None, call);
                    }
                }
                Some(ASTNode::Expr(Expr::Variable(variable, ..))) if !variable.2.is_empty() => {
                    facts.enum_paths.push(EnumPath {
                        module: variable.2.to_string(),
                        name: variable.1.to_string(),
                        kind: MemberKind::Value,
                    });
                }
                Some(
                    ASTNode::Expr(Expr::FnCall(call, _)) | ASTNode::Stmt(Stmt::FnCall(call, _)),
                ) if !call.namespace.is_empty() => {
                    facts.enum_paths.push(EnumPath {
                        module: call.namespace.to_string(),
                        name: call.name.to_string(),
                        kind: MemberKind::Call,
                    });
                }
                Some(
                    ASTNode::Expr(Expr::FnCall(call, _)) | ASTNode::Stmt(Stmt::FnCall(call, _)),
                ) if call.name == FN_POINTER => facts.function_pointer = true,
                Some(ASTNode::Expr(Expr::DynamicConstant(value, _))) if value.is_fnptr() => {
                    facts.function_pointer = true;
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
                let applies = member.and_then(|member| member.applies);
                self.read_names(member.map(|member| member.names), applies, call);
                self.read_enums(member.map(|member| member.enums), call);
            }
            Expr::Dot(inner, ..) | Expr::Index(inner, ..) => {
                let Some(name) = property(&inner.lhs) else {
                    return;
                };
                self.value(name);
                let data = first_property(&inner.rhs).map(str::to_owned);
                match name {
                    "p" => self.params.extend(data),
                    "state" => self.state_fields.extend(data),
                    _ => {}
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
                if !on_ctx && !data && property(&next.lhs) == Some("state") {
                    let field = first_property(&next.rhs).map(str::to_owned);
                    self.state_fields.extend(field);
                }
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

    /// The literals `call` gives the arguments that `names` marks as names, a modifier's with
    /// how the call `applies` it.
    fn read_names(&mut self, names: Option<NameArgs>, applies: Option<Applies>, call: &FnCallExpr) {
        for (at, kind) in names.into_iter().flatten().enumerate() {
            let literal = call.args.get(at).and_then(string);
            if let (Some(kind), Some(name)) = (kind, literal) {
                let applies = applies.filter(|_| kind == NameKind::Modifier);
                self.names.push(ScriptName {
                    kind,
                    name,
                    applies,
                });
            }
        }
    }

    /// Each string literal `call` gives an argument that `enums` marks as taking an engine enum.
    fn read_enums(&mut self, enums: Option<EnumArgs>, call: &FnCallExpr) {
        for (at, engine_enum) in enums.into_iter().flatten().enumerate() {
            let literal = call.args.get(at).and_then(string);
            if let (Some(engine_enum), Some(_)) = (engine_enum, literal) {
                self.enum_strings.push(EnumString {
                    call: call.name.to_string(),
                    engine_enum,
                });
            }
        }
    }

    /// The modifiers it applies by a literal name, each with how its call applies it.
    pub(crate) fn applied(&self) -> impl Iterator<Item = (&str, Applies)> {
        self.names
            .iter()
            .filter_map(|named| Some((named.name.as_str(), named.applies?)))
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
    if call.is_operator_call() {
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

/// The name a chain of links starts with: `a` of `a`, `a.b` or `a[0]`.
fn first_property(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Dot(next, ..) | Expr::Index(next, ..) => property(&next.lhs),
        next => property(next),
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
mod tests;
