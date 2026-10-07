use std::fmt::{self, Write};

use campfire_script::ScriptHost;
use campfire_script::rhai::Engine;
use campfire_sim::Capability;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::applies::Applies;
use crate::scripts::core_api::CoreApi;
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::enum_record::EnumRecord;
use crate::scripts::script_api::member_spec::{EnumArgs, MemberSpec, NameArgs};
use crate::scripts::script_api::status::Status;
use crate::scripts::script_role::ScriptRole;
use crate::units::new_unit::NewUnit;
use crate::units::script_view::View;
use crate::units::tag_property::TagProperty;
use crate::units::unit::Unit;
use crate::units::units_api::UnitsApi;
use crate::values::engine_enum::EngineEnum;
use crate::values::name_list::NameList;

pub(crate) mod api_owner;
pub(crate) mod data_table;
pub(crate) mod enum_record;
pub(crate) mod member_spec;
pub(crate) mod status;

/// The script API as the engine binds it: every name a script may use, each recorded by the
/// call that binds it, or planned, by design 08, and bound by no code yet. The load check and
/// the reference read it; nothing else lists the names.
#[derive(Debug)]
pub struct ScriptApi {
    /// Sorted by owner, then name.
    members: Vec<ApiMember>,
    /// Each hook, each tag property, and each data field: whether it runs.
    hooks: Vec<HookStatus>,
    tag_properties: Vec<TagPropertyStatus>,
    data: Vec<DataField>,
    /// The engine enums, in the order they bind.
    enums: Vec<EnumRecord>,
    /// The names of the functions the engine has before the API binds: Rhai's packages and
    /// `Num`'s, getters as `get$<field>`; sorted.
    builtins: NameList,
}

/// Whether the release calls a hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookStatus {
    pub hook: Hook,
    pub status: Status,
}

/// Whether the release honours a property a tag may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagPropertyStatus {
    pub property: TagProperty,
    pub status: Status,
}

/// A field of a data file's table, by its name in the file: whether the release reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataField {
    pub table: DataTable,
    pub name: &'static str,
    pub status: Status,
}

/// The opening of the generated reference.
const REFERENCE_HEAD: &str = "# Campfire — Script API reference

Generated from the script API's registry ([One source](08-script-api.md#one-source)); do not edit it. A test fails when it differs from what the registry writes; run that test with `CAMPFIRE_BLESS=1` to write it again. A name that runs is bound by the code that runs it; a planned one is one design 08 gives that the release does not run yet. The rules of the API are [design 08](08-script-api.md).
";

/// A name of the script API: whose it is, what it is, who may use it, and whether it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiMember {
    pub owner: ApiOwner,
    pub name: &'static str,
    pub kind: MemberKind,
    pub roles: RoleSet,
    pub capability: Option<Capability>,
    /// Each form it is called in, as `(target, amount, kind)`; empty for a value or field.
    pub signatures: Vec<&'static str>,
    pub description: &'static str,
    /// Whether a script may write it, as `m.stacks`.
    pub writable: bool,
    pub status: Status,
    /// What each of its arguments names, by place, `None` for one that names nothing.
    pub names: NameArgs,
    /// Which engine enum each of its arguments takes, by place.
    pub enums: EnumArgs,
    /// How its argument that names a modifier applies it; none for one that only names it.
    pub applies: Option<Applies>,
}

/// How a script uses a name: reads a value of `ctx`, calls `ctx`, reads a handle's field, calls
/// a handle's method, or applies an operator to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Value,
    Call,
    Field,
    Method,
    Operator,
}

