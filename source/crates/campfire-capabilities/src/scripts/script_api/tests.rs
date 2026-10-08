use std::any::TypeId;
use std::cell::RefCell;
use std::error::Error;
use std::num::NonZeroU64;
use std::panic::{self, AssertUnwindSafe};
use std::{env, fs};

use serde::de::{self, Deserialize, Deserializer, Visitor};

use super::*;
use crate::actions::action_data::ActionData;
use crate::actions::delivery_data::DeliveryData;
use crate::actions::effect_data::EffectData;
use crate::actions::requires_data::RequiresData;
use crate::actions::slot_kinds::SlotKindData;
use crate::areas::area_data::{AreaData, AreaInside};
use crate::capability_set::CapabilitySet;
use crate::combat::combat_data::CombatData;
use crate::combat::combat_rules::{CombatRules, Leech};
use crate::items::inventory_data::InventoryData;
use crate::items::item_data::ItemData;
use crate::items::shop_data::ShopData;
use crate::mode::choice_data::ChoiceData;
use crate::mode::mode_data::ModeData;
use crate::mode::relation_data::RelationData;
use crate::navigation::navigation_rules::NavigationRules;
use crate::orders::ai_data::AiData;
use crate::production::drop_off_data::DropOffData;
use crate::production::node_data::NodeData;
use crate::production::production_data::ProductionData;
use crate::production::supply_data::SupplyData;
use crate::production::supply_rules::SupplyRules;
use crate::progression::track_data::TrackData;
use crate::projectiles::projectile_data::ProjectileData;
use crate::scripts::name_kind::NameKind;
use crate::stats::modifier_data::{AuraData, ModifierData};
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
    let mut host = ScriptHost::new(NonZeroU64::MIN);
    let before = functions(host.engine_mut());
    let api = ScriptApi::bind(&mut host, CapabilitySet::apis());
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
        .filter(|member| member.status == Status::Runs(ApiVersion::FIRST))
    {
        match member.spec.kind {
            MemberKind::Value | MemberKind::Field => {
                recorded.push(format!("get${}", member.spec.name));
                if member.writable {
                    recorded.push(format!("set${}", member.spec.name));
                }
            }
            MemberKind::Call | MemberKind::Method | MemberKind::Operator => {
                recorded.push(member.spec.name.to_owned());
            }
        }
    }
    for _ in &api.enums {
        let functions = EnumRecord::FUNCTIONS
            .iter()
            .chain(&EnumRecord::MEMBER_FUNCTIONS);
        recorded.extend(functions.map(|&name| name.to_owned()));
    }
    recorded.sort_unstable();
    recorded.dedup();
    assert_eq!(bound, recorded);
}

#[test]
fn every_hook_and_state_has_a_status_and_names_hold_their_roles() {
    let api = CapabilitySet::script_api();
    let hooks: Vec<_> = api.hooks().iter().map(|status| status.hook).collect();
    assert!(Hook::ALL.iter().all(|hook| hooks.contains(hook)));
    assert_eq!(hooks.len(), Hook::ALL.len());
    let effects: Vec<_> = api
        .tag_properties()
        .iter()
        .map(|status| status.property)
        .collect();
    assert!(
        TagProperty::ALL
            .iter()
            .all(|effect| effects.contains(effect))
    );
    assert_eq!(effects.len(), TagProperty::ALL.len());
    let damage = api.member(ApiOwner::Ctx, "damage").unwrap();
    assert_eq!(
        (damage.spec.roles, damage.spec.capability, damage.status),
        (
            RoleSet::ALL,
            Some(Capability::Combat),
            Status::Runs(ApiVersion::FIRST)
        )
    );
    assert_eq!(damage.spec.forms, [["target", "amount", "kind"]]);
    let timer = api.member(ApiOwner::Ctx, "timer").unwrap();
    assert_eq!(timer.spec.roles, RoleSet::MODE);
    let stacks = api.member(ApiOwner::Modifier, "stacks").unwrap();
    assert!(stacks.writable);
    let points = api.member(ApiOwner::Unit, "points").unwrap();
    assert_eq!(
        (points.spec.capability, points.status),
        (
            Some(Capability::Progression),
            Status::Runs(ApiVersion::FIRST)
        )
    );
    let perk = api.member(ApiOwner::Unit, "has_perk").unwrap();
    assert_eq!(
        (perk.spec.capability, perk.status),
        (Some(Capability::Progression), Status::Planned)
    );
    assert!(api.builtin("len") && api.builtin("max") && !api.builtin("pos"));
}

