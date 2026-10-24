use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::sync::Arc;

use campfire_abilities::*;
use campfire_actions::{
    ActionData, ActionKeys, ActionTarget, Actions, DeliverySection, EffectTo, Targeting,
};
use campfire_areas::{AreaData, AreaDeliveryData, AreaInsideData, Areas};
use campfire_capabilities::internals::*;
use campfire_combat::DamageEffectData;
use campfire_common::DeclaredName;
use campfire_math::{Num, Vec3};
use campfire_projectiles::{
    CurveData, Projectile, ProjectileData, ProjectileDeliveryData, Projectiles, ProjectilesRefusal,
};
use campfire_script::FailureKind;
use campfire_script::{
    Audience, CapabilityRefusal, ScriptLimits, StateDecl, StateDefault, StateFieldDecl, StateType,
    StateValue,
};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, StableId};
use campfire_stats::{ModifierData, ModifierEffectData, Modifiers, RestoreEffectData, Stats};
use campfire_units::{AudienceNone, UnitState, UnitTypeCore, Units};
use campfire_values::{PackageIndex, Param, Ranked, Scalar, Team, TypeScope, UnitType};

use crate::abilities::{effect, half_meters, int, param, scripted};

#[test]
fn a_delivery_hook_reads_its_projectile_and_the_unit_its_cast_aimed_at() {
    let mut game = Abilities::match_of(
        ScriptLimits::ROOMY,
        &[
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Projectiles,
        ],
    );
    let bolt = Units::load_type(&mut game, TypeScope::Mode, "bolt", &UnitTypeCore::default());
    let data = ProjectileData {
        stop_on_hit: true,
        ..ProjectileData::flying(Num::int(15))
    };
    Projectiles::load_type(&mut game, bolt, &data);
    let shot = ActionData {
        delivery: Some(Arc::new(ProjectileDeliveryData {
            projectile: DeclaredName::new("bolt").unwrap(),
            count: NonZeroU8::MIN,
            spread_deg: Num::ZERO,
        })),
        ..Abilities::strike()
    };
    // Each hook deals damage only when it reads the bolt still there, and the hit's target is the
    // unit the cast aimed at.
    let source = r#"
        fn on_hit(ctx, caster, target, hit) {
            if hit.delivery.unit_type == "bolt" && hit.aimed == target {
                ctx.damage(target, "true", 50);
            }
        }
        fn on_end(ctx, caster, hit) {
            if hit.delivery.unit_type == "bolt" && hit.aimed != () {
                ctx.damage(caster, "true", 7);
            }
        }
    "#;
    let action = Abilities::load(&mut game, "shot", &shot, source);
    let caster = Abilities::caster(&mut game, action, 1);
    let target = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
    // The damage the hook deals carries the bolt's hit, which the target's `watch` reads, and
    // answers with 1 damage to its source: the hit's target, the unit the cast aimed at, after
    // 3 m flown, in the direction it flew.
    Abilities::load_stats(&mut game);
    let watch = r#"
        fn on_damage_taken(ctx, m, d) {
            if d.hit == () || d.hit.aimed != m.carrier || d.hit.distance != 3
                || d.hit.direction == () {
                throw "the damage carries the hit of its bolt";
            }
            ctx.damage(d.source, "true", 1);
        }
    "#;
    let script = Units::compile_hooked(&mut game, watch).unwrap();
    Stats::load_modifier(
        &mut game,
        PackageIndex::MODE,
        "watch",
        &scripted(None, &[]),
        Some(script),
    );
    Abilities::give(&mut game, target, "watch");

    // The bolt launches in tick 0 and flies half a meter a tick from tick 1: it reaches the
    // target 3 m out in tick 6, hits it, and ends there, as it stops on a hit.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
    game.run_until(7);
    assert_eq!(
        (Stats::health(&game, target), Stats::health(&game, caster)),
        (450, 492)
    );
    assert_eq!(Abilities::failed_calls(&mut game), []);
}

