use std::fmt::{self, Write};

use campfire_script::ScriptHost;
use campfire_script::rhai::Engine;
use campfire_sim::Capability;

use crate::abilities::abilities_api::AbilitiesApi;
use crate::combat::combat_api::CombatApi;
use crate::mode::mode_api::ModeApi;
use crate::orders::orders_api::OrdersApi;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::core_api::CoreApi;
use crate::scripts::hook::{Hook, ScriptRole};
use crate::scripts::role_set::RoleSet;
use crate::stats::stats_api::StatsApi;
use crate::units::script_view::View;
use crate::units::tag_effect::TagEffect;
use crate::units::unit::Unit;
use crate::vision::vision_api::VisionApi;

/// The script API as the engine binds it: every name a script may use, each recorded by the
/// call that binds it, or planned, by design 08, and bound by no code yet. The load check and
/// the reference read it; nothing else lists the names.
#[derive(Debug)]
pub struct ScriptApi {
    /// Sorted by owner, then name.
    members: Vec<ApiMember>,
    /// Each hook, each unit state, and each data field: whether it runs.
    hooks: Vec<HookStatus>,
    tag_effects: Vec<TagEffectStatus>,
    data: Vec<DataField>,
    /// The names of the functions the engine has before the API binds: Rhai's packages and
    /// `Num`'s, getters as `get$<field>`; sorted.
    builtins: Vec<String>,
}

/// Whether the release calls a hook, and its parameters' names, as `(ctx, m, d)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookStatus {
    pub hook: Hook,
    pub signature: &'static str,
    pub status: Status,
}

/// Whether the release honours an effect a tag may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagEffectStatus {
    pub effect: TagEffect,
    pub status: Status,
}

/// A field of a data file's table, by its name in the file: whether the release reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataField {
    pub table: DataTable,
    pub name: &'static str,
    pub status: Status,
}

/// A table of the data files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DataTable {
    Mode,
    ModeCombat,
    ModeNavigation,
    SlotKind,
    Choice,
    Leech,
    Relation,
    Action,
    Modifier,
    Aura,
    Combat,
    Attack,
    Vision,
    Collision,
    Ai,
    Tag,
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
}

/// What a script holds a name on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiOwner {
    Ctx,
    Unit,
    Modifier,
    Projectile,
    Area,
    Hit,
    Damage,
    Position,
    Vector,
    GameMap,
    Marker,
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

/// Whether the release runs a name, or design 08 plans it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Runs,
    Planned,
}

/// A name as the code that binds it describes it: an `ApiMember` with one form.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberSpec {
    pub owner: ApiOwner,
    pub name: &'static str,
    pub kind: MemberKind,
    pub roles: RoleSet,
    pub capability: Option<Capability>,
    pub signature: &'static str,
    pub description: &'static str,
}

impl ScriptApi {
    /// The script API of the release, recorded as a match's engine binds it, with the names
    /// the engine has before.
    pub fn release() -> ScriptApi {
        let mut host = ScriptHost::new(1);
        let builtins = ScriptApi::functions(host.engine_mut());
        let mut api = ScriptApi::bind(host.engine_mut());
        api.builtins = builtins;
        api
    }

