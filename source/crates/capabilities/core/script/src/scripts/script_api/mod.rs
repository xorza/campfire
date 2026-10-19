use std::any::TypeId;
use std::collections::BTreeSet;

use campfire_values::{EngineEnum, TagProperty};
use rhai::Engine;

use crate::script_host::ScriptHost;
use crate::scripts::api_holder::ApiHolder;
use crate::scripts::hook::Hook;
use crate::scripts::member_decl::{ArgKinds, MemberDecl};
use crate::scripts::member_kind::MemberKind;
use crate::scripts::script_api::api_builder::ApiBuilder;
use crate::scripts::script_api::api_member::ApiMember;
use crate::scripts::script_api::api_reference::ApiReference;
use crate::scripts::script_api::data_field::DataField;
use crate::scripts::script_api::enum_record::EnumRecord;
use crate::scripts::script_api::hook_status::HookStatus;
use crate::scripts::script_api::tag_property_status::TagPropertyStatus;
use crate::scripts::status::Status;
use campfire_values::NameKind;

pub(crate) mod api_builder;
pub(crate) mod api_member;
pub(crate) mod api_reference;
pub(crate) mod data_field;
pub(crate) mod enum_record;
pub(crate) mod hook_status;
pub(crate) mod tag_property_status;

/// The script API as the engine binds it: every name a script may use, each recorded by the
/// call that binds it, or planned by the design and bound by no code yet. The load check and
/// the reference read it; nothing else lists the names.
#[derive(Debug)]
pub struct ScriptApi {
    /// Sorted by holder, then name.
    pub(super) members: Vec<ApiMember>,
    /// Each hook, each tag property, and each data field: whether it runs.
    pub(super) hooks: Vec<HookStatus>,
    pub(super) tag_properties: Vec<TagPropertyStatus>,
    pub(super) data: Vec<DataField>,
    /// The engine enums, in the order they bind.
    pub(super) enums: Vec<EnumRecord>,
    /// The types whose indexers read names the data declares, as `ctx.params` gives one.
    indexed: Vec<TypeId>,
    /// The names of the functions the engine has before the API binds: Rhai's packages and
    /// `Num`'s, getters as `get$<field>`.
    builtins: BTreeSet<String>,
}