#[test]
fn a_script_launches_a_projectile_only_in_the_form_its_type_flies() {
    // Each cast launches its own projectile in tick 0, and its `on_resolve` one more: at the
    // target, or toward a vector, the direction to it or its point. A line type flies only along
    // a direction, a homing type only at a unit, and a curve at a unit or to a point; the other
    // form fails the call, and a failed cast launches nothing.
    let at_unit = "ctx.launch(caster.position, target)";
    let along = "ctx.launch(caster.position, caster.position.direction_to(target.position))";
    let to_point = "ctx.launch(caster.position, target.position)";
    let curve = CurveData {
        first: [Num::QUARTER, Num::int(4)],
        second: [Num::QUARTER * 3, Num::int(4)],
        follow: Num::ZERO,
        life_ms: None,
    };
    let line = ProjectileData {
        stop_on_hit: true,
        ..ProjectileData::flying(Num::int(15))
    };
    let homing = ProjectileData {
        homing: true,
        ..line.clone()
    };
    let curved = ProjectileData {
        curve: Some(curve),
        ..ProjectileData::flying(Num::int(15))
    };
    for (data, launches) in [
        (&line, [(at_unit, false), (along, true)]),
        (&homing, [(at_unit, true), (along, false)]),
        (&curved, [(at_unit, true), (to_point, true)]),
    ] {
        for (call, launches) in launches {
            let mut game = Abilities::match_of(
                ScriptLimits::ROOMY,
                &[
                    Capability::Stats,
                    Capability::Combat,
                    Capability::Abilities,
                    Capability::Projectiles,
                ],
            );
            let bolt =
                Units::load_type(&mut game, TypeScope::Mode, "bolt", &UnitTypeCore::default());
            Projectiles::load_type(&mut game, bolt, data);
            let mut shot = Abilities::strike();
            let bolt_name = DeclaredName::new("bolt").unwrap();
            shot.delivery = Some(Arc::new(ProjectileDeliveryData::one(bolt_name)));
            let source = format!("fn on_resolve(ctx, caster, target) {{ {call}; }}");
            let action = Abilities::load(&mut game, "shot", &shot, &source);
            let caster = Abilities::caster(&mut game, action, 1);
            let target = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
            Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
            game.run_until(1);
            let mut projectiles = game.query::<&Projectile>();
            let launched = projectiles.iter(&game).count();
            let failed: Vec<_> = Abilities::failed_calls(&mut game)
                .into_iter()
                .map(|failure| failure.kind)
                .collect();
            if launches {
                assert_eq!((launched, failed), (2, vec![]), "{call}");
            } else {
                let other = FailureKind::Api(ProjectilesRefusal::OtherFlight.refused());
                assert_eq!((launched, failed), (0, vec![other]), "{call}");
            }
        }
    }
}

