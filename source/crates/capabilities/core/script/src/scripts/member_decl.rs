use campfire_sim::Capability;
use campfire_values::EngineEnum;

use crate::scripts::api_holder::ApiHolder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::applies::Applies;
use crate::scripts::member_kind::MemberKind;
use crate::scripts::planned_capability::PlannedCapability;
use crate::scripts::role_set::RoleSet;
use campfire_values::NameKind;

/// A name of the script API as the code that binds it describes it: whose it is, what it is,
/// who may use it, the forms it is called in, and what its arguments name.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberDecl {
    pub holder: ApiHolder,
    pub name: &'static str,
    pub kind: MemberKind,
    pub roles: RoleSet,
    pub capability: Option<Capability>,
    /// The capability it is planned with, of no module the release builds; none for a name of a
    /// built capability's, or the core's.
    pub planned_in: Option<PlannedCapability>,
    /// Each form it is called in, by the names of its arguments, the receiver aside; none for a
    /// value, a field or an operator.
    pub forms: Forms,
    pub description: &'static str,
    pub names: NameArgs,
    pub enums: EnumArgs,
    /// How its argument that names a modifier applies it; none for one that only names it.
    pub applies: Option<Applies>,
    /// The package API version it came in, when it runs: the first unless said otherwise.
    pub since: ApiVersion,
    /// Whether only the mode's own script may use it, and no rules script its list names
    /// (03-game-scripting.md, Rules scripts in order).
    pub own_script: bool,
}

/// The forms of a call or a method, each by the names of its arguments.
pub(crate) type Forms = &'static [&'static [&'static str]];

/// What each argument of a call or a method names, by place, the receiver aside: the load
/// checks a literal an argument of a name kind is given.
pub(crate) type NameArgs = [Option<NameKind>; MemberDecl::ARGS];

/// Which engine enum each argument of a call takes, by place, the receiver aside: the load
/// refuses a string literal an argument of an enum is given.
pub(crate) type EnumArgs = [Option<EngineEnum>; MemberDecl::ARGS];

/// What the arguments of a form name, and the engine enums they take, as their types say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArgKinds {
    pub names: NameArgs,
    pub enums: EnumArgs,
}

impl MemberDecl {
    /// The most arguments a form takes, the receiver aside.
    pub(crate) const ARGS: usize = 5;

    /// A value of `ctx`, for every role and of the core until said otherwise.
    pub const fn value(name: &'static str, description: &'static str) -> Self {
        Self::new(ApiHolder::CTX, name, MemberKind::Value, &[], description)
    }

    /// A call of `ctx`, in the forms `forms`.
    pub const fn call(name: &'static str, forms: Forms, description: &'static str) -> Self {
        Self::new(ApiHolder::CTX, name, MemberKind::Call, forms, description)
    }

    /// A call of `ctx` for the mode's script only, in the forms `forms`.
    pub const fn mode_call(name: &'static str, forms: Forms, description: &'static str) -> Self {
        Self::call(name, forms, description).roles(RoleSet::MODE)
    }

    /// A field of `holder`'s handle.
    pub const fn field(holder: ApiHolder, name: &'static str, description: &'static str) -> Self {
        Self::new(holder, name, MemberKind::Field, &[], description)
    }

    /// A method of `holder`'s handle, in the forms `forms`.
    pub const fn method(
        holder: ApiHolder,
        name: &'static str,
        forms: Forms,
        description: &'static str,
    ) -> Self {
        Self::new(holder, name, MemberKind::Method, forms, description)
    }

    /// The operator `name` on `holder`'s handles.
    pub const fn operator(
        holder: ApiHolder,
        name: &'static str,
        description: &'static str,
    ) -> Self {
        Self::new(holder, name, MemberKind::Operator, &[], description)
    }

    const fn new(
        holder: ApiHolder,
        name: &'static str,
        kind: MemberKind,
        forms: Forms,
        description: &'static str,
    ) -> Self {
        Self {
            holder,
            name,
            kind,
            roles: RoleSet::ALL,
            capability: None,
            planned_in: None,
            forms,
            description,
            names: [None; Self::ARGS],
            enums: [None; Self::ARGS],
            applies: None,
            since: ApiVersion::FIRST,
            own_script: false,
        }
    }

    /// The same, for `roles` only.
    pub const fn roles(mut self, roles: RoleSet) -> Self {
        self.roles = roles;
        self
    }

    /// The same, of `capability`.
    pub const fn capability(mut self, capability: Capability) -> Self {
        self.capability = Some(capability);
        self
    }

    /// The same, planned with `planned`, a capability the release does not build.
    pub const fn planned_in(mut self, planned: PlannedCapability) -> Self {
        self.planned_in = Some(planned);
        self
    }

    /// The same, for the mode's own script alone, not a rules script its list names.
    pub const fn own_script(mut self) -> Self {
        self.own_script = true;
        self
    }

    /// The same, come in the package API version `since`.
    pub const fn since(mut self, since: ApiVersion) -> Self {
        self.since = since;
        self
    }

    /// The same, its argument that names a modifier applying it as `applies` says.
    pub const fn applies(mut self, applies: Applies) -> Self {
        self.applies = Some(applies);
        self
    }

    /// The same, its arguments naming and taking what `args` says, the receiver aside, as the
    /// types of a form's arguments say.
    pub const fn with_args(mut self, args: ArgKinds) -> Self {
        self.names = args.names;
        self.enums = args.enums;
        self
    }

    /// What its arguments name and take, the receiver aside.
    pub const fn args(&self) -> ArgKinds {
        ArgKinds {
            names: self.names,
            enums: self.enums,
        }
    }

    /// Whether a form of it takes `arity` arguments: a value and a field take none, and an
    /// operator any, as its operands are no arguments.
    pub fn has_form_of(&self, arity: usize) -> bool {
        match self.kind {
            MemberKind::Value | MemberKind::Field => arity == 0,
            MemberKind::Operator => true,
            MemberKind::Call | MemberKind::Method => {
                self.forms.iter().any(|form| form.len() == arity)
            }
        }
    }
}
