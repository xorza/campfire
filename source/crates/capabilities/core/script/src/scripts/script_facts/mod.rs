use std::ptr;

use campfire_values::EngineEnum;
use derive_more::Display;
use rhai::{AST, ASTNode, Expr, FnCallExpr, ScriptFnMetadata, Stmt};

use crate::script_host::ScriptHost;
use crate::script_host::error::ScriptError;
use crate::scripts::api_owner::ApiOwner;
use crate::scripts::applies::Applies;
use crate::scripts::error::CtxMisuse;
use crate::scripts::member_kind::MemberKind;
use crate::scripts::member_spec::MemberSpec;
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::ScriptApi;

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
pub struct ScriptFacts {
    /// Sorted by name, then by parameter count.
    pub functions: Vec<ScriptFunction>,
    pub ctx_names: Vec<CtxUse>,
    /// Each `ctx.p.<name>` it reads.
    pub params: Vec<String>,
    /// Each field it reads or writes after `.state`, of the mode, a modifier or a unit.
    pub state_fields: Vec<String>,
    /// Each literal it gives an argument the registry marks as a name, in the script's order.
    pub names: Vec<ScriptName>,
    /// Each string literal it gives an argument that takes an engine enum.
    pub enum_strings: Vec<EnumString>,
    /// Each `Module::name` it reads or calls, in the script's order.
    pub enum_paths: Vec<EnumPath>,
    /// Whether it makes a function pointer: a closure, an anonymous function or a call of `Fn`.
    pub function_pointer: bool,
    /// Each field or method it reads on a value other than `ctx`, a call of no namespace that is
    /// not its own and gives `ctx` no first argument among them, but the names after a member
    /// that leads to data; and the keys of its object-map literals.
    pub members: Vec<MemberUse>,
    pub map_keys: Vec<String>,
    /// The first use of `ctx` that breaks the convention, if any.
    pub ctx_misuse: Option<CtxMisuse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFunction {
    pub name: String,
    pub params: usize,
    /// Whether its first parameter is named `ctx`.
    pub ctx_first: bool,
}

/// A name used on `ctx`, as a value or as a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtxUse {
    pub name: String,
    pub kind: MemberKind,
}

/// A literal a script gives an argument that names something of `kind`, and, for a modifier,
/// how the call applies it; none when it only names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptName {
    pub kind: NameKind,
    pub name: String,
    pub applies: Option<Applies>,
}

/// A string literal a script gives `call`'s argument that takes a member of `engine_enum`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumString {
    pub call: String,
    pub engine_enum: EngineEnum,
}

/// A name a script reads in a module, `Relation::Hostile`, as a value, or calls,
/// `PathEnd::named(text)`.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
#[display("{module}::{name}")]
pub struct EnumPath {
    pub module: String,
    pub name: String,
    pub kind: MemberKind,
}

/// A name read on a value, as a field or as a method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberUse {
    pub name: String,
    pub kind: MemberKind,
}

/// What the first link of a chain names: a member of a value, a name of `ctx`, or data, as after
/// `ctx.p`; only a member is recorded as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChainHead {
    Member,
    Ctx,
    Data,
}

impl ScriptFacts {
    /// The facts of `source`, as `host` parses it; an error when it does not parse. `ctx` is the
    /// variable of that name, as every hook's first parameter is by convention and every helper
    /// passes it on; a use that breaks the convention is recorded, so the facts see every use of
    /// `ctx` in a script that keeps it.
    pub fn read(host: &ScriptHost, source: &str, api: &ScriptApi) -> Result<Self, ScriptError> {
        Ok(Self::of(&host.parse(source)?, api))
    }

    fn of(ast: &AST, api: &ScriptApi) -> Self {
        let mut facts = Self {
            functions: ast
                .iter_functions()
                .map(|function| ScriptFunction {
                    name: function.name.to_owned(),
                    params: function.params.len(),
                    ctx_first: function.params.first() == Some(&CTX),
                })
                .collect(),
            ..Self::default()
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
                        let head = if on_ctx {
                            ChainHead::Ctx
                        } else {
                            ChainHead::Member
                        };
                        facts.read_chain(&dot.rhs, head, api);
                    }
                    if let Expr::MethodCall(call, _) = &dot.rhs
                        && !on_ctx
                    {
                        facts.read_args(api.method(&call.name), &call.args);
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
                Some(
                    ASTNode::Expr(Expr::FnCall(call, _)) | ASTNode::Stmt(Stmt::FnCall(call, _)),
                ) if !call.is_operator_call() && own_function(ast, call).is_none() => {
                    facts.read_call(call, api);
                }
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
            Expr::MethodCall(call, _) => self.read_ctx_call(&call.name, &call.args, api),
            Expr::Dot(inner, ..) | Expr::Index(inner, ..) => {
                let Some(name) = property(&inner.lhs) else {
                    return;
                };
                self.value(name);
                let data = data_name(&inner.rhs);
                match name {
                    "p" => self.params.extend(data),
                    "state" => self.state_fields.extend(data),
                    _ => {}
                }
            }
            _ => self.value(property(rhs).unwrap_or_default()),
        }
    }

    /// What the unqualified call `call`, not the script's own, uses: with `ctx` first, it calls
    /// `ctx`, as `ctx.<name>(…)` does, Rhai's method call being the call with its receiver first;
    /// else it calls a method of its first argument, or a function of the engine's.
    fn read_call(&mut self, call: &FnCallExpr, api: &ScriptApi) {
        match call.args.split_first() {
            Some((first, args)) if variable(first) == Some(CTX) => {
                self.read_ctx_call(&call.name, args, api);
            }
            _ => {
                self.members.push(MemberUse {
                    name: call.name.to_string(),
                    kind: MemberKind::Method,
                });
                let args = call.args.get(1..).unwrap_or_default();
                self.read_args(api.method(&call.name), args);
            }
        }
    }

