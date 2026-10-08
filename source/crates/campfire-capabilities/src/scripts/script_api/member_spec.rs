use campfire_sim::Capability;

use crate::scripts::applies::Applies;
use crate::scripts::name_kind::NameKind;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::MemberKind;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::values::engine_enum::EngineEnum;

/// A name of the script API as the code that binds it describes it: whose it is, what it is,
/// who may use it, the forms it is called in, and what its arguments name.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberSpec {
    pub owner: ApiOwner,
    pub name: &'static str,
    pub kind: MemberKind,
    pub roles: RoleSet,
    pub capability: Option<Capability>,
    /// Each form it is called in, by the names of its arguments, the receiver aside; none for a
    /// value, a field or an operator.
    pub forms: Forms,
    pub description: &'static str,
    pub names: NameArgs,
    pub enums: EnumArgs,
    /// How its argument that names a modifier applies it; none for one that only names it.
    pub applies: Option<Applies>,
}

/// The forms of a call or a method, each by the names of its arguments.
pub type Forms = &'static [&'static [&'static str]];

/// What each argument of a call or a method names, by place, the receiver aside: the load
/// checks a literal an argument of a name kind is given.
pub type NameArgs = [Option<NameKind>; MemberSpec::ARGS];

/// Which engine enum each argument of a call takes, by place, the receiver aside: the load
/// refuses a string literal an argument of an enum is given.
pub type EnumArgs = [Option<EngineEnum>; MemberSpec::ARGS];

impl MemberSpec {
    /// The most arguments a member's name roles reach.
    pub(crate) const ARGS: usize = 4;

    /// A value of `ctx`, for every role and of the core until said otherwise.
    pub(crate) const fn value(name: &'static str, description: &'static str) -> MemberSpec {
        MemberSpec::new(ApiOwner::Ctx, name, MemberKind::Value, &[], description)
    }

    /// A call of `ctx`, in the forms `forms`.
    pub(crate) const fn call(
        name: &'static str,
        forms: Forms,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(ApiOwner::Ctx, name, MemberKind::Call, forms, description)
    }

    /// A call of `ctx` for the mode's script only, in the forms `forms`.
    pub(crate) const fn mode_call(
        name: &'static str,
        forms: Forms,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::call(name, forms, description).roles(RoleSet::MODE)
    }

    /// A field of `owner`'s handle.
    pub(crate) const fn field(
        owner: ApiOwner,
        name: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(owner, name, MemberKind::Field, &[], description)
    }

    /// A method of `owner`'s handle, in the forms `forms`.
    pub(crate) const fn method(
        owner: ApiOwner,
        name: &'static str,
        forms: Forms,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(owner, name, MemberKind::Method, forms, description)
    }

    /// The operator `name` on `owner`'s handles.
    pub(crate) const fn operator(
        owner: ApiOwner,
        name: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(owner, name, MemberKind::Operator, &[], description)
    }

    const fn new(
        owner: ApiOwner,
        name: &'static str,
        kind: MemberKind,
        forms: Forms,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec {
            owner,
            name,
            kind,
            roles: RoleSet::ALL,
            capability: None,
            forms,
            description,
            names: [None; MemberSpec::ARGS],
            enums: [None; MemberSpec::ARGS],
            applies: None,
        }
    }

    /// The same, for `roles` only.
    pub(crate) const fn roles(mut self, roles: RoleSet) -> MemberSpec {
        self.roles = roles;
        self
    }

    /// The same, of `capability`.
    pub(crate) const fn capability(mut self, capability: Capability) -> MemberSpec {
        self.capability = Some(capability);
        self
    }

    /// The same, its argument that names a modifier applying it as `applies` says.
    pub(crate) const fn applies(mut self, applies: Applies) -> MemberSpec {
        self.applies = Some(applies);
        self
    }

    /// The same, its argument at `at`, the receiver aside, a name of `kind`.
    pub(crate) const fn name(mut self, at: usize, kind: NameKind) -> MemberSpec {
        self.names[at] = Some(kind);
        self
    }

    /// The same, its argument at `at`, the receiver aside, a member of `engine_enum`.
    pub(crate) const fn takes(mut self, at: usize, engine_enum: EngineEnum) -> MemberSpec {
        self.enums[at] = Some(engine_enum);
        self
    }
}