#[test]
fn a_delivery_a_script_creates_spawns_with_the_id_its_call_took_and_the_state_it_wrote() {
    let enemy = Team::new(1);
    // Each cast delivers its own projectile or area, and its `on_resolve` one more, whose
    // `charge` it writes and reads back. The caster and the target take ids 0 and 1; the call
    // takes 2 for its delivery, and the cast's own takes 3 as it spawns later in the tick. A
    // cast whose call fails after its write delivers nothing and takes no id: the next unit to
    // spawn takes 2.
    let state = |charge: i64| vec![StateValue::Int(charge), StateValue::Int(7)];
    for area in [false, true] {
        for fails in [false, true] {
            let delivery = if area {
                Capability::Areas
            } else {
                Capability::Projectiles
            };
            let declared = [
                Capability::Stats,
                Capability::Combat,
                Capability::Abilities,
                delivery,
            ];
            let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
            let field = |default| StateFieldDecl {
                decl: StateDecl::new(StateType::Int, Some(StateDefault::Int(default))).unwrap(),
                audience: Audience::None,
            };
            let data = UnitTypeCore {
                state: [("charge", field(0)), ("mark", field(7))]
                    .map(|(name, field)| (DeclaredName::new(name).unwrap(), field))
                    .into(),
                ..UnitTypeCore::default()
            };
            let bolt = Units::load_type(&mut game, TypeScope::Mode, "bolt", &data);
            let (delivery, call) = if area {
                let data = AreaData {
                    radius: Num::int(2),
                    delay_ms: 100,
                    duration_ms: 0,
                    affects: None,
                    inside: AreaInsideData::default(),
                };
                Areas::load_type(&mut game, bolt, PackageIndex::MODE, &data);
                let area = DeclaredName::new("bolt").unwrap();
                let delivery: Arc<dyn DeliverySection> = Arc::new(AreaDeliveryData { area });
                (delivery, "ctx.land(target.position)")
            } else {
                let data = ProjectileData {
                    homing: true,
                    ..ProjectileData::flying(Num::int(15))
                };
                Projectiles::load_type(&mut game, bolt, &data);
                let bolt = DeclaredName::new("bolt").unwrap();
                let delivery: Arc<dyn DeliverySection> =
                    Arc::new(ProjectileDeliveryData::one(bolt));
                (delivery, "ctx.launch(caster.position, target)")
            };
            let shot = ActionData {
                delivery: Some(delivery),
                ..Abilities::strike()
            };
            let source = format!(
                r#"
                fn on_resolve(ctx, caster, target) {{
                    let next = {call};
                    if next.state.charge != 0 {{ throw "at its type's default"; }}
                    next.state.charge = 3;
                    if next.state.charge != 3 || next.state.mark != 7 {{
                        throw "reads its write back";
                    }}
                    if {fails} {{ throw "fails"; }}
                }}
                "#
            );
            let action = Abilities::load(&mut game, "shot", &shot, &source);
            let caster = Abilities::caster(&mut game, action, 1);
            let target = Abilities::spawn_combatant(&mut game, (3, 0, 0), enemy, ());
            assert_eq!([caster, target].map(StableId::get), [0, 1]);
            Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
            game.run_until(1);
            let mut query = game.query::<(&StableId, &UnitType, &UnitState<AudienceNone>)>();
            let mut spawned: Vec<_> = query
                .iter(&game)
                .filter(|&(_, &unit_type, _)| unit_type == bolt)
                .map(|(id, _, state)| (id.get(), state.values().to_vec()))
                .collect();
            spawned.sort_unstable_by_key(|&(id, _)| id);
            let failed = Abilities::failed_calls(&mut game).len();
            if fails {
                assert_eq!((spawned, failed), (vec![], 1), "{area}");
                let next = Abilities::spawn_combatant(&mut game, (5, 0, 0), enemy, ());
                assert_eq!(next.get(), 2, "{area}");
            } else {
                let both = vec![(2, state(3)), (3, state(0))];
                assert_eq!((spawned, failed), (both, 0), "{area}");
            }
        }
    }
}

/// Fan of Frost as data alone: five arrows of `frost_arrow` over 30 degrees, aimed along a
/// direction, whose hits deal the `damage` param, 30, as physical damage, apply `chilled`, and
/// restore 2 mana to the caster.
fn fan_of_frost() -> ActionData {
    ActionData {
        keys: ActionKeys {
            script: None,
            targeting: Targeting::Direction,
            range: None,
            params: BTreeMap::from([(
                DeclaredName::new("damage").unwrap(),
                Param::Ranked(Ranked::One(Scalar::Int(30))),
            )]),
            ..Abilities::strike().keys
        },
        delivery: Some(Arc::new(ProjectileDeliveryData {
            projectile: DeclaredName::new("frost_arrow").unwrap(),
            count: NonZeroU8::new(5).unwrap(),
            spread_deg: Num::int(30),
        })),
        on_hit: vec![
            effect(
                Arc::new(DamageEffectData {
                    amount: param("damage"),
                    kind: DeclaredName::new("physical").unwrap(),
                }),
                EffectTo::Reached,
            ),
            effect(
                Arc::new(ModifierEffectData {
                    id: DeclaredName::new("chilled").unwrap(),
                    duration_ms: None,
                }),
                EffectTo::Reached,
            ),
            effect(
                Arc::new(RestoreEffectData {
                    pool: DeclaredName::new("mana").unwrap(),
                    amount: int(2),
                }),
                EffectTo::Source,
            ),
        ],
        ..Abilities::strike()
    }
}

