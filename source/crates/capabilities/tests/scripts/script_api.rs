use std::any::TypeId;
use std::cell::RefCell;
use std::error::Error;
use std::num::NonZeroU64;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::{env, fmt};

use campfire_actions::{ActionData, DeliveryData, EffectData, RequiresData, SlotKindData};
use campfire_areas::{AreaData, AreaInsideData};
use campfire_capabilities::{Release, UnitTypeData};
use campfire_codec::Toml;
use campfire_combat::{CombatData, LeechData, ModeCombatData, TurretData};
use campfire_interaction::{UseData, UsesData};
use campfire_items::{InventoryData, ItemData, ShopData};
use campfire_math::Num;
use campfire_mode::{
    ChoiceData, ModeData, OptionData, PlayersData, RelationData, SavesData, TeamData,
};
use campfire_navigation::ModeNavigationData;
use campfire_orders::AiData;
use campfire_production::{DropOffData, ModeSupplyData, NodeData, ProductionData, SupplyData};
use campfire_progression::TrackData;
use campfire_projectiles::ProjectileData;
use campfire_script::*;
use campfire_sim::Capability;
use campfire_stats::{AuraData, ModifierData};
use campfire_store::InputFile;
use campfire_store::internals::PinnedFile;
use campfire_units::{CollisionData, TagData};
use campfire_values::TagProperty;
use campfire_vision::VisionData;
use serde::de::{self, Deserialize, Deserializer, Visitor};

/// Each function `host`'s engine binds: its name, and the type of its first parameter.
fn functions(host: &ScriptHost) -> Vec<(String, Option<TypeId>)> {
    let mut functions: Vec<_> = host
        .bound_functions()
        .into_iter()
        .map(|function| (function.name, function.params.first().copied()))
        .collect();
    functions.dedup();
    functions
}