    /// The names of every function `engine` has, sorted, without repeats.
    fn functions(engine: &Engine) -> Vec<String> {
        let mut names =
            engine.collect_fn_metadata(None, |info| Some(info.metadata.name.to_string()), true);
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Binds the whole script API into `engine`, and records it.
    pub(crate) fn bind(engine: &mut Engine) -> ScriptApi {
        let mut api = ScriptApi {
            members: Vec::new(),
            hooks: Vec::new(),
            tag_effects: Vec::new(),
            data: Vec::new(),
            builtins: Vec::new(),
        };
        let mut builder = ApiBuilder::new(engine, &mut api);
        CoreApi::register(&mut builder);
        Unit::register(&mut builder);
        View::register_queries(&mut builder);
        ModeApi::register(&mut builder);
        CombatApi::register(&mut builder);
        StatsApi::register(&mut builder);
        AbilitiesApi::register(&mut builder);
        OrdersApi::register(&mut builder);
        VisionApi::register(&mut builder);
        api
    }

    pub fn hooks(&self) -> &[HookStatus] {
        &self.hooks
    }

    pub fn tag_effects(&self) -> &[TagEffectStatus] {
        &self.tag_effects
    }

    pub fn data(&self) -> &[DataField] {
        &self.data
    }

    /// Whether the engine has a function `name` of its own, before the API: `get$<field>` for a
    /// field.
    pub fn builtin(&self, name: &str) -> bool {
        self.builtins
            .binary_search_by(|held| held.as_str().cmp(name))
            .is_ok()
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
                    (MemberKind::Call | MemberKind::Method, _) => member
                        .signatures
                        .iter()
                        .map(|signature| format!("`{signature}`"))
                        .collect::<Vec<_>>()
                        .join(" "),
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
                    member.status.name(),
                    member.description,
                )?;
            }
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
                "| `{}{}` | {} | {} | {} |",
                hook.name(),
                status.signature,
                hook.role().name(),
                capability(hook.capability()),
                status.status.name(),
            )?;
        }
        out.push_str("\n## Tag effects\n\n| Effect | Status |\n| --- | --- |\n");
        for status in &self.tag_effects {
            writeln!(
                out,
                "| `{}` | {} |",
                status.effect.name(),
                status.status.name()
            )?;
        }
        out.push_str("\n## Data fields\n");
        for table in DataTable::ALL {
            writeln!(
                out,
                "\n### {}\n\n| Field | Status |\n| --- | --- |",
                table.title()
            )?;
            for field in self.data.iter().filter(|field| field.table == table) {
                writeln!(out, "| `{}` | {} |", field.name, field.status.name())?;
            }
        }
        Ok(())
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

    /// Records whether the release honours `effect`.
    pub(crate) fn record_tag_effect(&mut self, effect: TagEffect, status: Status) {
        assert!(
            self.tag_effects.iter().all(|held| held.effect != effect),
            "{effect:?} is recorded once"
        );
        self.tag_effects.push(TagEffectStatus { effect, status });
    }

    /// Records the fields of `table`: `runs`, which the release reads, and `planned`.
    pub(crate) fn record_data(
        &mut self,
        table: DataTable,
        runs: &[&'static str],
        planned: &[&'static str],
    ) {
        for (names, status) in [(runs, Status::Runs), (planned, Status::Planned)] {
            self.data.extend(names.iter().map(|&name| DataField {
                table,
                name,
                status,
            }));
        }
    }

    /// The member `name` of `owner`.
    pub fn member(&self, owner: ApiOwner, name: &str) -> Option<&ApiMember> {
        let at = self
            .members
            .binary_search_by(|member| (member.owner, member.name).cmp(&(owner, name)))
            .ok()?;
        Some(&self.members[at])
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
                    (held.kind, held.roles, held.capability, held.status)
                        == (spec.kind, spec.roles, spec.capability, status),
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
                },
            ),
        }
    }
}

impl ApiOwner {
    pub const ALL: [ApiOwner; 11] = [
        ApiOwner::Ctx,
        ApiOwner::Unit,
        ApiOwner::Modifier,
        ApiOwner::Projectile,
        ApiOwner::Area,
        ApiOwner::Hit,
        ApiOwner::Damage,
        ApiOwner::Position,
        ApiOwner::Vector,
        ApiOwner::GameMap,
        ApiOwner::Marker,
    ];

    /// The owner as the reference titles it.
    pub const fn title(self) -> &'static str {
        match self {
            ApiOwner::Ctx => "`ctx`",
            ApiOwner::Unit => "Unit",
            ApiOwner::Modifier => "Modifier `m`",
            ApiOwner::Projectile => "Projectile",
            ApiOwner::Area => "Area",
            ApiOwner::Hit => "Hit `hit`",
            ApiOwner::Damage => "Damage `d`",
            ApiOwner::Position => "Position",
            ApiOwner::Vector => "Vector",
            ApiOwner::GameMap => "Map, `ctx.map`",
            ApiOwner::Marker => "Marker, of `ctx.map.markers(tag)`",
        }
    }
}

impl Status {
    pub const fn name(self) -> &'static str {
        match self {
            Status::Runs => "runs",
            Status::Planned => "planned",
        }
    }
}

impl DataTable {
    pub const ALL: [DataTable; 16] = [
        DataTable::Mode,
        DataTable::ModeCombat,
        DataTable::ModeNavigation,
        DataTable::SlotKind,
        DataTable::Choice,
        DataTable::Leech,
        DataTable::Relation,
        DataTable::Action,
        DataTable::Modifier,
        DataTable::Aura,
        DataTable::Combat,
        DataTable::Attack,
        DataTable::Vision,
        DataTable::Collision,
        DataTable::Ai,
        DataTable::Tag,
    ];

