use std::any::TypeId;
use std::collections::BTreeSet;

use campfire_sim::Capability;
use campfire_values::{EngineEnum, TagProperty};
use rhai::Engine;

use crate::script_host::ScriptHost;
use crate::scripts::api_owner::ApiOwner;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::data_table::DataTable;
use crate::scripts::hook::Hook;
use crate::scripts::member_kind::MemberKind;
use crate::scripts::member_spec::{ArgKinds, MemberSpec};
use crate::scripts::script_api::api_builder::ApiBuilder;
use crate::scripts::script_api::api_member::ApiMember;
use crate::scripts::script_api::api_reference::ApiReference;
use crate::scripts::script_api::data_field::DataField;
use crate::scripts::script_api::enum_record::EnumRecord;
use crate::scripts::script_api::hook_status::HookStatus;
use crate::scripts::script_api::tag_property_status::TagPropertyStatus;
use crate::scripts::status::Status;

pub(crate) mod api_builder;
pub(crate) mod api_member;
pub(crate) mod api_reference;
pub(crate) mod data_field;
pub(crate) mod enum_record;
pub(crate) mod hook_status;
pub(crate) mod tag_property_status;

/// The script API as the engine binds it: every name a script may use, each recorded by the
/// call that binds it, or planned, by design 08, and bound by no code yet. The load check and
/// the reference read it; nothing else lists the names.
#[derive(Debug)]
pub struct ScriptApi {
    /// Sorted by owner, then name.
    pub(super) members: Vec<ApiMember>,
    /// Each hook, each tag property, and each data field: whether it runs.
    pub(super) hooks: Vec<HookStatus>,
    pub(super) tag_properties: Vec<TagPropertyStatus>,
    pub(super) data: Vec<DataField>,
    /// The engine enums, in the order they bind.
    pub(super) enums: Vec<EnumRecord>,
    /// The types whose indexers read names the data declares, as `ctx.p` gives one.
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

    /// Whether the release calls `hook`, and its owner; none for a hook it does not record.
    pub fn hook(&self, hook: Hook) -> Option<&HookStatus> {
        self.hooks.iter().find(|held| held.hook == hook)
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

    /// Whether the field or value `name` of some owner leads to data: it gives a type whose
    /// indexer reads names the data declares, so each name after it is data, not a member.
    pub fn leads_to_data(&self, name: &str) -> bool {
        self.members.iter().any(|member| {
            member.spec.name == name
                && matches!(member.spec.kind, MemberKind::Field | MemberKind::Value)
                && member
                    .output
                    .is_some_and(|output| self.indexed.contains(&output))
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
            self.hooks.iter().all(|held| held.hook != hook.hook),
            "{:?} is recorded once",
            hook.hook
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

    /// Records the fields of `table`: `runs`, which the release reads, and `planned`.
    pub(crate) fn record_data(
        &mut self,
        table: DataTable,
        runs: &[&'static str],
        planned: &[&'static str],
        capability: Option<Capability>,
    ) {
        for (names, status) in [
            (runs, Status::Runs(ApiVersion::FIRST)),
            (planned, Status::Planned),
        ] {
            for &name in names {
                self.record_field(DataField {
                    table,
                    name,
                    status,
                    capability,
                });
            }
        }
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

    /// The capability that reads the field `name` of `table`, which the API records; none for
    /// the core's.
    pub fn data_owner(&self, table: DataTable, name: &str) -> Option<Capability> {
        self.data
            .iter()
            .find(|held| (held.table, held.name) == (table, name))
            .unwrap_or_else(|| panic!("{table:?}.{name} is recorded"))
            .capability
    }

    /// The member `name` of `owner`.
    pub fn member(&self, owner: ApiOwner, name: &str) -> Option<&ApiMember> {
        let at = self
            .members
            .binary_search_by(|member| (member.spec.owner, member.spec.name).cmp(&(owner, name)))
            .ok()?;
        Some(&self.members[at])
    }

    /// The spec of the method `name` of any handle: every handle with a method of that name
    /// gives the same arguments, so a script's literal is checked whatever value it calls it on.
    pub fn method(&self, name: &str) -> Option<&MemberSpec> {
        self.members.iter().map(|member| &member.spec).find(|spec| {
            spec.owner != ApiOwner::Ctx && spec.kind == MemberKind::Method && spec.name == name
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
            spec,
            writable,
            status,
            output,
        } = member;
        let key = (spec.owner, spec.name);
        match self
            .members
            .binary_search_by(|held| (held.spec.owner, held.spec.name).cmp(&key))
        {
            Ok(at) => {
                let held = &mut self.members[at];
                let args = ArgKinds {
                    names: merged(&held.spec.names, &spec.names, &spec),
                    enums: merged(&held.spec.enums, &spec.enums, &spec),
                };
                assert!(
                    held.spec.with_args(args) == spec.with_args(args) && held.status == status,
                    "the bindings of {:?}.{} agree",
                    spec.owner,
                    spec.name
                );
                assert!(
                    held.output.is_none() || output.is_none() || held.output == output,
                    "the forms of {:?}.{} give one type",
                    spec.owner,
                    spec.name
                );
                held.spec = held.spec.with_args(args);
                held.writable |= writable;
                held.output = held.output.or(output);
            }
            Err(at) => self.members.insert(at, member),
        }
    }
}

/// What the forms of `spec` say each argument is, by place: the `held` forms' and this one's,
/// which agree where both say.
fn merged<T: Copy + PartialEq>(
    held: &[Option<T>; MemberSpec::ARGS],
    form: &[Option<T>; MemberSpec::ARGS],
    spec: &MemberSpec,
) -> [Option<T>; MemberSpec::ARGS] {
    let mut merged = *held;
    for (merged, &said) in merged.iter_mut().zip(form) {
        assert!(
            merged.is_none() || said.is_none() || *merged == said,
            "the forms of {:?}.{} say alike what an argument is",
            spec.owner,
            spec.name
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