#[test]
fn fan_of_frost_from_data_alone_hits_exactly_the_units_in_reach() {
    // Rime's Fan of Frost as its data writes it, with no script: five arrows of 0.5 m wide,
    // fanned over 30 degrees around the aim along +x, each for 6 m at half a meter a tick, whose
    // hits deal the `damage` param, 30, and apply `chilled`; and each hit restores 2 mana to the
    // caster.
    let mut game = Abilities::match_of(
        ScriptLimits::ROOMY,
        &[
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Projectiles,
        ],
    );
    Abilities::load_stats(&mut game);
    let chilled = ModifierData {
        duration_ms: Some(int(2000)),
        ..scripted(None, &[])
    };
    let chilled = ModifierData {
        script: None,
        ..chilled
    };
    Stats::load_modifier(&mut game, PackageIndex::MODE, "chilled", &chilled, None);
    let arrow = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "frost_arrow",
        &UnitTypeCore::default(),
    );
    let data = ProjectileData {
        width: half_meters(1),
        range: Some(Num::int(6)),
        ..ProjectileData::flying(Num::int(15))
    };
    Projectiles::load_type(&mut game, arrow, &data);
    let fan = fan_of_frost();
    let action =
        Actions::load(&mut game, PackageIndex::MODE, "fan_of_frost", &fan, None, 1).unwrap();
    Actions::load_effect_lists(&mut game, action, PackageIndex::MODE, &fan);
    let caster = Abilities::caster(&mut game, action, 1);
    // Bodiless units: on the middle arrow 4 m out; on the outer arrow 4 m out, at 15 degrees,
    // (3.8637, 1.0353); between two arrows at 4 m, 3.75 degrees off each, 0.26 m from each line,
    // past its 0.25; at 26.6 degrees, past the fan; 8 m out on the middle line, past the range;
    // behind the caster; and an ally of the caster's on the middle line.
    let point = |x: Num, z: Num| Vec3::from((x, 0, z));
    let decimal = |text: &str| text.parse::<Num>().unwrap();
    let enemies = [
        point(Num::int(4), Num::ZERO),
        point(decimal("3.8637"), decimal("1.0353")),
        point(decimal("3.9914"), decimal("0.2617")),
        point(Num::int(4), Num::int(2)),
        point(Num::int(8), Num::ZERO),
        point(Num::int(-2), Num::ZERO),
    ]
    .map(|position| Abilities::spawn_combatant(&mut game, position, Team::new(1), ()));
    let ally =
        Abilities::spawn_combatant(&mut game, point(Num::int(2), Num::ZERO), Team::new(0), ());
    Abilities::cast(
        &mut game,
        caster,
        ActionTarget::Point(point(Num::int(10), Num::ZERO)),
    );
    game.run_until(16);
    let chill = Stats::modifier(&game, PackageIndex::MODE, "chilled").unwrap();
    let struck = |id: StableId| {
        let entity = game.unit_entity(id);
        let chilled = game
            .get::<Modifiers>(entity)
            .unwrap()
            .get(chill, Some(caster));
        (Stats::health(&game, id), chilled.is_some())
    };
    let reached = enemies.map(struck);
    assert_eq!(
        reached,
        [
            (470, true),
            (470, true),
            (500, false),
            (500, false),
            (500, false),
            (500, false),
        ]
    );
    assert_eq!(struck(ally), (500, false));
    // The caster paid its 10 mana of 100, and two hits gave back 2 each.
    assert_eq!(Abilities::pool(&game, caster), 94);
    assert_eq!(Abilities::failed_calls(&mut game), []);
}