    /// The table as the reference titles it.
    pub const fn title(self) -> &'static str {
        match self {
            DataTable::Mode => "`data/mode.toml`",
            DataTable::ModeCombat => "The mode's `[combat]`",
            DataTable::ModeNavigation => "The mode's `[navigation]`",
            DataTable::SlotKind => "A slot kind, `[[slots]]`",
            DataTable::Choice => "A choice, `[choices.<name>]`",
            DataTable::Leech => "The mode's `[combat] leech`",
            DataTable::Relation => "A pair of teams, `[[relations]]`",
            DataTable::Action => "An action, `[actions.<id>]`",
            DataTable::Modifier => "A modifier, `[modifiers.<id>]`",
            DataTable::Aura => "A modifier's `aura`",
            DataTable::Combat => "A unit type's `combat`",
            DataTable::Attack => "A unit type's `combat.attack`",
            DataTable::Vision => "A unit type's `vision`",
            DataTable::Collision => "A unit type's `collision`",
            DataTable::Ai => "A unit type's `orders`",
            DataTable::Tag => "A tag's effects, `[tags.<name>]`",
        }
    }
}

impl MemberSpec {
    /// A value of `ctx`, for every role and of the core until said otherwise.
    pub const fn value(name: &'static str, description: &'static str) -> MemberSpec {
        MemberSpec::new(ApiOwner::Ctx, name, MemberKind::Value, "", description)
    }

    /// A call of `ctx`, in the form `signature`.
    pub const fn call(
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
    pub const fn field(
        owner: ApiOwner,
        name: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(owner, name, MemberKind::Field, "", description)
    }

    /// A method of `owner`'s handle, in the form `signature`.
    pub const fn method(
        owner: ApiOwner,
        name: &'static str,
        signature: &'static str,
        description: &'static str,
    ) -> MemberSpec {
        MemberSpec::new(owner, name, MemberKind::Method, signature, description)
    }

    /// The operator `name` on `owner`'s handles.
    pub const fn operator(
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
        }
    }

    /// The same, for `roles` only.
    pub const fn roles(mut self, roles: RoleSet) -> MemberSpec {
        self.roles = roles;
        self
    }

    /// The same, of `capability`.
    pub const fn capability(mut self, capability: Capability) -> MemberSpec {
        self.capability = Some(capability);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::cell::RefCell;
    use std::error::Error;
    use std::{env, fs};

    use serde::de::value::{Error as ValueError, StrDeserializer};
    use serde::de::{self, Deserialize, Deserializer, Visitor};

    use super::*;
    use crate::abilities::action_data::ActionData;
    use crate::abilities::slot_kinds::SlotKindData;
    use crate::combat::combat_data::{AttackData, CombatData};
    use crate::combat::combat_rules::{CombatRules, Leech};
    use crate::mode::choice_data::ChoiceData;
    use crate::mode::mode_data::ModeData;
    use crate::mode::relation_data::RelationData;
    use crate::navigation::navigation_rules::NavigationRules;
    use crate::orders::ai_data::AiData;
    use crate::stats::modifier_data::{AuraData, ModifierData};
    use crate::units::block::Block;
    use crate::units::collision_data::CollisionData;
    use crate::units::tag_data::TagData;
    use crate::vision::vision_data::VisionData;

    /// Each function of `engine`: its name, and the type of its first parameter.
    fn functions(engine: &Engine) -> Vec<(String, Option<TypeId>)> {
        let mut functions = engine.collect_fn_metadata(
            None,
            |info| {
                let first = info.metadata.param_types.first().copied();
                Some((info.metadata.name.to_string(), first))
            },
            true,
        );
        functions.sort_unstable();
        functions.dedup();
        functions
    }

    #[test]
    fn the_registry_holds_exactly_what_the_engine_binds() {
        let mut host = ScriptHost::new(1);
        let before = functions(host.engine_mut());
        let api = ScriptApi::bind(host.engine_mut());
        let mut bound: Vec<String> = functions(host.engine_mut())
            .into_iter()
            .filter(|function| !before.contains(function))
            .map(|(name, _)| name)
            .collect();
        bound.dedup();
        let mut recorded: Vec<String> = ["index$get$".to_owned(), "index$set$".to_owned()].into();
        for member in api
            .members()
            .iter()
            .filter(|member| member.status == Status::Runs)
        {
            match member.kind {
                MemberKind::Value | MemberKind::Field => {
                    recorded.push(format!("get${}", member.name));
                    if member.writable {
                        recorded.push(format!("set${}", member.name));
                    }
                }
                MemberKind::Call | MemberKind::Method | MemberKind::Operator => {
                    recorded.push(member.name.to_owned());
                }
            }
        }
        recorded.sort_unstable();
        recorded.dedup();
        assert_eq!(bound, recorded);
    }

    #[test]
    fn every_hook_and_state_has_a_status_and_names_hold_their_roles() {
        let api = ScriptApi::release();
        let hooks: Vec<_> = api.hooks().iter().map(|status| status.hook).collect();
        assert!(Hook::ALL.iter().all(|hook| hooks.contains(hook)));
        assert_eq!(hooks.len(), Hook::ALL.len());
        let effects: Vec<_> = api
            .tag_effects()
            .iter()
            .map(|status| status.effect)
            .collect();
        assert!(TagEffect::ALL.iter().all(|effect| effects.contains(effect)));
        assert_eq!(effects.len(), TagEffect::ALL.len());
        let damage = api.member(ApiOwner::Ctx, "damage").unwrap();
        assert_eq!(
            (damage.roles, damage.capability, damage.status),
            (RoleSet::ALL, Some(Capability::Combat), Status::Runs)
        );
        assert_eq!(damage.signatures, ["(target, amount, kind)"]);
        let timer = api.member(ApiOwner::Ctx, "timer").unwrap();
        assert_eq!(timer.roles, RoleSet::MODE);
        let stacks = api.member(ApiOwner::Modifier, "stacks").unwrap();
        assert!(stacks.writable);
        let spawn_pos = api.member(ApiOwner::Unit, "spawn_pos").unwrap();
        assert_eq!(spawn_pos.status, Status::Planned);
        assert!(api.builtin("len") && api.builtin("max") && !api.builtin("pos"));
    }

    /// The names serde reads for `T`'s fields, as its derive or its own impl gives them.
    fn serde_fields<'de, T: Deserialize<'de>>() -> Vec<&'static str> {
        let fields = RefCell::new(Vec::new());
        let read = T::deserialize(FieldNames(&fields));
        assert!(read.is_err(), "a field name reader reads no value");
        let mut fields = fields.into_inner();
        fields.sort_unstable();
        fields
    }