#[test]
fn the_registry_holds_exactly_what_the_engine_binds() {
    let mut host = ScriptHost::new(NonZeroU64::MIN);
    let before = functions(&host);
    let api = Release::bind_script_api(&mut host);
    let mut bound: Vec<String> = functions(&host)
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
    for _ in api.enums() {
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
fn each_form_that_takes_a_num_takes_an_integer_in_its_place_too() {
    // Each function the API binds, by its name and its parameters' types; Rhai's own before it
    // binds aside.
    let forms = |host: &ScriptHost| {
        let functions = host.bound_functions().into_iter();
        functions
            .map(|function| (function.name, function.params))
            .collect::<Vec<_>>()
    };
    let mut host = ScriptHost::new(NonZeroU64::MIN);
    let before = forms(&host);
    Release::bind_script_api(&mut host);
    let bound: Vec<(String, Vec<TypeId>)> = forms(&host)
        .into_iter()
        .filter(|form| !before.contains(form))
        .collect();
    let (num, int) = (TypeId::of::<Num>(), TypeId::of::<i64>());
    let numbers: Vec<_> = bound
        .iter()
        .filter(|(_, params)| params.contains(&num))
        .collect();
    // `ctx.damage`, `ctx.heal`, `pos.within`, `vector * n` and `n * vector` among them.
    assert!(numbers.len() >= 5, "{numbers:?}");
    for (name, params) in numbers {
        let as_int: Vec<TypeId> = params
            .iter()
            .map(|&param| if param == num { int } else { param })
            .collect();
        assert!(
            bound.contains(&(name.clone(), as_int)),
            "`{name}` takes an integer for each `Num`"
        );
    }
}

#[test]
fn every_hook_and_state_has_a_status_and_names_hold_their_roles() {
    let api = Release::script_api();
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
    // A name takes the capability of the module that binds it, as `damage` and `points` do; the
    // mode's are the core's but one that names another capability, as `respawn` names combat.
    let capability = |name| api.member(ApiOwner::Ctx, name).unwrap().spec.capability;
    assert_eq!(capability("timer"), None);
    assert_eq!(capability("respawn"), Some(Capability::Combat));
    // A data field is the capability's whose module records it: the mode's sections each of
    // the capability that reads them, the mode's script the core's.
    let owners = [
        (DataTable::Mode, "script", None),
        (DataTable::Mode, "combat", Some(Capability::Combat)),
        (DataTable::Mode, "stats", Some(Capability::Stats)),
        (DataTable::Mode, "tracks", Some(Capability::Progression)),
        (
            DataTable::ModeNavigation,
            "layers",
            Some(Capability::Navigation),
        ),
        (DataTable::UnitType, "shop", Some(Capability::Items)),
        (DataTable::UnitType, "use", Some(Capability::Interaction)),
        (DataTable::UnitType, "uses", Some(Capability::Interaction)),
    ];
    for (table, field, owner) in owners {
        assert_eq!(api.data_owner(table, field), owner, "{table:?}.{field}");
    }
    let method = MemberKind::Method;
    assert!(api.builtin("len", method) && api.builtin("max", method));
    assert!(!api.builtin("pos", method) && !api.builtin("pos", MemberKind::Field));
}

/// A misuse of the registry, and the panic it ends in.
type Misuse = (fn(&mut ScriptApi), &'static str);

/// A binding of `spec`, not written, of `status`, of no output.
const fn binding(spec: MemberSpec, status: Status) -> ApiMember {
    ApiMember {
        spec,
        writable: false,
        status,
        output: None,
    }
}

#[test]
fn a_binding_records_its_spec_once_and_whole() {
    let mut api = Release::script_api();
    let stacks = api.member(ApiOwner::Modifier, "stacks").unwrap().clone();
    let damage = api.member(ApiOwner::Ctx, "damage").unwrap().clone();
    // Another binding of the same spec adds only its writing; one of no output keeps the
    // output held.
    api.record_binding(ApiMember {
        writable: true,
        output: None,
        ..damage
    });
    let held = api.member(ApiOwner::Ctx, "damage").unwrap();
    assert_eq!(
        held,
        &ApiMember {
            writable: true,
            ..damage
        }
    );
    api.record_binding(ApiMember {
        writable: false,
        ..stacks
    });
    assert!(api.member(ApiOwner::Modifier, "stacks").unwrap().writable);
    let misuses: [Misuse; 5] = [
        (
            |api| {
                let damage = api.member(ApiOwner::Ctx, "damage").unwrap().spec;
                let spec = MemberSpec {
                    description: "another",
                    ..damage
                };
                api.record_binding(binding(spec, Status::Runs(ApiVersion::FIRST)));
            },
            "the bindings of Ctx.damage agree",
        ),
        (
            |api| {
                let damage = api.member(ApiOwner::Ctx, "damage").unwrap().spec;
                api.record_binding(binding(damage, Status::Planned));
            },
            "the bindings of Ctx.damage agree",
        ),
        (
            |api| {
                let damage = api.member(ApiOwner::Ctx, "damage").unwrap().spec;
                let mut names = damage.names;
                names[2] = Some(NameKind::Pool);
                let args = ArgKinds {
                    names,
                    enums: damage.enums,
                };
                let status = Status::Runs(ApiVersion::FIRST);
                api.record_binding(binding(damage.with_args(args), status));
            },
            "the forms of Ctx.damage say alike what an argument is",
        ),
        (
            |api| {
                let damage = api.member(ApiOwner::Ctx, "damage").unwrap().clone();
                api.record_binding(ApiMember {
                    output: Some(TypeId::of::<u8>()),
                    ..damage
                });
            },
            "the forms of Ctx.damage give one type",
        ),
        (
            |api| {
                let field = api.data()[0];
                api.record_data_field(field);
            },
            "is recorded once",
        ),
    ];
    for (misuse, message) in misuses {
        let mut api = Release::script_api();
        let panic = panic::catch_unwind(AssertUnwindSafe(|| misuse(&mut api))).unwrap_err();
        let said = panic.downcast_ref::<String>().unwrap();
        assert!(said.contains(message), "{said}");
    }
    // A capability's module that names its own capability on a name says what it already is.
    let mut host = ScriptHost::new(NonZeroU64::MIN);
    let redundant = panic::catch_unwind(AssertUnwindSafe(|| {
        ScriptApi::bind(&mut host, |api| {
            api.module(Some(Capability::Combat));
            let spec = MemberSpec::value("strength", "a test's").capability(Capability::Combat);
            api.bind(spec, |_: &mut Ctx| 1_i64);
        });
    }));
    let said = redundant
        .unwrap_err()
        .downcast_ref::<String>()
        .cloned()
        .unwrap();
    assert!(
        said.contains("names the capability of the module that binds it"),
        "{said}"
    );
}

/// The table of `T`, and the fields its schema reads, sorted.
fn table<T: DataSection>() -> (DataTable, Vec<&'static str>) {
    (T::TABLE, serde_fields::<T>())
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
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
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

/// The most bytes a test reads of the reference: far past the few hundred kilobytes it holds.
const REFERENCE_LEN: usize = 16 << 20;

/// The reference beside the script API's design.
const REFERENCE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/design/08-script-api-reference.md"
);

#[test]
fn the_reference_is_what_the_registry_writes() {
    let mut api = Release::script_api();
    let written = api.reference();
    PinnedFile::new(Path::new(REFERENCE))
        .bless(written.as_bytes())
        .unwrap();
    let held = InputFile::read_text(Path::new(REFERENCE), REFERENCE_LEN).unwrap();
    assert!(
        held == written,
        "the reference differs from the registry: run this test with CAMPFIRE_BLESS=1"
    );
    api.members_mut()[0].spec.description = "another description";
    assert_ne!(api.reference(), held);
    // A script's literal is checked by a method's name alone, so every handle's method of a
    // name marks the same names.
    let methods = api.members().iter().filter(|member| {
        member.spec.owner != ApiOwner::Ctx && member.spec.kind == MemberKind::Method
    });
    for member in methods {
        assert_eq!(
            api.method(member.spec.name).map(MemberSpec::args),
            Some(member.spec.args()),
            "{}",
            member.spec.name
        );
    }
}

#[test]
fn the_data_fields_are_the_schemas() {
    let api = Release::script_api();
    // The mode's file holds its script and its package's actions, modifiers and item types
    // beside `ModeData`, which the package load reads apart.
    let mut mode = serde_fields::<ModeData>();
    mode.extend(["actions", "modifiers", "items", "script"]);
    mode.sort_unstable();
    let empty = Toml::parse::<UnitTypeData>("").unwrap();
    let mut unit_type = empty.fields().map(|(name, _)| name).to_vec();
    unit_type.sort_unstable();
    // Its fields as `given` names them are those its table reads.
    assert_eq!(unit_type, serde_fields::<UnitTypeData>());
    let tables = [
        (DataTable::Mode, mode),
        (DataTable::UnitType, unit_type),
        table::<ModeCombatData>(),
        table::<ModeNavigationData>(),
        table::<SlotKindData>(),
        table::<ChoiceData>(),
        table::<OptionData>(),
        table::<LeechData>(),
        table::<RelationData>(),
        table::<TeamData>(),
        table::<PlayersData>(),
        table::<SavesData>(),
        table::<ActionData>(),
        table::<EffectData>(),
        table::<DeliveryData>(),
        table::<ProjectileData>(),
        table::<AreaData>(),
        table::<AreaInsideData>(),
        table::<TrackData>(),
        table::<ModifierData>(),
        table::<AuraData>(),
        table::<CombatData>(),
        table::<TurretData>(),
        table::<ProductionData>(),
        table::<SupplyData>(),
        table::<ModeSupplyData>(),
        table::<RequiresData>(),
        table::<NodeData>(),
        table::<DropOffData>(),
        table::<VisionData>(),
        table::<CollisionData>(),
        table::<AiData>(),
        table::<TagData>(),
        table::<ItemData>(),
        table::<InventoryData>(),
        table::<ShopData>(),
        table::<UseData>(),
        table::<UsesData>(),
    ];
    // Each table once, its type the one its section names.
    let mut held: Vec<DataTable> = tables.iter().map(|&(table, _)| table).collect();
    held.sort_unstable();
    assert_eq!(held, DataTable::ALL);
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