#[test]
fn a_binding_records_its_spec_once_and_whole() {
    let mut api = CapabilitySet::script_api();
    let stacks = api.member(ApiOwner::Modifier, "stacks").unwrap().clone();
    let damage = api.member(ApiOwner::Ctx, "damage").unwrap().spec;
    // Another binding of the same spec adds only its writing.
    api.record(damage, true, Status::Runs(ApiVersion::FIRST));
    let held = api.member(ApiOwner::Ctx, "damage").unwrap();
    assert_eq!((held.spec, held.writable), (damage, true));
    api.record(stacks.spec, false, stacks.status);
    assert!(api.member(ApiOwner::Modifier, "stacks").unwrap().writable);
    let misuses: [(fn(&mut ScriptApi), &str); 4] = [
        (
            |api| {
                let damage = api.member(ApiOwner::Ctx, "damage").unwrap().spec;
                let spec = MemberSpec {
                    description: "another",
                    ..damage
                };
                api.record(spec, false, Status::Runs(ApiVersion::FIRST));
            },
            "the bindings of Ctx.damage agree",
        ),
        (
            |api| {
                let damage = api.member(ApiOwner::Ctx, "damage").unwrap().spec;
                api.record(damage, false, Status::Planned);
            },
            "the bindings of Ctx.damage agree",
        ),
        (
            |api| {
                let spec = MemberSpec::call("named", &[&["unit"]], "a test call")
                    .name(1, NameKind::Modifier);
                api.record(spec, false, Status::Planned);
            },
            "Ctx.named: argument 1, which names something, is in its first form",
        ),
        (
            |api| {
                let field = api.data[0];
                api.record_field(field.table, field.name, field.status);
            },
            "is recorded once",
        ),
    ];
    for (misuse, message) in misuses {
        let mut api = CapabilitySet::script_api();
        let panic = panic::catch_unwind(AssertUnwindSafe(|| misuse(&mut api))).unwrap_err();
        let said = panic.downcast_ref::<String>().unwrap();
        assert!(said.contains(message), "{said}");
    }
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
#[derive(Debug)]
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
    "/../../../docs/design/08-script-api-reference.md"
);

#[test]
fn the_reference_is_what_the_registry_writes() {
    let mut api = CapabilitySet::script_api();
    let written = api.reference();
    if env::var_os("CAMPFIRE_BLESS").is_some() {
        fs::write(REFERENCE, &written).unwrap();
    }
    let held = fs::read_to_string(REFERENCE).unwrap();
    assert!(
        held == written,
        "the reference differs from the registry: run this test with CAMPFIRE_BLESS=1"
    );
    api.members[0].spec.description = "another description";
    assert_ne!(api.reference(), held);
    // A script's literal is checked by a method's name alone, so every handle's method of a
    // name marks the same names.
    let methods = api.members.iter().filter(|member| {
        member.spec.owner != ApiOwner::Ctx && member.spec.kind == MemberKind::Method
    });
    for member in methods {
        assert_eq!(
            api.method_names(member.spec.name),
            Some(member.spec.names),
            "{}",
            member.spec.name
        );
    }
}

#[test]
fn the_data_fields_are_the_schemas() {
    let api = CapabilitySet::script_api();
    // The mode's file holds its package's actions, modifiers and item types beside `ModeData`,
    // which the package load reads apart.
    let mut mode = serde_fields::<ModeData>();
    mode.extend(["actions", "modifiers", "items"]);
    mode.sort_unstable();
    let tables = [
        (DataTable::Mode, mode),
        (DataTable::ModeCombat, serde_fields::<CombatRules>()),
        (DataTable::ModeNavigation, serde_fields::<NavigationRules>()),
        (DataTable::SlotKind, serde_fields::<SlotKindData>()),
        (DataTable::Choice, serde_fields::<ChoiceData>()),
        (DataTable::Leech, serde_fields::<Leech>()),
        (DataTable::Relation, serde_fields::<RelationData>()),
        (DataTable::Action, serde_fields::<ActionData>()),
        (DataTable::Effect, serde_fields::<EffectData>()),
        (DataTable::Delivery, serde_fields::<DeliveryData>()),
        (DataTable::Projectile, serde_fields::<ProjectileData>()),
        (DataTable::Area, serde_fields::<AreaData>()),
        (DataTable::AreaInside, serde_fields::<AreaInside>()),
        (DataTable::Track, serde_fields::<TrackData>()),
        (DataTable::Modifier, serde_fields::<ModifierData>()),
        (DataTable::Aura, serde_fields::<AuraData>()),
        (DataTable::Combat, serde_fields::<CombatData>()),
        (DataTable::Production, serde_fields::<ProductionData>()),
        (DataTable::Supply, serde_fields::<SupplyData>()),
        (DataTable::ModeSupply, serde_fields::<SupplyRules>()),
        (DataTable::Requires, serde_fields::<RequiresData>()),
        (DataTable::Node, serde_fields::<NodeData>()),
        (DataTable::DropOff, serde_fields::<DropOffData>()),
        (DataTable::Vision, serde_fields::<VisionData>()),
        (DataTable::Collision, serde_fields::<CollisionData>()),
        (DataTable::Ai, serde_fields::<AiData>()),
        (DataTable::Tag, serde_fields::<TagData>()),
        (DataTable::Item, serde_fields::<ItemData>()),
        (DataTable::Inventory, serde_fields::<InventoryData>()),
        (DataTable::Shop, serde_fields::<ShopData>()),
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
