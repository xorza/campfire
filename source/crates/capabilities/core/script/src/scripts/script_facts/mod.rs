use std::collections::BTreeSet;
use std::ptr;

use campfire_values::{EngineEnum, ModifierId, Named};
use derive_more::Display;
use rhai::{AST, ASTNode, Expr, FnCallExpr, Position, ScriptFnMetadata, Stmt};

use crate::script_host::ScriptHost;
use crate::script_host::error::ScriptError;
use crate::scripts::api_holder::ApiHolder;
use crate::scripts::applies::Applies;
use crate::scripts::error::CtxMisuse;
use crate::scripts::given_names::{GivenNames, NameShape};
use crate::scripts::member_decl::MemberDecl;
use crate::scripts::member_kind::MemberKind;
use crate::scripts::script_api::ScriptApi;
use campfire_values::NameKind;

/// The variable every script API call goes through, by the script API's convention
/// (06-script-api.md, `ctx` stays visible).
const CTX: &str = "ctx";
/// The function Rhai calls for `a in b`, `contains(b, a)`.
const CONTAINS: &str = "contains";

/// What the package load checks read from a script: its functions, the names it uses on `ctx`,
/// and the string literals it gives the arguments the registry marks as names.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScriptFacts {
    /// Sorted by name, then by parameter count.
    pub functions: Vec<ScriptFunction>,
    pub ctx_names: Vec<CtxUse>,
    /// Each `ctx.params.<name>` it reads.
    pub params: Vec<String>,
    /// Each name it reads after a handle's `.params`, a unit type's param or a map marker's.
    pub handle_params: Vec<String>,
    /// Each field it reads or writes after `.state`, of the mode, a modifier or a unit.
    pub state_fields: Vec<String>,
    /// Each literal it gives an argument the registry marks as a name, in the script's order.
    pub names: Vec<ScriptName>,
    /// Each string literal it compares with a name the API returns, in the script's order.
    pub compared: Vec<ComparedName>,
    /// Each member that returns a name whose value a `switch` reads, whose cases no check reads.
    pub switched: Vec<String>,
    /// Each string literal it gives an argument that takes an engine enum.
    pub enum_strings: Vec<EnumString>,
    /// Each `Module::name` it reads or calls, in the script's order.
    pub enum_paths: Vec<EnumPath>,
    /// Whether it holds statements outside its functions, which no call runs.
    pub top_level: bool,
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

/// A name used on `ctx`, as a value or as a call, and the count of arguments it gives a call,
/// `ctx` aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtxUse {
    pub name: String,
    pub kind: MemberKind,
    pub args: usize,
}

/// A literal a script gives an argument that names something of `kind`, and, for a modifier,
/// how the call applies it; none when it only names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptName {
    pub kind: NameKind,
    pub name: String,
    pub applies: Option<Applies>,
}

/// A string literal a script compares, by `==`, `!=` or `in`, with the name of `kind` that the
/// member `member` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparedName {
    pub member: String,
    pub kind: NameKind,
    pub name: String,
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

/// A name read on a value, as a field or as a method, and the count of arguments it gives a
/// method, the receiver aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberUse {
    pub name: String,
    pub kind: MemberKind,
    pub args: usize,
}