impl ScriptApi {
    /// The script API of the release, with each capability's API `apis` registers, bound into
    /// `host` as a match's engine binds it, and recorded with the names the engine has before.
    pub(crate) fn release(
        host: &mut ScriptHost,
        apis: impl IntoIterator<Item = fn(&mut ApiBuilder<'_>)>,
    ) -> ScriptApi {
        let builtins = ScriptApi::functions(host.engine_mut());
        let mut api = ScriptApi::bind(host, apis);
        api.builtins = builtins;
        api
    }

    /// The names of every function `engine` has, sorted, without repeats.
    fn functions(engine: &Engine) -> NameList {
        let mut names =
            engine.collect_fn_metadata(None, |info| Some(info.metadata.name.to_string()), true);
        names.sort_unstable();
        names.dedup();
        names.iter().map(String::as_str).collect()
    }

    /// Binds the core's script API into `host`, then each capability's API `apis` registers,
    /// and records it.
    pub(crate) fn bind(
        host: &mut ScriptHost,
        apis: impl IntoIterator<Item = fn(&mut ApiBuilder<'_>)>,
    ) -> ScriptApi {
        let mut api = ScriptApi {
            members: Vec::new(),
            hooks: Vec::new(),
            tag_properties: Vec::new(),
            data: Vec::new(),
            enums: Vec::new(),
            builtins: NameList::default(),
        };
        let mut builder = ApiBuilder::new(host, &mut api);
        CoreApi::register(&mut builder);
        Unit::register(&mut builder);
        NewUnit::register(&mut builder);
        UnitsApi::register(&mut builder);
        View::register_queries(&mut builder);
        for register in apis {
            register(&mut builder);
        }
        api
    }

    pub fn hooks(&self) -> &[HookStatus] {
        &self.hooks
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
        self.builtins
            .sorted_named(0..self.builtins.len(), name)
            .is_some()
    }

    /// The script API's reference, in Markdown: every name, hook, state and data field, with
    /// whether the release runs it.
    pub fn reference(&self) -> String {
        let mut out = String::from(REFERENCE_HEAD);
        self.write_reference(&mut out)
            .expect("a string takes any text");
        out
    }

    fn write_reference(&self, out: &mut String) -> fmt::Result {
        let roles = |roles: RoleSet| {
            if roles == RoleSet::ALL {
                "every role".to_owned()
            } else {
                let names: Vec<_> = roles.iter().map(ScriptRole::name).collect();
                names.join(", ")
            }
        };
        let capability =
            |capability: Option<Capability>| capability.map_or("core", Capability::name);
        for owner in ApiOwner::ALL {
            let members = self.members.iter().filter(|member| member.owner == owner);
            let ctx = owner == ApiOwner::Ctx;
            write!(out, "\n## {}\n\n", owner.title())?;
            out.push_str(if ctx {
                "| Name | Form | Roles | Capability | Status | What it is |\n| --- | --- | --- | --- | --- | --- |\n"
            } else {
                "| Name | Form | Capability | Status | What it is |\n| --- | --- | --- | --- | --- |\n"
            });
            for member in members {
                let form = match (member.kind, member.writable) {
                    (MemberKind::Field | MemberKind::Value, true) => "written and read".to_owned(),
                    (MemberKind::Field | MemberKind::Value, false) => "read".to_owned(),
                    (MemberKind::Operator, _) => "operator".to_owned(),
                    (MemberKind::Call | MemberKind::Method, _) => {
                        let forms: Vec<_> = member
                            .signatures
                            .iter()
                            .map(|signature| format!("`{signature}`"))
                            .collect();
                        forms.join(" ") + member.name_args_text().as_str()
                    }
                };
                let roles = if ctx {
                    format!(" {} |", roles(member.roles))
                } else {
                    String::new()
                };
                writeln!(
                    out,
                    "| `{}` | {form} |{roles} {} | {} | {} |",
                    member.name,
                    capability(member.capability),
                    member.status,
                    member.description,
                )?;
            }
        }
        out.push_str(
            "\n## Engine enums\n\nEach enum's module holds its members, and the function `named`, which gives the member a text names as data does; a member has `==`, `!=` and `to_string`, its name in data.\n\n| Enum | Members |\n| --- | --- |\n",
        );
        for record in &self.enums {
            let members: Vec<_> = record
                .members
                .iter()
                .map(|member| format!("`{member}`"))
                .collect();
            writeln!(out, "| `{}` | {} |", record.engine_enum, members.join(", "))?;
        }
        out.push_str(
            "\n## Hooks\n\n| Hook | Role | Capability | Status |\n| --- | --- | --- | --- |\n",
        );
        for status in Hook::ALL
            .iter()
            .filter_map(|&hook| self.hooks.iter().find(|status| status.hook == hook))
        {
            let hook = status.hook;
            writeln!(
                out,
                "| `{}({})` | {} | {} | {} |",
                hook.name(),
                hook.param_names().join(", "),
                hook.role().name(),
                capability(hook.capability()),
                status.status,
            )?;
        }
        out.push_str("\n## Tag properties\n\n| Property | Status |\n| --- | --- |\n");
        for status in &self.tag_properties {
            writeln!(out, "| `{}` | {} |", status.property.name(), status.status)?;
        }
        out.push_str("\n## Data fields\n");
        for table in DataTable::ALL {
            writeln!(
                out,
                "\n### {}\n\n| Field | Status |\n| --- | --- |",
                table.title()
            )?;
            for field in self.data.iter().filter(|field| field.table == table) {
                writeln!(out, "| `{}` | {} |", field.name, field.status)?;
            }
        }
        Ok(())
    }

    /// Records the engine enum `engine_enum`, with its `members` by their names in scripts.
    pub(crate) fn record_enum(&mut self, engine_enum: EngineEnum, members: Vec<&'static str>) {
        assert!(
            self.enums
                .iter()
                .all(|held| held.engine_enum != engine_enum),
            "{engine_enum:?} is recorded once"
        );
        self.enums.push(EnumRecord {
            engine_enum,
            members,
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
    ) {
        for (names, status) in [
            (runs, Status::Runs(ApiVersion::FIRST)),
            (planned, Status::Planned),
        ] {
            for &name in names {
                self.record_field(table, name, status);
            }
        }
    }

    /// Records the field `name` of `table`, of `status`.
    pub(crate) fn record_field(&mut self, table: DataTable, name: &'static str, status: Status) {
        self.data.push(DataField {
            table,
            name,
            status,
        });
    }

    /// The member `name` of `owner`.
    pub fn member(&self, owner: ApiOwner, name: &str) -> Option<&ApiMember> {
        let at = self
            .members
            .binary_search_by(|member| (member.owner, member.name).cmp(&(owner, name)))
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
                member.owner != ApiOwner::Ctx
                    && member.kind == MemberKind::Method
                    && member.name == name
            })
            .map(|member| member.names)
    }

    pub fn members(&self) -> &[ApiMember] {
        &self.members
    }

    /// Records a form of `spec`, written when `writable`, with `status`; a second form of a
    /// name already recorded adds its signature and its writing, and must agree on the rest.
    pub(crate) fn record(&mut self, spec: MemberSpec, writable: bool, status: Status) {
        let key = (spec.owner, spec.name);
        let signatures = (!spec.signature.is_empty()).then_some(spec.signature);
        match self
            .members
            .binary_search_by(|held| (held.owner, held.name).cmp(&key))
        {
            Ok(at) => {
                let held = &mut self.members[at];
                assert!(
                    (
                        held.kind,
                        held.roles,
                        held.capability,
                        held.status,
                        held.names,
                        held.enums
                    ) == (
                        spec.kind,
                        spec.roles,
                        spec.capability,
                        status,
                        spec.names,
                        spec.enums
                    ),
                    "the forms of {:?}.{} agree",
                    spec.owner,
                    spec.name
                );
                if let Some(signature) = signatures
                    && !held.signatures.contains(&signature)
                {
                    held.signatures.push(signature);
                }
                held.writable |= writable;
            }
            Err(at) => self.members.insert(
                at,
                ApiMember {
                    owner: spec.owner,
                    name: spec.name,
                    kind: spec.kind,
                    roles: spec.roles,
                    capability: spec.capability,
                    signatures: signatures.into_iter().collect(),
                    description: spec.description,
                    writable,
                    status,
                    names: spec.names,
                    enums: spec.enums,
                    applies: spec.applies,
                },
            ),
        }
    }
}

impl ApiMember {
    /// Its arguments that name something or take an engine enum, as the reference lists them
    /// after its forms: `, `id` a modifier`, by their names in its first form.
    fn name_args_text(&self) -> String {
        let Some(first) = self.signatures.first() else {
            return String::new();
        };
        let params = first
            .trim_start_matches('(')
            .split(')')
            .next()
            .unwrap_or_default();
        let params: Vec<&str> = params.split(", ").collect();
        let mut named = String::new();
        let param = |at: usize| {
            *params
                .get(at)
                .expect("a name role is within the first form")
        };
        for (at, kind) in self.names.iter().enumerate() {
            if let Some(kind) = kind {
                write!(named, ", `{}` a {kind}", param(at)).expect("text writes into a string");
            }
        }
        for (at, engine_enum) in self.enums.iter().enumerate() {
            if let Some(engine_enum) = engine_enum {
                write!(named, ", `{}` a `{engine_enum}`", param(at))
                    .expect("text writes into a string");
            }
        }
        named
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test writes the script API reference it generates from the registry"
)]
#[cfg(test)]
mod tests;
