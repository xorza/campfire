use campfire_sim::Capability;

use crate::scripts::name_kind::NameKind;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::MemberKind;
use crate::scripts::script_api::api_owner::ApiOwner;

/// A name as the code that binds it describes it: an `ApiMember` with one form.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemberSpec {
    pub owner: ApiOwner,
    pub name: &'static str,
    pub kind: MemberKind,
    pub roles: RoleSet,
    pub capability: Option<Capability>,
    pub signature: &'static str,
    pub description: &'static str,
    pub names: NameArgs,
}

/// What each argument of a call or a method names, by place, the receiver aside: the load
/// checks a literal an argument of a name kind is given.
pub type NameArgs = [Option<NameKind>; MemberSpec::ARGS];
impl MemberSpec {
    /// The most arguments a member's name roles reach.
    pub(crate) const ARGS: usize = 4;

    /// A value of `ctx`, for every role and of the core until said otherwise.
    pub(crate) const fn value(name: &'static str, description: &'static str) -> MemberSpec {
        MemberSpec::new(ApiOwner::Ctx, name, MemberKind::Value, "", description)
    }

    /// A call of `ctx`, in the form `signature`.
    pub(crate) const fn call(
        name: &'static str,
        signature: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(
            ApiOwner::Ctx,
            name,
            MemberKind::Call,
            signature,
            description,
        )
    }

    /// A field of `owner`'s handle.
    pub(crate) const fn field(
        owner: ApiOwner,
        name: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(owner, name, MemberKind::Field, "", description)
    }

    /// A method of `owner`'s handle, in the form `signature`.
    pub(crate) const fn method(
        owner: ApiOwner,
        name: &'static str,
        signature: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(owner, name, MemberKind::Method, signature, description)
    }

    /// The operator `name` on `owner`'s handles.
    pub(crate) const fn operator(
        owner: ApiOwner,
        name: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(owner, name, MemberKind::Operator, "", description)
    }

    const fn new(
        owner: ApiOwner,
        name: &'static str,
        kind: MemberKind,
        signature: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec {
            owner,
            name,
            kind,
            roles: RoleSet::ALL,
            capability: None,
            signature,
            description,
            names: [None; MemberSpec::ARGS],
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

    /// The same, its argument at `at`, the receiver aside, a name of `kind`.
    pub(crate) const fn name(mut self, at: usize, kind: NameKind) -> MemberSpec {
        self.names[at] = Some(kind);
        self
    }
}
