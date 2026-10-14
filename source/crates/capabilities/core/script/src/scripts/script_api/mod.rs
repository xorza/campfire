use std::collections::BTreeSet;

use campfire_sim::Capability;
use campfire_values::{EngineEnum, TagProperty};
use rhai::Engine;

use crate::script_host::ScriptHost;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::hook::Hook;
use crate::scripts::script_api::api_member::ApiMember;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::api_reference::ApiReference;
use crate::scripts::script_api::data_field::DataField;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::enum_record::EnumRecord;
use crate::scripts::script_api::hook_status::HookStatus;
use crate::scripts::script_api::member_kind::MemberKind;
use crate::scripts::script_api::member_spec::{ArgKinds, MemberSpec, NameArgs};
use crate::scripts::script_api::status::Status;
use crate::scripts::script_api::tag_property_status::TagPropertyStatus;

pub(crate) mod api_member;
pub(crate) mod api_owner;
pub(crate) mod api_reference;
pub(crate) mod data_field;
pub(crate) mod data_table;
pub(crate) mod enum_record;
pub(crate) mod hook_status;
pub(crate) mod member_kind;
pub(crate) mod member_spec;
pub(crate) mod status;
pub(crate) mod tag_property_status;

/// The script API as the engine binds it: every name a script may use, each recorded by the
/// call that binds it, or planned, by design 08, and bound by no code yet. The load check and
/// the reference read it; nothing else lists the names.
#[derive(Debug)]
pub struct ScriptApi {
    /// Sorted by owner, then name.
    pub members: Vec<ApiMember>,
    /// Each hook, each tag property, and each data field: whether it runs.
    pub(super) hooks: Vec<HookStatus>,
    pub(super) tag_properties: Vec<TagPropertyStatus>,
    pub data: Vec<DataField>,
    /// The engine enums, in the order they bind.
    pub enums: Vec<EnumRecord>,
    /// The names of the functions the engine has before the API binds: Rhai's packages and
    /// `Num`'s, getters as `get$<field>`.
    builtins: BTreeSet<String>,
}

impl ScriptApi {
    /// The script API of the release, which `bind` binds into `host` as a match's engine binds
    /// it, recorded with the names the engine has before.
    pub fn release(host: &mut ScriptHost, bind: impl FnOnce(&mut ApiBuilder<'_>)) -> ScriptApi {
        let builtins = ScriptApi::functions(host.engine_mut());
        let mut api = ScriptApi::bind(host, bind);
        api.builtins = builtins;
        api
    }

    /// The names of every function `engine` has.
    fn functions(engine: &Engine) -> BTreeSet<String> {
        let names =
            engine.collect_fn_metadata(None, |info| Some(info.metadata.name.to_string()), true);
        names.into_iter().collect()
    }

    /// Binds into `host` the script API that `bind` binds, and records it.
    pub fn bind(host: &mut ScriptHost, bind: impl FnOnce(&mut ApiBuilder<'_>)) -> ScriptApi {
        let mut api = ScriptApi {
            members: Vec::new(),
            hooks: Vec::new(),
            tag_properties: Vec::new(),
            data: Vec::new(),
            enums: Vec::new(),
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

    /// Whether the engine has a function `name` of its own, before the API: `get$<field>` for a
    /// field.
    pub fn builtin(&self, name: &str) -> bool {
        self.builtins.contains(name)
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
                self.record_field(table, name, status, capability);
            }
        }
    }

    /// Records the field `name` of `table`, of `status`.
    pub fn record_field(
        &mut self,
        table: DataTable,
        name: &'static str,
        status: Status,
        capability: Option<Capability>,
    ) {
        assert!(
            self.data
                .iter()
                .all(|held| (held.table, held.name) != (table, name)),
            "{table:?}.{name} is recorded once"
        );
        self.data.push(DataField {
            table,
            name,
            status,
            capability,
        });
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

    /// What the arguments of the method `name` of any handle name: every handle with a method
    /// of that name gives the same, so a script's literal is checked whatever value it calls it
    /// on.
    pub fn method_names(&self, name: &str) -> Option<NameArgs> {
        self.members
            .iter()
            .find(|member| {
                let spec = &member.spec;
                spec.owner != ApiOwner::Ctx && spec.kind == MemberKind::Method && spec.name == name
            })
            .map(|member| member.spec.names)
    }

    pub fn members(&self) -> &[ApiMember] {
        &self.members
    }

    /// Records a binding of `spec`, written when `writable`, with `status`; another binding of a
    /// name already recorded adds its writing and what its form's arguments name, and gives the
    /// same spec and status otherwise.
    pub fn record(&mut self, spec: MemberSpec, writable: bool, status: Status) {
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
                held.spec = held.spec.with_args(args);
                held.writable |= writable;
            }
            Err(at) => self.members.insert(
                at,
                ApiMember {
                    spec,
                    writable,
                    status,
                },
            ),
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
