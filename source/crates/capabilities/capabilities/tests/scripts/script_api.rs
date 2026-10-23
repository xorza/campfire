use std::any::TypeId;
use std::env;
use std::num::NonZeroU64;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use campfire_actions::{
    ActionData, DeliverySections, EffectData, KindSections, ListedEffects, PlannedEffect,
    SlotKindData,
};
use campfire_areas::{AreaData, AreaDeliveryData, AreaInsideData};
use campfire_capabilities::Release;
use campfire_codec::StructFields;
use campfire_combat::{
    AttackData, CombatData, DamageHandle, HealHandle, LeechData, ModeCombatData, TurretData,
};
use campfire_declarations::{ListedSet, SectionTable};
use campfire_interaction::{UseData, UsesData};
use campfire_items::{InventoryData, ItemData, ShopData};
use campfire_math::Num;
use campfire_mode::{
    BountyData, ChoiceData, ModeBountyData, ModeData, OptionData, PlayersData, RelationData,
    SavesData, TeamData, VictoryData,
};
use campfire_navigation::ModeNavigationData;
use campfire_orders::AiData;
use campfire_production::{
    BuildData, DropOffData, GatherData, ModeSupplyData, NodeData, ProductionData, RequiresData,
    SupplyData, TrainData,
};
use campfire_progression::TrackData;
use campfire_projectiles::{ProjectileData, ProjectileDeliveryData};
use campfire_script::*;
use campfire_sim::Capability;
use campfire_stats::{AuraData, ModifierData, ModifierHandle};
use campfire_store::InputFile;
use campfire_store::internals::PinnedFile;
use campfire_units::{CollisionData, PoolId, TagData};
use campfire_values::{ActionId, DamageKind, Named, PathId, TagProperty, Team, UnitType};
use campfire_vision::VisionData;
use serde::Deserialize;

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
        match member.decl.kind {
            MemberKind::Value | MemberKind::Field => {
                recorded.push(format!("get${}", member.decl.name));
                if member.writable {
                    recorded.push(format!("set${}", member.decl.name));
                }
            }
            MemberKind::Call | MemberKind::Method | MemberKind::Operator => {
                recorded.push(member.decl.name.to_owned());
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
    // `ctx.damage`, `ctx.heal`, `position.within`, `vector * n` and `n * vector` among them.
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
    // The registry records each hook a module declares, once, in their order, and the hooks of
    // every role: 5 of an action, 6 of a modifier, 15 of the mode, and the AI's `on_think`.
    let hooks: Vec<_> = api.hooks().iter().map(|status| status.hook).collect();
    assert_eq!(hooks, Release::hooks());
    let of_role = |role| hooks.iter().filter(|hook| hook.role == role).count();
    let roles = [
        ScriptRole::Action,
        ScriptRole::Modifier,
        ScriptRole::Mode,
        ScriptRole::Ai,
    ];
    assert_eq!(roles.map(of_role), [5, 6, 15, 1]);
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
    let damage = api.member(ApiHolder::CTX, "damage").unwrap();
    assert_eq!(
        (damage.decl.roles, damage.decl.capability, damage.status),
        (
            RoleSet::SIM,
            Some(Capability::Combat),
            Status::Runs(ApiVersion::FIRST)
        )
    );
    assert_eq!(
        damage.decl.forms,
        [
            &["unit", "kind", "amount"][..],
            &["unit", "kind", "amount", "source"]
        ]
    );
    // Its closures' signatures name the types: a unit, a damage kind's name, a number, and the
    // unit of its source.
    assert_eq!(damage.types, [["Unit"], ["text"], ["Num"], ["Unit"]]);
    // A call bound once for each type an argument takes lists them all at its place, in the
    // order bound: a dash goes to a point or after a unit.
    let dash = api.member(ApiHolder::CTX, "dash").unwrap();
    assert_eq!(dash.types[1], ["Vector", "Unit"]);
    // A form's value says the names it is: a team's from a call or a field, a damage kind's, an
    // action's, or a path's, which may be none; a list of teams', of paths' or of unit types';
    // a number names nothing.
    let names = |holder, name| api.member(holder, name).unwrap().output.unwrap().names;
    let one = |kind| Some(GivenNames::one(kind));
    let list = |kind| Some(GivenNames::list(kind));
    assert_eq!(
        [
            names(ApiHolder::of::<PlayerHandle>(), "team"),
            names(ApiHolder::of::<Unit>(), "team"),
            names(ApiHolder::of::<DamageHandle>(), "kind"),
            names(ApiHolder::of::<HealHandle>(), "action"),
            names(ApiHolder::of::<Unit>(), "path"),
            names(ApiHolder::CTX, "teams"),
            names(ApiHolder::CTX, "enemies"),
            api.returned("paths", MemberKind::Field),
            names(ApiHolder::of::<Unit>(), "queue"),
            names(ApiHolder::CTX, "damage"),
        ],
        [
            one(Team::NAMES),
            one(Team::NAMES),
            one(DamageKind::NAMES),
            one(ActionId::NAMES),
            one(PathId::NAMES),
            list(Team::NAMES),
            list(Team::NAMES),
            list(PathId::NAMES),
            list(UnitType::NAMES),
            None,
        ]
    );
    // A handle's member gives the same kind of name on every handle, so a field's name gives it.
    let (field, method) = (MemberKind::Field, MemberKind::Method);
    assert_eq!(
        [
            api.returned("team", field),
            api.returned("action", field),
            api.returned("amount", field),
            api.returned("team", method),
            api.returned("is_action", method),
        ],
        [one(Team::NAMES), one(ActionId::NAMES), None, None, None]
    );
    let timer = api.member(ApiHolder::CTX, "after").unwrap();
    assert_eq!(timer.decl.roles, RoleSet::MODE);
    let stacks = api
        .member(ApiHolder::of::<ModifierHandle>(), "stacks")
        .unwrap();
    assert!(stacks.writable);
    let points = api.member(ApiHolder::of::<Unit>(), "points").unwrap();
    assert_eq!(
        (points.decl.capability, points.status),
        (
            Some(Capability::Progression),
            Status::Runs(ApiVersion::FIRST)
        )
    );
    let perk = api.member(ApiHolder::of::<Unit>(), "has_perk").unwrap();
    assert_eq!(
        (perk.decl.capability, perk.status),
        (Some(Capability::Progression), Status::Planned)
    );
    // A name takes the capability of the module that binds it, as `damage` and `points` do; the
    // mode's are the core's but one that names another capability, as `respawn` names combat.
    let capability = |name| api.member(ApiHolder::CTX, name).unwrap().decl.capability;
    assert_eq!(capability("after"), None);
    assert_eq!(capability("respawn"), Some(Capability::Combat));
    // A section is the capability's whose module registers it: the mode's sections each of the
    // capability that reads them, and the mode's scripts, which no section holds, no module's.
    let sections = Release::sections();
    let capabilities = [
        (SectionTable::Mode, "scripts", None),
        (SectionTable::Mode, "combat", Some(Capability::Combat)),
        (SectionTable::Mode, "stats", Some(Capability::Stats)),
        (SectionTable::Mode, "tracks", Some(Capability::Progression)),
        (
            SectionTable::Mode,
            "navigation",
            Some(Capability::Navigation),
        ),
        (SectionTable::UnitType, "shop", Some(Capability::Items)),
        (SectionTable::UnitType, "use", Some(Capability::Interaction)),
        (
            SectionTable::UnitType,
            "uses",
            Some(Capability::Interaction),
        ),
    ];
    for (table, key, capability) in capabilities {
        assert_eq!(
            sections.capability(table, key),
            capability,
            "{table:?}.{key}"
        );
    }
    let method = MemberKind::Method;
    assert!(api.builtin("len", method) && api.builtin("max", method));
    assert!(!api.builtin("position", method) && !api.builtin("position", MemberKind::Field));
}

/// A misuse of the registry, and the panic it ends in.
type Misuse = (fn(&mut ScriptApi), &'static str);

/// A misuse of the builder as a module binds, and the panic it ends in.
type BindMisuse = (fn(&mut ApiBuilder<'_>), &'static str);

/// A binding of `spec`, not written, of `status`, of no output and no types.
#[expect(
    clippy::large_types_passed_by_value,
    reason = "the binding holds the member it takes"
)]
const fn binding(decl: MemberDecl, status: Status) -> ApiMember {
    ApiMember {
        decl,
        writable: false,
        status,
        output: None,
        types: Vec::new(),
    }
}

#[test]
fn a_binding_records_its_spec_once_and_whole() {
    let mut api = Release::script_api();
    let stacks = api
        .member(ApiHolder::of::<ModifierHandle>(), "stacks")
        .unwrap()
        .clone();
    let damage = api.member(ApiHolder::CTX, "damage").unwrap().clone();
    // Another binding of the same spec adds only its writing; one of no output keeps the
    // output held.
    api.record_binding(ApiMember {
        writable: true,
        output: None,
        ..damage.clone()
    });
    let held = api.member(ApiHolder::CTX, "damage").unwrap();
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
    assert!(
        api.member(ApiHolder::of::<ModifierHandle>(), "stacks")
            .unwrap()
            .writable
    );
    let misuses: [Misuse; 6] = [
        (
            |api| {
                let damage = api.member(ApiHolder::CTX, "damage").unwrap().decl;
                let spec = MemberDecl {
                    description: "another",
                    ..damage
                };
                api.record_binding(binding(spec, Status::Runs(ApiVersion::FIRST)));
            },
            "the bindings of Ctx.damage agree",
        ),
        (
            |api| {
                let damage = api.member(ApiHolder::CTX, "damage").unwrap().decl;
                api.record_binding(binding(damage, Status::Planned));
            },
            "the bindings of Ctx.damage agree",
        ),
        (
            |api| {
                let damage = api.member(ApiHolder::CTX, "damage").unwrap().decl;
                let mut names = damage.names;
                names[1] = Some(PoolId::NAMES);
                let args = ArgKinds {
                    names,
                    ..damage.args()
                };
                let status = Status::Runs(ApiVersion::FIRST);
                api.record_binding(binding(damage.with_args(args), status));
            },
            "the forms of Ctx.damage say alike what an argument is",
        ),
        (
            |api| {
                let damage = api.member(ApiHolder::CTX, "damage").unwrap().clone();
                api.record_binding(ApiMember {
                    output: Some(FormOutput {
                        ty: TypeId::of::<u8>(),
                        names: None,
                    }),
                    ..damage
                });
            },
            "the forms of Ctx.damage give one type",
        ),
        (
            |api| {
                let team = api.member(ApiHolder::of::<Unit>(), "team").unwrap().clone();
                let output = team.output.unwrap();
                api.record_binding(ApiMember {
                    decl: MemberDecl {
                        holder: ApiHolder::of::<DamageHandle>(),
                        ..team.decl
                    },
                    output: Some(FormOutput {
                        names: Some(GivenNames::one(PoolId::NAMES)),
                        ..output
                    }),
                    ..team
                });
            },
            "every handle's `team` gives the same kind of name",
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
    // A capability's module that names its own capability on a name says what it already is; a
    // planned field, and a field a module owns of a shared type, are fields serde reads of it.
    let binds: [BindMisuse; 3] = [
        (
            |api| {
                api.module(Some(Capability::Combat));
                let decl = MemberDecl::value("strength", "a test's").capability(Capability::Combat);
                api.bind(decl, |_: &mut Ctx| 1_i64);
            },
            "names the capability of the module that binds it",
        ),
        (
            |api| {
                api.data::<TeamData>(&[("colour", Status::Planned)]);
            },
            "the fields of A team, `[teams.<team>]` given a status are fields of its type",
        ),
        (
            |api| {
                api.own_fields::<AreaDeliveryData>(&["areas"]);
            },
            "the fields of An action's `delivery` a module owns are fields of its type",
        ),
    ];
    for (bind, message) in binds {
        let mut host = ScriptHost::new(NonZeroU64::MIN);
        let panic = panic::catch_unwind(AssertUnwindSafe(|| ScriptApi::bind(&mut host, bind)));
        let said = panic
            .unwrap_err()
            .downcast_ref::<String>()
            .cloned()
            .unwrap();
        assert!(said.contains(message), "{said}");
    }
    // The planned field is recorded so, and the rest of its type's fields as the release reads
    // them.
    let mut host = ScriptHost::new(NonZeroU64::MIN);
    let api = ScriptApi::release(&mut host, |api| {
        api.data::<OptionData>(&[("default", Status::Planned)]);
    });
    let fields: Vec<_> = api
        .data()
        .iter()
        .map(|field| (field.name, field.status))
        .collect();
    let runs = Status::Runs(ApiVersion::FIRST);
    assert_eq!(fields, [("values", runs), ("default", Status::Planned)]);
    // A member runs since the version its decl gives, the first unless said otherwise.
    let later = ApiVersion { major: 1, minor: 1 };
    let mut host = ScriptHost::new(NonZeroU64::MIN);
    let api = ScriptApi::release(&mut host, |api| {
        let decl = |name| MemberDecl::value(name, "a test's");
        api.bind(decl("strength").since(later), |_: &mut Ctx| 1_i64)
            .bind(decl("weight"), |_: &mut Ctx| 2_i64);
    });
    let status = |name| api.member(ApiHolder::CTX, name).unwrap().status;
    assert_eq!(
        [status("strength"), status("weight")],
        [Status::Runs(later), runs]
    );
}

/// The table of `T`, and the fields its schema reads, sorted.
fn table<T: DataSection>() -> (DataTable, Vec<&'static str>) {
    (T::TABLE, serde_fields::<T>())
}

/// The names serde reads for `T`'s fields, as its derive or its own impl gives them, sorted.
fn serde_fields<'de, T: Deserialize<'de>>() -> Vec<&'static str> {
    let mut fields = StructFields::of::<T>().to_vec();
    fields.sort_unstable();
    fields
}

/// The most bytes a test reads of the reference: far past the few hundred kilobytes it holds.
const REFERENCE_LEN: usize = 16 << 20;

/// The reference beside the script API's design.
const REFERENCE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../docs/design/06-script-api-reference.md"
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
    api.members_mut()[0].decl.description = "another description";
    assert_ne!(api.reference(), held);
    // A script's literal is checked by a method's name alone, so every handle's method of a
    // name marks the same names.
    let methods = api.members().iter().filter(|member| {
        member.decl.holder != ApiHolder::CTX && member.decl.kind == MemberKind::Method
    });
    for member in methods {
        assert_eq!(
            api.method(member.decl.name).map(MemberDecl::args),
            Some(member.decl.args()),
            "{}",
            member.decl.name
        );
    }
}

#[test]
fn the_data_fields_are_the_schemas() {
    let api = Release::script_api();
    // The mode's file holds its scripts, its unit types' defaults and its package's actions,
    // modifiers and item types beside `ModeData`, which the package load reads apart.
    // A table's keys are its own fields and the sections its modules register.
    let sections = Release::sections();
    let mut mode = serde_fields::<ModeData>();
    mode.extend(["actions", "modifiers", "items", "scripts", "defaults"]);
    mode.extend(sections.keys(SectionTable::Mode));
    mode.sort_unstable();
    let mut unit_type: Vec<&str> = sections.keys(SectionTable::UnitType).collect();
    unit_type.sort_unstable();
    // An action's pipeline keys and its lists are every kind's, and the rest its kind's own,
    // each field once.
    let mut action: Vec<&str> = ActionData::pipeline_keys().collect();
    let kinds = [
        StructFields::of::<AttackData>(),
        StructFields::of::<TrainData>(),
        StructFields::of::<BuildData>(),
        StructFields::of::<GatherData>(),
    ];
    action.extend(kinds.into_iter().flatten());
    action.sort_unstable();
    action.dedup();
    let mut kind: Vec<&str> = sections.listed_keys::<KindSections>().collect();
    kind.sort_unstable();
    // A delivery's keys are each form's own.
    let mut delivery = serde_fields::<ProjectileDeliveryData>();
    delivery.extend(serde_fields::<AreaDeliveryData>());
    delivery.sort_unstable();
    let mut effect: Vec<&str> = sections.listed_keys::<ListedEffects>().collect();
    effect.extend(PlannedEffect::ALL.map(PlannedEffect::name));
    effect.push(EffectData::TO);
    effect.sort_unstable();
    let tables = [
        (DataTable::MODE, mode),
        (DataTable::UNIT_TYPE, unit_type),
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
        table::<VictoryData>(),
        table::<ModeBountyData>(),
        table::<BountyData>(),
        (ActionData::TABLE, action),
        (KindSections::TABLE, kind),
        (ListedEffects::TABLE, effect),
        (DeliverySections::TABLE, delivery),
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
    let mut recorded: Vec<DataTable> = api.data().iter().map(|field| field.table).collect();
    recorded.sort_unstable();
    recorded.dedup();
    assert_eq!(held, recorded);
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