/// What the first link of a chain names: a member of a value, a name of `ctx`, or data, as after
/// `ctx.params`; only a member is recorded as one.
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
        facts.top_level = !ast.statements().is_empty();
        let mut scopes = Scopes::of(ast, api);
        ast.walk(&mut |path: &[ASTNode<'_>]| {
            if let Some(name) = rebound(path.last()) {
                scopes.at_mut(path).shadowed.insert(name.to_owned());
            }
            true
        });
        ast.walk(&mut |path: &[ASTNode<'_>]| {
            let walk = Walk {
                ast,
                api,
                scope: scopes.at(path),
                path,
            };
            match path.last() {
                Some(ASTNode::Expr(Expr::Map(map, _))) => {
                    let keys = map.0.iter().map(|(key, _)| key.name.to_string());
                    facts.map_keys.extend(keys);
                }
                Some(ASTNode::Expr(expr @ Expr::Dot(dot, ..))) => {
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
                        facts.read_contains(&walk, expr);
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
                ) if !call.is_operator_call() && own_function(ast, call).is_none() => {
                    facts.read_comparison(&walk, call);
                    facts.read_call(call, api);
                }
                Some(
                    ASTNode::Expr(Expr::FnCall(call, _)) | ASTNode::Stmt(Stmt::FnCall(call, _)),
                ) if call.is_operator_call() => {
                    facts.read_comparison(&walk, call);
                }
                Some(ASTNode::Stmt(Stmt::Switch(switch, _))) => {
                    if let Some(returned) = walk.returned(&switch.0) {
                        facts.switched.push(returned.member);
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
            Expr::MethodCall(call, _) => self.read_ctx_call(&call.name, &call.args, api),
            Expr::Dot(inner, ..) | Expr::Index(inner, ..) => {
                let Some(name) = property(&inner.lhs) else {
                    return;
                };
                self.value(name);
                let data = data_name(&inner.rhs);
                match name {
                    "params" => self.params.extend(data),
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
                let args = call.args.get(1..).unwrap_or_default();
                self.members.push(MemberUse {
                    name: call.name.to_string(),
                    kind: MemberKind::Method,
                    args: args.len(),
                });
                self.read_args(api.method(&call.name), args);
            }
        }
    }

    /// What the call `name` of `ctx`, with `args`, uses.
    fn read_ctx_call(&mut self, name: &str, args: &[Expr], api: &ScriptApi) {
        self.ctx_names.push(CtxUse {
            name: name.to_owned(),
            kind: MemberKind::Call,
            args: args.len(),
        });
        let member = api.member(ApiHolder::CTX, name);
        let spec = member
            .map(|member| &member.decl)
            .filter(|spec| spec.kind == MemberKind::Call);
        self.read_args(spec, args);
    }

    /// The members a chain of links reads, from `link`, whose first `head` names.
    fn read_chain(&mut self, link: &Expr, head: ChainHead, api: &ScriptApi) {
        let member = head == ChainHead::Member;
        let read = |facts: &mut Self, link: &Expr| -> bool {
            let (name, kind, args) = match link {
                Expr::Property(property, _) => (property.2.as_str(), MemberKind::Field, 0),
                Expr::MethodCall(call, _) => {
                    (call.name.as_str(), MemberKind::Method, call.args.len())
                }
                _ => return false,
            };
            if member {
                facts.members.push(MemberUse {
                    name: name.to_owned(),
                    kind,
                    args,
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
                if member {
                    match property(&next.lhs) {
                        Some("state") => self.state_fields.extend(data_name(&next.rhs)),
                        Some("params") => self.handle_params.extend(data_name(&next.rhs)),
                        _ => {}
                    }
                }
                self.read_chain(&next.rhs, rest, api);
            }
            Expr::Index(next, ..) => {
                read(self, &next.lhs);
                if member {
                    match property(&next.lhs) {
                        Some("state") => self.state_fields.extend(data_name(&next.rhs)),
                        Some("params") => self.handle_params.extend(data_name(&next.rhs)),
                        _ => {}
                    }
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
    fn read_args(&mut self, spec: Option<&MemberDecl>, args: &[Expr]) {
        let Some(spec) = spec else {
            return;
        };
        let kinds = spec.args();
        for (at, arg) in args.iter().enumerate().take(MemberDecl::ARGS) {
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
                let applies = spec.applies.filter(|_| kind == ModifierId::NAMES);
                self.names.push(ScriptName {
                    kind,
                    name: literal,
                    applies,
                });
            }
        }
    }

    /// The literals the call `call` compares with a name the API returns, or a variable bound
    /// to one: `a == b` and `a != b` either way round, `a in [b, c]`, which Rhai calls as
    /// `contains` with the list first, and `b in list`, of a list of names. A list not written in
    /// place holds no literal the check reads.
    fn read_comparison(&mut self, walk: &Walk<'_>, call: &FnCallExpr) {
        let [left, right] = &*call.args else {
            return;
        };
        let one = |expr| {
            walk.returned(expr)
                .filter(|returned| returned.names.shape == NameShape::One)
        };
        let (returned, literals) = match call.name.as_str() {
            "==" | "!=" => match (one(left), one(right)) {
                (Some(returned), _) => (returned, string(right).into_iter().collect()),
                (None, Some(returned)) => (returned, string(left).into_iter().collect()),
                (None, None) => return,
            },
            CONTAINS => match walk.returned(left) {
                Some(list) if list.names.shape == NameShape::List => {
                    (list, string(right).into_iter().collect())
                }
                _ => {
                    let Some(returned) = one(right) else {
                        return;
                    };
                    (returned, strings(left))
                }
            },
            _ => return,
        };
        self.compare(&returned, literals);
    }

    /// The literal `list.contains("b")` finds in a list of names, the chain `dot` ending in that
    /// call.
    fn read_contains(&mut self, walk: &Walk<'_>, dot: &Expr) {
        let Expr::Dot(chain, ..) = dot else {
            return;
        };
        let mut links = Vec::new();
        if !chain_links(&chain.rhs, &mut links) {
            return;
        }
        let Some((last, before)) = links.split_last() else {
            return;
        };
        let Expr::MethodCall(call, _) = last.expr else {
            return;
        };
        let ([arg], false) = (&*call.args, last.indexed) else {
            return;
        };
        if call.name != CONTAINS {
            return;
        }
        let list = if before.is_empty() {
            walk.returned(&chain.lhs)
        } else {
            walk.chain(&chain.lhs, before)
        };
        if let Some(list) = list.filter(|list| list.names.shape == NameShape::List) {
            self.compare(&list, string(arg).into_iter().collect());
        }
    }

    /// Records `literals`, each compared with a name of `returned`.
    fn compare(&mut self, returned: &Returned, literals: Vec<String>) {
        self.compared
            .extend(literals.into_iter().map(|name| ComparedName {
                member: returned.member.clone(),
                kind: returned.names.kind,
                name,
            }));
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
                args: 0,
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

/// A link of a chain after its head: a property or a method call, and whether the value it gives
/// is indexed.
#[derive(Debug, Clone, Copy)]
struct Link<'a> {
    expr: &'a Expr,
    indexed: bool,
}

/// The links of the chain after its head, from `link`, into `out`; `false` for a chain of a link
/// that is neither a property nor a method call.
fn chain_links<'a>(mut link: &'a Expr, out: &mut Vec<Link<'a>>) -> bool {
    loop {
        match link {
            Expr::Property(..) | Expr::MethodCall(..) => {
                out.push(Link {
                    expr: link,
                    indexed: false,
                });
                return true;
            }
            Expr::Dot(next, ..) => {
                out.push(Link {
                    expr: &next.lhs,
                    indexed: false,
                });
                link = &next.rhs;
            }
            Expr::Index(next, ..) => {
                out.push(Link {
                    expr: &next.lhs,
                    indexed: true,
                });
                match &next.rhs {
                    Expr::Dot(element, ..) => link = &element.rhs,
                    _ => return true,
                }
            }
            _ => return false,
        }
    }
}

/// Where the facts' walk stands: the script, the API, the scope of the function it walks, and
/// the path to the node.
#[derive(Debug, Clone, Copy)]
struct Walk<'a> {
    ast: &'a AST,
    api: &'a ScriptApi,
    scope: &'a FunctionScope,
    path: &'a [ASTNode<'a>],
}

impl Walk<'_> {
    /// The names `expr` reads, as the API gives them: a variable bound to names, an item of a
    /// list of names, the last link of a chain, none after a member that leads to data, or a call
    /// as a function, of `ctx` when it gives `ctx` first.
    fn returned(&self, expr: &Expr) -> Option<Returned> {
        match expr {
            Expr::Variable(variable, ..) if variable.2.is_empty() => {
                let name = variable.1.as_str();
                let names = self.bound(name)?;
                Some(Returned {
                    member: name.to_owned(),
                    names,
                })
            }
            Expr::Index(index, ..) => {
                let list = self.returned(&index.lhs)?;
                (list.names.shape == NameShape::List).then(|| Returned {
                    names: list.names.item(),
                    ..list
                })
            }
            Expr::Dot(dot, ..) => {
                let mut links = Vec::new();
                if !chain_links(&dot.rhs, &mut links) {
                    return None;
                }
                self.chain(&dot.lhs, &links)
            }
            Expr::FnCall(call, _)
                if call.namespace.is_empty()
                    && !call.is_operator_call()
                    && own_function(self.ast, call).is_none() =>
            {
                let on_ctx = call.args.first().and_then(variable) == Some(CTX);
                let name = call.name.as_str();
                let names = if on_ctx {
                    self.api.member(ApiHolder::CTX, name)?.output?.names?
                } else {
                    self.api.returned(name, MemberKind::Method)?
                };
                Some(Returned {
                    member: name.to_owned(),
                    names,
                })
            }
            _ => None,
        }
    }

    /// The names the last of `links`, after `head`, reads: none after a member that leads to
    /// data, and an item of a list of names where the link is indexed.
    fn chain(&self, head: &Expr, links: &[Link<'_>]) -> Option<Returned> {
        let (last, before) = links.split_last()?;
        let data = before
            .iter()
            .any(|link| property(link.expr).is_some_and(|name| self.api.leads_to_data(name)));
        if data {
            return None;
        }
        let (name, kind) = match last.expr {
            Expr::Property(property, _) => (property.2.as_str(), MemberKind::Field),
            Expr::MethodCall(call, _) => (call.name.as_str(), MemberKind::Method),
            _ => return None,
        };
        let names = if before.is_empty() && variable(head) == Some(CTX) {
            self.api.member(ApiHolder::CTX, name)?.output?.names?
        } else {
            self.api.returned(name, kind)?
        };
        let names = if last.indexed {
            (names.shape == NameShape::List).then(|| names.item())?
        } else {
            names
        };
        Some(Returned {
            member: name.to_owned(),
            names,
        })
    }

    /// The names the variable `name` holds where the walk stands: the item of the list of names
    /// the nearest loop over it binds `name` to, or the names a hook passes in the parameter
    /// `name`; none for `ctx`, a loop's counter, a name its function binds by `let`, `const` or an
    /// assignment, which the check cannot follow, and any other.
    fn bound(&self, name: &str) -> Option<GivenNames> {
        if name == CTX || self.scope.shadowed.contains(name) {
            return None;
        }
        for (at, node) in self.path.iter().enumerate().rev() {
            let ASTNode::Stmt(Stmt::For(each, _)) = node else {
                continue;
            };
            let (item, counter, flow) = &**each;
            if counter.as_ref().is_some_and(|counter| counter.name == name) {
                return None;
            }
            if item.name == name {
                let around = Self {
                    path: &self.path[..at],
                    ..*self
                };
                let list = around.returned(&flow.expr)?;
                return (list.names.shape == NameShape::List).then(|| list.names.item());
            }
        }
        self.scope
            .params
            .iter()
            .find(|(param, _)| param == name)
            .map(|&(_, names)| names)
    }
}

/// The scope of a function of the script, or of its statements outside them: the names a hook
/// passes in each parameter, the names it binds by `let`, `const` or an assignment, and the span
/// of its body.
#[derive(Debug, Default)]
struct FunctionScope {
    params: Vec<(String, GivenNames)>,
    shadowed: BTreeSet<String>,
    span: Option<(Position, Position)>,
}

/// The scope of each function of a script, and of its statements outside them.
#[derive(Debug)]
struct Scopes {
    functions: Vec<FunctionScope>,
    top: FunctionScope,
}

impl Scopes {
    /// The scopes of `ast`'s functions: a hook's parameters take the names the API says its
    /// call passes in each.
    fn of(ast: &AST, api: &ScriptApi) -> Self {
        let functions = ast
            .iter_fn_def()
            .map(|function| {
                let kinds = api
                    .hook_named(&function.name)
                    .map_or(&[][..], |held| held.hook.param_kinds);
                let params = function
                    .params
                    .iter()
                    .zip(kinds)
                    .filter_map(|(param, names)| Some((param.to_string(), (*names)?)))
                    .collect();
                FunctionScope {
                    params,
                    shadowed: BTreeSet::new(),
                    span: Some((function.body.start_position(), function.body.end_position())),
                }
            })
            .collect();
        Self {
            functions,
            top: FunctionScope::default(),
        }
    }

    /// The place among the functions of the one whose body holds the statement `path` starts
    /// with; none for one outside every function's.
    fn place(&self, path: &[ASTNode<'_>]) -> Option<usize> {
        let Some(ASTNode::Stmt(first)) = path.first() else {
            return None;
        };
        let at = first.position();
        self.functions.iter().position(|function| {
            function
                .span
                .is_some_and(|(start, end)| !at.is_none() && start <= at && at <= end)
        })
    }

    fn at(&self, path: &[ASTNode<'_>]) -> &FunctionScope {
        self.place(path).map_or(&self.top, |at| &self.functions[at])
    }

    fn at_mut(&mut self, path: &[ASTNode<'_>]) -> &mut FunctionScope {
        match self.place(path) {
            Some(at) => &mut self.functions[at],
            None => &mut self.top,
        }
    }
}

/// The variable the node binds by `let` or `const`, or writes by an assignment.
fn rebound<'a>(node: Option<&'a ASTNode<'_>>) -> Option<&'a str> {
    match node? {
        ASTNode::Stmt(Stmt::Var(var, ..)) => Some(var.0.name.as_str()),
        ASTNode::Stmt(Stmt::Assignment(assignment)) => variable(&assignment.1.lhs),
        _ => None,
    }
}

/// A member, or a variable, that holds names.
#[derive(Debug)]
struct Returned {
    member: String,
    names: GivenNames,
}

/// The string literals of a list written in place, as the parse gives it, or folded into a
/// constant.
fn strings(expr: &Expr) -> Vec<String> {
    match expr {
        Expr::Array(items, _) => items.iter().filter_map(string).collect(),
        Expr::DynamicConstant(value, _) => value.clone().into_array().map_or_else(
            |_| Vec::new(),
            |items| {
                items
                    .into_iter()
                    .filter_map(|item| item.into_immutable_string().ok())
                    .map(|text| text.to_string())
                    .collect()
            },
        ),
        _ => Vec::new(),
    }
}

fn string(expr: &Expr) -> Option<String> {
    match expr {
        Expr::StringConstant(text, _) => Some(text.to_string()),
        _ => None,
    }
}