    /// What the call `name` of `ctx`, with `args`, uses.
    fn read_ctx_call(&mut self, name: &str, args: &[Expr], api: &ScriptApi) {
        self.ctx_names.push(CtxUse {
            name: name.to_owned(),
            kind: MemberKind::Call,
        });
        let member = api.member(ApiOwner::Ctx, name);
        let spec = member
            .map(|member| &member.spec)
            .filter(|spec| spec.kind == MemberKind::Call);
        self.read_args(spec, args);
    }

    /// The members a chain of links reads, from `link`, whose first `head` names.
    fn read_chain(&mut self, link: &Expr, head: ChainHead, api: &ScriptApi) {
        let member = head == ChainHead::Member;
        let read = |facts: &mut Self, link: &Expr| -> bool {
            let (name, kind) = match link {
                Expr::Property(property, _) => (property.2.as_str(), MemberKind::Field),
                Expr::MethodCall(call, _) => (call.name.as_str(), MemberKind::Method),
                _ => return false,
            };
            if member {
                facts.members.push(MemberUse {
                    name: name.to_owned(),
                    kind,
                });
            }
            kind == MemberKind::Field && api.leads_to_data(name)
        };
        match link {
            Expr::Dot(next, ..) => {
                let rest = if read(self, &next.lhs) {
                    ChainHead::Data
                } else {
                    ChainHead::Member
                };
                if member && property(&next.lhs) == Some("state") {
                    self.state_fields.extend(data_name(&next.rhs));
                }
                self.read_chain(&next.rhs, rest, api);
            }
            Expr::Index(next, ..) => {
                read(self, &next.lhs);
                if member && property(&next.lhs) == Some("state") {
                    self.state_fields.extend(data_name(&next.rhs));
                }
                if let Expr::Dot(element, ..) = &next.rhs {
                    self.read_chain(&element.rhs, ChainHead::Member, api);
                }
            }
            link => drop(read(self, link)),
        }
    }

    /// The literals of `args`, the arguments past its receiver of a call of the member `spec`,
    /// that its arguments mark: a name, a modifier's with how the call applies it, and a string
    /// an engine enum's argument is given.
    fn read_args(&mut self, spec: Option<&MemberSpec>, args: &[Expr]) {
        let Some(spec) = spec else {
            return;
        };
        let kinds = spec.args();
        for (at, arg) in args.iter().enumerate().take(MemberSpec::ARGS) {
            let Some(literal) = string(arg) else {
                continue;
            };
            if let Some(engine_enum) = kinds.enums[at] {
                self.enum_strings.push(EnumString {
                    call: spec.name.to_owned(),
                    engine_enum,
                });
            }
            if let Some(kind) = kinds.names[at] {
                let applies = spec.applies.filter(|_| kind == NameKind::Modifier);
                self.names.push(ScriptName {
                    kind,
                    name: literal,
                    applies,
                });
            }
        }
    }

    /// The modifiers it applies by a literal name, each with how its call applies it.
    pub fn applied(&self) -> impl Iterator<Item = (&str, Applies)> {
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
/// `ctx.<name>`, the first argument of a function call of no namespace that is not the script's
/// own, which calls `ctx`, or a whole argument of the script's own function, called as a
/// function or a method, that keeps its name.
fn ctx_use(ast: &AST, ctx: &Expr, parent: Option<&ASTNode<'_>>) -> Option<CtxMisuse> {
    let (call, method) = match parent {
        Some(ASTNode::Expr(Expr::Dot(dot, ..))) if ptr::eq(&raw const dot.lhs, ctx) => return None,
        Some(ASTNode::Expr(Expr::FnCall(call, _)) | ASTNode::Stmt(Stmt::FnCall(call, _))) => {
            (call, false)
        }
        Some(ASTNode::Expr(Expr::MethodCall(call, _))) => (call, true),
        _ => return Some(CtxMisuse::Stray),
    };
    let Some(at) = call.args.iter().position(|arg| ptr::eq(arg, ctx)) else {
        return Some(CtxMisuse::Stray);
    };
    if call.is_operator_call() || !call.namespace.is_empty() {
        return Some(CtxMisuse::Stray);
    }
    match own_function(ast, call) {
        Some(function) if function.params[at] != CTX => Some(CtxMisuse::Renamed {
            function: function.name.to_owned(),
        }),
        Some(_) => None,
        None if at == 0 && !method => None,
        None => Some(CtxMisuse::Stray),
    }
}

/// The script's own function that `call` calls, by its name and its number of arguments, the
/// receiver of a method call aside, as Rhai resolves a script's function before the engine's.
fn own_function<'a>(ast: &'a AST, call: &FnCallExpr) -> Option<ScriptFnMetadata<'a>> {
    ast.iter_functions()
        .find(|function| function.name == call.name && function.params.len() == call.args.len())
}

/// The name of a variable.
fn variable(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Variable(variable, ..) => Some(&variable.1),
        _ => None,
    }
}

/// The name of data a chain of links after a member that leads to data starts with: `a` of
/// `.a`, `.a.b` or `.a[0]`, and of `["a"]`, a string literal index.
fn data_name(expr: &Expr) -> Option<String> {
    let first = match expr {
        Expr::Dot(next, ..) | Expr::Index(next, ..) => &next.lhs,
        next => next,
    };
    property(first).map(str::to_owned).or_else(|| string(first))
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