impl ScriptApi {
    /// The script API of the release, which `bind` binds into `host` as a match's engine binds
    /// it, recorded with the names the engine has before.
    pub fn release(host: &mut ScriptHost, bind: impl FnOnce(&mut ApiBuilder<'_>)) -> Self {
        let builtins = Self::functions(host.engine_mut());
        let mut api = Self::recorded(host, bind);
        api.builtins = builtins;
        api
    }

    /// The names of every function `engine` has.
    fn functions(engine: &Engine) -> BTreeSet<String> {
        let names =
            engine.collect_fn_metadata(None, |info| Some(info.metadata.name.to_string()), true);
        names.into_iter().collect()
    }

    /// Binds into `host` the script API that `bind` binds, as a match's engine binds it, which
    /// reads no record: the load check reads the release's.
    pub fn bind(host: &mut ScriptHost, bind: impl FnOnce(&mut ApiBuilder<'_>)) {
        Self::recorded(host, bind);
    }

    /// Binds into `host` the script API that `bind` binds, and records it, with no builtins.
    fn recorded(host: &mut ScriptHost, bind: impl FnOnce(&mut ApiBuilder<'_>)) -> Self {
        let mut api = Self {
            members: Vec::new(),
            hooks: Vec::new(),
            tag_properties: Vec::new(),
            data: Vec::new(),
            enums: Vec::new(),
            indexed: Vec::new(),
            builtins: BTreeSet::new(),
        };
        bind(&mut ApiBuilder::new(host, &mut api));
        api
    }

    pub fn hooks(&self) -> &[HookStatus] {
        &self.hooks
    }

    /// Whether the release calls `hook`, and its holder; none for a hook it does not record.
    pub fn hook(&self, hook: Hook) -> Option<&HookStatus> {
        self.hook_named(hook.name)
    }

    /// The hook a script function named `name` defines, and whether the release calls it; none
    /// for a name no module's hook has.
    pub fn hook_named(&self, name: &str) -> Option<&HookStatus> {
        self.hooks.iter().find(|held| held.hook.name == name)
    }

    pub fn tag_properties(&self) -> &[TagPropertyStatus] {
        &self.tag_properties
    }

    pub fn data(&self) -> &[DataField] {
        &self.data
    }

    /// The engine enum scripts name `name`, the module of its members.
    pub fn enum_named(&self, name: &str) -> Option<&EnumRecord> {
        self.enums
            .iter()
            .find(|record| record.engine_enum.name() == name)
    }

    /// Whether the engine has a member `name` of `kind` of its own, before the API: a field's
    /// getter, or a function.
    pub fn builtin(&self, name: &str, kind: MemberKind) -> bool {
        self.builtins.contains(&kind.bound_name(name))
    }

    /// Whether the field or value `name` of some holder leads to data: it gives a type whose
    /// indexer reads names the data declares, so each name after it is data, not a member.
    pub fn leads_to_data(&self, name: &str) -> bool {
        self.members.iter().any(|member| {
            member.decl.name == name
                && matches!(member.decl.kind, MemberKind::Field | MemberKind::Value)
                && member
                    .output
                    .is_some_and(|output| self.indexed.contains(&output.ty))
        })
    }

    /// Records that `T`'s indexer reads names the data declares.
    pub(crate) fn record_indexed<T: 'static>(&mut self) {
        let indexed = TypeId::of::<T>();
        if !self.indexed.contains(&indexed) {
            self.indexed.push(indexed);
        }
    }

    /// The script API's reference, in Markdown: every name, hook, state and data field, with
    /// whether the release runs it.
    pub fn reference(&self) -> String {
        ApiReference(self).text()
    }

    /// Records the engine enum `engine_enum`, with its `members` by their names in scripts.
    pub(crate) fn record_enum(
        &mut self,
        engine_enum: EngineEnum,
        members: impl IntoIterator<Item = &'static str>,
    ) {
        assert!(
            self.enums
                .iter()
                .all(|held| held.engine_enum != engine_enum),
            "{engine_enum:?} is recorded once"
        );
        self.enums.push(EnumRecord {
            engine_enum,
            members: members.into_iter().collect(),
        });
    }

    /// Records whether the release calls a hook.
    pub(crate) fn record_hook(&mut self, hook: HookStatus) {
        assert!(
            self.hooks
                .iter()
                .all(|held| held.hook.name != hook.hook.name),
            "{} is recorded once",
            hook.hook.name
        );
        self.hooks.push(hook);
    }

    /// Records whether the release honours `property`.
    pub(crate) fn record_tag_property(&mut self, property: TagProperty, status: Status) {
        assert!(
            self.tag_properties
                .iter()
                .all(|held| held.property != property),
            "{property:?} is recorded once"
        );
        self.tag_properties
            .push(TagPropertyStatus { property, status });
    }

    /// Records `field`.
    pub(crate) fn record_field(&mut self, field: DataField) {
        let DataField { table, name, .. } = field;
        assert!(
            self.data
                .iter()
                .all(|held| (held.table, held.name) != (table, name)),
            "{table:?}.{name} is recorded once"
        );
        self.data.push(field);
    }

    /// The member `name` of `holder`.
    pub fn member(&self, holder: ApiHolder, name: &str) -> Option<&ApiMember> {
        let at = self
            .members
            .binary_search_by(|member| (member.decl.holder, member.decl.name).cmp(&(holder, name)))
            .ok()?;
        Some(&self.members[at])
    }

    /// The kind of name the field or method `name` of any handle gives, as `kind` says which;
    /// none for a value that names nothing: every handle with a getter or method of that name
    /// gives the same kind, so a script's comparison is checked whatever value it reads it on.
    pub fn returned(&self, name: &str, kind: MemberKind) -> Option<NameKind> {
        self.members
            .iter()
            .filter(|member| {
                member.decl.holder != ApiHolder::CTX
                    && member.decl.kind == kind
                    && member.decl.name == name
            })
            .find_map(|member| member.output)?
            .names
    }

    /// The spec of the method `name` of any handle: every handle with a method of that name
    /// gives the same arguments, so a script's literal is checked whatever value it calls it on.
    pub fn method(&self, name: &str) -> Option<&MemberDecl> {
        self.members.iter().map(|member| &member.decl).find(|spec| {
            spec.holder != ApiHolder::CTX && spec.kind == MemberKind::Method && spec.name == name
        })
    }

    pub fn members(&self) -> &[ApiMember] {
        &self.members
    }

    /// Records a binding of `member`; another binding of a name already recorded adds its
    /// writing, what its form's arguments name and its output, and gives the same spec and
    /// status otherwise.
    pub(crate) fn record(&mut self, member: ApiMember) {
        let ApiMember {
            decl,
            writable,
            status,
            output,
            ref types,
        } = member;
        if let Some(output) = output
            && decl.holder != ApiHolder::CTX
        {
            let others = self.members.iter().filter(|held| {
                held.decl.holder != ApiHolder::CTX
                    && (held.decl.kind, held.decl.name) == (decl.kind, decl.name)
            });
            for held in others.filter_map(|held| held.output) {
                assert_eq!(
                    held.names, output.names,
                    "every handle's `{}` gives the same kind of name",
                    decl.name
                );
            }
        }
        let key = (decl.holder, decl.name);
        match self
            .members
            .binary_search_by(|held| (held.decl.holder, held.decl.name).cmp(&key))
        {
            Ok(at) => {
                let held = &mut self.members[at];
                let args = ArgKinds {
                    names: merged(&held.decl.names, &decl.names, &decl),
                    enums: merged(&held.decl.enums, &decl.enums, &decl),
                };
                assert!(
                    held.decl.with_args(args) == decl.with_args(args) && held.status == status,
                    "the bindings of {}.{} agree",
                    decl.holder.name(),
                    decl.name
                );
                assert!(
                    held.output.is_none() || output.is_none() || held.output == output,
                    "the forms of {}.{} give one type",
                    decl.holder.name(),
                    decl.name
                );
                held.decl = held.decl.with_args(args);
                held.writable |= writable;
                held.output = held.output.or(output);
                held.take_types(types);
            }
            Err(at) => self.members.insert(at, member),
        }
    }
}

/// What the forms of `decl` say each argument is, by place: the `held` forms' and this one's,
/// which agree where both say.
fn merged<T: Copy + PartialEq>(
    held: &[Option<T>; MemberDecl::ARGS],
    form: &[Option<T>; MemberDecl::ARGS],
    decl: &MemberDecl,
) -> [Option<T>; MemberDecl::ARGS] {
    let mut merged = *held;
    for (merged, &said) in merged.iter_mut().zip(form) {
        assert!(
            merged.is_none() || said.is_none() || *merged == said,
            "the forms of {}.{} say alike what an argument is",
            decl.holder.name(),
            decl.name
        );
        *merged = merged.or(said);
    }
    merged
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::scripts::script_api::ScriptApi;
    use crate::scripts::script_api::api_member::ApiMember;
    use crate::scripts::script_api::data_field::DataField;
    use crate::scripts::script_api::enum_record::EnumRecord;

    impl ScriptApi {
        /// Records a binding of `member`, as a module's binding does, to test the registry's
        /// checks.
        pub fn record_binding(&mut self, member: ApiMember) {
            self.record(member);
        }

        /// Records `field`, as a module's data does, to test the registry's checks.
        pub fn record_data_field(&mut self, field: DataField) {
            self.record_field(field);
        }

        /// Its members, to change as a test changes one.
        pub const fn members_mut(&mut self) -> &mut Vec<ApiMember> {
            &mut self.members
        }

        /// The engine enums, in the order they bind.
        pub fn enums(&self) -> &[EnumRecord] {
            &self.enums
        }
    }
}