    /// A deserializer that reads only the field names of the struct it is asked for.
    struct FieldNames<'a>(&'a RefCell<Vec<&'static str>>);

    #[derive(Debug)]
    struct Read;

    impl fmt::Display for Read {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("read")
        }
    }

    impl Error for Read {}

    impl de::Error for Read {
        fn custom<T: fmt::Display>(_: T) -> Read {
            Read
        }
    }

    impl<'de> Deserializer<'de> for FieldNames<'_> {
        type Error = Read;

        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Read> {
            Err(Read)
        }

        fn deserialize_struct<V: Visitor<'de>>(
            self,
            _: &'static str,
            fields: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Read> {
            self.0.borrow_mut().extend_from_slice(fields);
            Err(Read)
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
            byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map enum
            identifier ignored_any
        }
    }

    /// The reference beside design 08.
    const REFERENCE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../design/08-script-api-reference.md"
    );

    #[test]
    fn the_reference_is_what_the_registry_writes() {
        let mut api = ScriptApi::release();
        let written = api.reference();
        if env::var_os("CAMPFIRE_BLESS").is_some() {
            fs::write(REFERENCE, &written).unwrap();
        }
        let held = fs::read_to_string(REFERENCE).unwrap();
        assert!(
            held == written,
            "the reference differs from the registry: run this test with CAMPFIRE_BLESS=1"
        );
        api.members[0].description = "another description";
        assert_ne!(api.reference(), held);
        for status in api.hooks() {
            let named = status.signature.split(',').count();
            assert_eq!(named, status.hook.params(), "{:?}", status.hook);
        }
        for block in Block::ALL {
            let name = StrDeserializer::<ValueError>::new(block.name());
            assert_eq!(Block::deserialize(name), Ok(block));
        }
    }

    #[test]
    fn the_data_fields_are_the_schemas() {
        let api = ScriptApi::release();
        let tables = [
            (DataTable::Mode, serde_fields::<ModeData>()),
            (DataTable::ModeCombat, serde_fields::<CombatRules>()),
            (DataTable::ModeNavigation, serde_fields::<NavigationRules>()),
            (DataTable::SlotKind, serde_fields::<SlotKindData>()),
            (DataTable::Choice, serde_fields::<ChoiceData>()),
            (DataTable::Leech, serde_fields::<Leech>()),
            (DataTable::Relation, serde_fields::<RelationData>()),
            (DataTable::Action, serde_fields::<ActionData>()),
            (DataTable::Modifier, serde_fields::<ModifierData>()),
            (DataTable::Aura, serde_fields::<AuraData>()),
            (DataTable::Combat, serde_fields::<CombatData>()),
            (DataTable::Attack, serde_fields::<AttackData>()),
            (DataTable::Vision, serde_fields::<VisionData>()),
            (DataTable::Collision, serde_fields::<CollisionData>()),
            (DataTable::Ai, serde_fields::<AiData>()),
            (DataTable::Tag, serde_fields::<TagData>()),
        ];
        for (table, schema) in tables {
            let mut recorded: Vec<_> = api
                .data()
                .iter()
                .filter(|field| field.table == table)
                .map(|field| field.name)
                .collect();
            recorded.sort_unstable();
            assert_eq!(recorded, schema, "{table:?}");
        }
    }
}
