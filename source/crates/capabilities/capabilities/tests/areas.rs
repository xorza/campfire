use std::collections::BTreeMap;

use campfire_abilities::*;
use campfire_actions::{ActionData, ActionTarget, DeliveryData, LaunchId, Targeting};
use campfire_areas::*;
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_codec::Binary;
use campfire_common::{DeclaredName, Tick, Ticks};
use campfire_declarations::Role;
use campfire_deliveries::{Deliveries, Delivering};
use campfire_math::{Num, Vec3};
use campfire_production::Production;
use campfire_projectiles::Projectiles;
use campfire_script::{Filter, ScriptLimits};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimComponent, SimWorld};
use campfire_stats::{
    Application, AuraData, CarriedMut, ModifierData, Modifiers, NewInstance, Reapply, Stats,
};
use campfire_units::internals::Spawn;
use campfire_units::{Body, Dead, Life, UnitTypeCore, UnitTypes, Units, UnitsWorld};
use campfire_values::{
    Block, EngineTag, FilterData, Number, PackageIndex, ParamRef, Rank, Relation, RelationSet,
    Scalar, TagSet, Team, TypeScope, VisionShare,
};

use crate::abilities::{abilities_match_with, changing, half_meters, strike};

#[test]
fn a_filter_selects_a_delivery_unit_only_when_it_names_its_tag() {
    let types = UnitTypes::new(Release::engine_tags());
    let [projectile, area] = [Projectiles::TAG, Areas::TAG].map(EngineTag::tag);
    let parse = |text| Filter::parse(text, types.tags()).unwrap();
    // A relation alone is the filter of its name.
    assert_eq!(
        Filter::of_release_relations(RelationSet::Enemies),
        parse("enemies")
    );
    assert_eq!(Filter::of_release_relations(RelationSet::All), parse("all"));
    let units = [
        TagSet::NONE,
        TagSet::from([projectile]),
        TagSet::from([area]),
    ];
    for (filter, selected) in [
        ("enemies", [true, false, false]),
        ("enemies:area", [false, false, true]),
        ("enemies:projectile", [false, true, false]),
        ("enemies:!area", [true, false, false]),
    ] {
        let selects = units.map(|tags| parse(filter).selects(Relation::Hostile, tags));
        assert_eq!(selects, selected, "{filter}");
    }
}

#[test]
fn an_area_is_state_and_restores() {
    // A match of combat and areas with an area type and a train for its action, the same in both,
    // and a warding area type, which holds Rally on allies, reading its action's `ward`.
    let loads = |game: &mut Match| {
        let data = UnitTypeCore::default();
        let field = Units::load_type(game, TypeScope::Mode, "field", &data);
        let area = AreaData {
            radius: Num::int(2),
            delay_ms: 0,
            duration_ms: 0,
            affects: None,
            inside: AreaInsideData::default(),
        };
        Areas::load_type(game, field, PackageIndex::MODE, &area);
        let action = Production::add_train(game, field, Ticks::ZERO, None);
        let ward = DeclaredName::new("ward").unwrap();
        let rally = ModifierData {
            shield: Some(Number::Param(ParamRef { param: ward })),
            ..ModifierData::default()
        };
        Stats::load_modifier(game, PackageIndex::MODE, "rally", &rally, None);
        let warding = Units::load_type(game, TypeScope::Mode, "warding", &data);
        let inside = AreaInsideData {
            allies: Some(DeclaredName::new("rally").unwrap()),
            ..AreaInsideData::default()
        };
        let area = AreaData { inside, ..area };
        Areas::load_type(game, warding, PackageIndex::MODE, &area);
        let source = Units::load_type(game, TypeScope::Mode, "source", &data);
        (field, action, warding, source)
    };
    let declared = [Capability::Stats, Capability::Combat, Capability::Areas];
    let mut game = Match::builder(&declared).role(Role::Rules).build();
    let (field, action, warding, source_type) = loads(&mut game);
    let source = Spawn::of(source_type, Vec3::ZERO).place(&mut game);
    let by = Delivering {
        source,
        action,
        rank: Rank::FIRST,
        start: None,
        launch: None,
    };
    let area = Area::new(by, None, Tick::new(9));
    let trigger = AreaTrigger::at(Tick::new(5));
    let id = Deliveries::spawn_of(&mut game, source, field, Vec3::ZERO, (area, trigger));
    let mut restored = Match::builder(&declared).role(Role::Rules).build();
    loads(&mut restored);
    Release::restore_into(&game, &mut restored);
    assert_eq!(restored.part::<Area>(id), game.part::<Area>(id));
    assert_eq!(restored.part::<AreaTrigger>(id), &trigger);
    // It triggers by its end, at tick 9 at the latest; a unit that is no area has no trigger.
    let entity = game.unit_entity(id);
    let triggers = |at| AreaTrigger::at(Tick::new(at)).check(&game, entity);
    assert_eq!([5, 9, 10].map(triggers), [true, true, false]);
    let id = Spawn::of(source_type, Vec3::ZERO).place(&mut game);
    assert!(!trigger.check(&game, game.unit_entity(id)));
    // Its times are at most the limit: a tick past it fails to decode.
    let past = Tick::new(Tick::LIMIT.get() + 1);
    assert!(Binary::decodes_again(&Area::new(by, None, Tick::LIMIT)));
    assert!(!Binary::decodes_again(&Area::new(by, None, past)));
    assert!(Binary::decodes_again(&AreaTrigger::at(Tick::LIMIT)));
    assert!(!Binary::decodes_again(&AreaTrigger::at(past)));
    // An area of a launch the match did not load has no lists to run.
    let launched = Delivering {
        launch: Some(LaunchId::nth(0)),
        ..by
    };
    let area = Area::new(launched, None, Tick::LIMIT);
    assert!(!area.check(&game, entity));
    // A warding area: the train, which declares no `ward`, cannot have delivered it, as Rally
    // would fail as an ally takes it.
    let warded = Deliveries::spawn_of(&mut game, source, warding, Vec3::ZERO, ());
    let area = Area::new(by, None, Tick::LIMIT);
    assert!(area.check(&game, entity));
    assert!(!area.check(&game, game.unit_entity(warded)));
}

#[test]
fn an_area_and_an_aura_hold_on_the_same_units_of_each_relation_and_life() {
    // An area that holds `guarded` on its source, `rallied` on the source's other allies and
    // `marked` on the units that may be attacked, and an aura of the enemies that holds
    // `inspired`, each of 2 m around the source at 0.
    let declared = [Capability::Stats, Capability::Combat, Capability::Areas];
    let mut game = Match::builder(&declared).role(Role::Rules).build();
    let core = UnitTypeCore::default();
    let plain = Units::load_type(&mut game, TypeScope::Mode, "plain", &core);
    Units::give_type(&mut game, plain, Life::full(Num::int(100)));
    let lifeless = Units::load_type(&mut game, TypeScope::Mode, "lifeless", &core);
    let field = Units::load_type(&mut game, TypeScope::Mode, "field", &core);
    let action = Production::add_train(&mut game, field, Ticks::ZERO, None);
    let name = |name| DeclaredName::new(name).unwrap();
    let presence = ModifierData {
        aura: Some(AuraData {
            radius: Number::Value(Scalar::Int(2)),
            affects: FilterData::parse("enemies").unwrap(),
            modifier: name("inspired"),
        }),
        ..ModifierData::default()
    };
    // A package's modifiers load by name.
    let plain_modifier = ModifierData::default();
    for (modifier, data) in [
        ("guarded", &plain_modifier),
        ("inspired", &plain_modifier),
        ("marked", &plain_modifier),
        ("presence", &presence),
        ("rallied", &plain_modifier),
    ] {
        Stats::load_modifier(&mut game, PackageIndex::MODE, modifier, data, None);
    }
    let inside = AreaInsideData {
        caster: Some(name("guarded")),
        allies: Some(name("rallied")),
        enemies: Some(name("marked")),
    };
    let area = AreaData {
        radius: Num::int(2),
        delay_ms: 0,
        duration_ms: 0,
        affects: None,
        inside,
    };
    Areas::load_type(&mut game, field, PackageIndex::MODE, &area);
    let [inspired, guarded, rallied, marked, presence] =
        ["inspired", "guarded", "rallied", "marked", "presence"]
            .map(|modifier| Stats::modifier(&game, PackageIndex::MODE, modifier).unwrap());
    // Team 0 regards team 2 as neutral, and team 1 as hostile, as every pair not set is.
    game.relations_mut().set(
        Team::new(0),
        Team::new(2),
        Relation::Neutral,
        VisionShare::Shared,
    );
    let spawn = |game: &mut Match, unit_type, (x, z), team| {
        let at = Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
        Spawn::of(unit_type, at).team(Team::new(team)).place(game)
    };
    let source = spawn(&mut game, plain, (0, 0), 0);
    let units = [
        (plain, (1, 0), 1),
        (plain, (-1, 0), 2),
        (plain, (0, 1), 0),
        (lifeless, (0, -1), 1),
        (plain, (1, 1), 1),
    ]
    .map(|(unit_type, at, team)| spawn(&mut game, unit_type, at, team));
    let [.., dead] = units;
    game.insert_parts(dead, Dead);
    let by = Delivering {
        source,
        action,
        rank: Rank::FIRST,
        start: None,
        launch: None,
    };
    let area = Area::new(by, None, Tick::LIMIT);
    Deliveries::spawn_of(&mut game, source, field, Vec3::ZERO, area);
    let holder = game.unit_entity(source);
    let aura = Application {
        instance: NewInstance {
            aura_radius: Some(Num::int(2)),
            ..NewInstance::bare(presence, Some(source))
        },
        reapply: Reapply::Refresh,
        max_stacks: None,
    };
    CarriedMut::with(&mut game, holder, |carried| carried.apply(aura)).unwrap();
    game.step();
    // Each holds on the hostile unit, the neutral one and the hostile one of no life, which lives
    // by the one living test, and on neither the source, the ally nor the dead unit; the source
    // holds `guarded` alone, and the ally `rallied`.
    let holds = |modifier| {
        [source]
            .into_iter()
            .chain(units)
            .map(|id| {
                game.part::<Modifiers>(id)
                    .get(modifier, Some(source))
                    .is_some()
            })
            .collect::<Vec<_>>()
    };
    let enemies = [false, true, true, false, true, false];
    assert_eq!(holds(inspired), enemies);
    assert_eq!(holds(marked), enemies);
    assert_eq!(holds(guarded), [true, false, false, false, false, false]);
    assert_eq!(holds(rallied), [false, false, false, true, false, false]);
}

#[test]
fn an_area_reaches_the_bodies_within_its_radius_once_at_its_delay_and_ends() {
    // A delay of 0 ms triggers in tick 1, of 100 ms, 3 ticks at 30 a second, in tick 3: an area
    // lands in the tick its cast resolves, tick 0, and triggers its delay later, one tick at the
    // least.
    for (delay_ms, trigger) in [(0, 1), (100, 3)] {
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Areas,
        ];
        let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
        let blast = Units::load_type(
            &mut game,
            TypeScope::Mode,
            "blast",
            &UnitTypeCore::default(),
        );
        let data = AreaData {
            radius: Num::int(2),
            delay_ms,
            duration_ms: 0,
            affects: None,
            inside: AreaInsideData::default(),
        };
        Areas::load_type(&mut game, blast, PackageIndex::MODE, &data);
        let shot = ActionData {
            targeting: Targeting::Point,
            delivery: Some(DeliveryData::Area {
                unit_type: DeclaredName::new("blast").unwrap(),
            }),
            ..strike()
        };
        // Each hook deals damage only when it reads the area still there, no aimed unit, and no
        // direction, as an area flies none.
        let source = r#"
            fn on_hit(ctx, caster, target, hit) {
                if hit.delivery.unit_type == "blast" && hit.aimed == () && hit.direction == () {
                    ctx.damage(target, "true", 50);
                }
            }
            fn on_end(ctx, caster, hit) {
                if hit.delivery.unit_type == "blast" {
                    ctx.damage(caster, "true", 7);
                }
            }
        "#;
        let action = Abilities::load(&mut game, "shot", &shot, source);
        let caster = Abilities::caster(&mut game, action, 1);
        // The area lands on (4, 0, 0), of radius 2: a body of no radius 2 m away is inside it and
        // one a bit farther is not; a body of radius 0.5 2.5 m away touches it; an enemy whose
        // tags block it as a target is reached all the same; an ally is not.
        let center = Num::int(4);
        let edge = Abilities::spawn_combatant(
            &mut game,
            Vec3::from((center + Num::int(2), 0, 0)),
            Team::new(1),
            (),
        );
        let beyond = Abilities::spawn_combatant(
            &mut game,
            Vec3::from((center, 0, Num::int(2) + Num::EPSILON)),
            Team::new(1),
            (),
        );
        let body = Body::new(half_meters(1)).unwrap();
        let wide = Abilities::spawn_combatant(
            &mut game,
            Vec3::from((center - half_meters(5), 0, 0)),
            Team::new(1),
            body,
        );
        let hidden =
            Abilities::spawn_combatant(&mut game, Vec3::from((center, 0, 1)), Team::new(1), ());
        Units::set_blocks(&mut game, hidden, &[Block::Target]);
        let ally =
            Abilities::spawn_combatant(&mut game, Vec3::from((center, 0, 0)), Team::new(0), ());
        let units = [edge, beyond, wide, hidden, ally, caster];
        let areas = |game: &mut Match| {
            let mut query = game.query::<&Area>();
            query.iter(game).count()
        };

        Abilities::cast(
            &mut game,
            caster,
            ActionTarget::Point(Vec3::from((center, 0, 0))),
        );
        game.run_until(trigger);
        assert_eq!(
            units.map(|id| Stats::health(&game, id)),
            [500; 6],
            "{delay_ms}"
        );
        assert_eq!(areas(&mut game), 1);
        // It triggers, deals 50 to each enemy it reaches, ends at once with no duration, and
        // its `on_end` deals 7 to its caster.
        game.run_until(trigger + 1);
        let healths = units.map(|id| Stats::health(&game, id));
        assert_eq!(healths, [450, 500, 450, 450, 500, 493], "{delay_ms}");
        assert_eq!(areas(&mut game), 0);
        assert_eq!(Abilities::failed_calls(&game), []);
    }
}

#[test]
fn an_area_holds_its_inside_modifiers_by_attitude_and_hits_what_its_filter_selects() {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Abilities,
        Capability::Areas,
    ];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    let grunt = UnitTypeCore::tagged(&["grunt"]);
    let grunt = Units::load_type(&mut game, TypeScope::Mode, "grunt", &grunt);
    Abilities::give_combatant(&mut game, grunt);
    let field = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "field",
        &UnitTypeCore::default(),
    );
    Abilities::load_stats(&mut game);
    let [cover, rally, slow] = ["cover", "rally", "slow"].map(|name| {
        Stats::load_modifier(
            &mut game,
            PackageIndex::MODE,
            name,
            &changing(&[], &[], None),
            None,
        );
        Stats::modifier(&game, PackageIndex::MODE, name).unwrap()
    });
    // Radius 2 and 100 ms, 3 ticks at 30 a second; it hits only enemy grunts, and holds a
    // modifier of its own on its caster, its caster's allies and its enemies.
    let data = AreaData {
        radius: Num::int(2),
        delay_ms: 0,
        duration_ms: 100,
        affects: Some(FilterData::parse("enemies:grunt").unwrap()),
        inside: AreaInsideData {
            caster: Some(DeclaredName::new("cover").unwrap()),
            allies: Some(DeclaredName::new("rally").unwrap()),
            enemies: Some(DeclaredName::new("slow").unwrap()),
        },
    };
    Areas::load_type(&mut game, field, PackageIndex::MODE, &data);
    // The tag book, which a match's stats derive each unit's tags from, once every type is tagged.
    Units::seal_tags(&mut game, &BTreeMap::new());
    let shot = ActionData {
        targeting: Targeting::Point,
        delivery: Some(DeliveryData::Area {
            unit_type: DeclaredName::new("field").unwrap(),
        }),
        ..strike()
    };
    let source = r#"
        fn on_hit(ctx, caster, target, hit) {
            ctx.damage(target, "true", 50);
        }
    "#;
    let action = Abilities::load(&mut game, "shot", &shot, source);
    let caster = Abilities::caster(&mut game, action, 1);
    // The area lands on (1, 0, 0), with its caster 1 m away. Inside it: an ally, an enemy grunt,
    // an enemy of no type, and one whose tags block it as a target. Outside it, 3 m away: an ally
    // and an enemy grunt.
    let point = |x: i16, z: i16| Vec3::from((x, 0, z));
    let ally = Abilities::spawn_combatant(&mut game, point(1, 1), Team::new(0), ());
    let enemy_grunt = Spawn::of(grunt, point(2, 0))
        .team(Team::new(1))
        .place(&mut game);
    let enemy = Abilities::spawn_combatant(&mut game, point(1, -1), Team::new(1), ());
    let hidden = Abilities::spawn_combatant(&mut game, point(0, 0), Team::new(1), ());
    Units::set_blocks(&mut game, hidden, &[Block::Target]);
    let far_ally = Abilities::spawn_combatant(&mut game, point(4, 0), Team::new(0), ());
    let far_grunt = Spawn::of(grunt, point(1, 3))
        .team(Team::new(1))
        .place(&mut game);
    let units = [
        caster,
        ally,
        enemy_grunt,
        enemy,
        hidden,
        far_ally,
        far_grunt,
    ];
    // The area's modifiers each unit holds, beside the block the hidden one holds.
    let held = |game: &Match| {
        units.map(|unit| {
            let mut held = Stats::carried_modifiers(game, unit);
            held.retain(|(id, _)| [cover, rally, slow].contains(id));
            held
        })
    };
    let from_caster = |id| vec![(id, Some(caster))];
    let inside = [
        from_caster(cover),
        from_caster(rally),
        from_caster(slow),
        from_caster(slow),
        from_caster(slow),
        vec![],
        vec![],
    ];

    // It lands in tick 0 and holds its modifiers from that tick, on the hidden enemy too. It
    // triggers in tick 1, where its filter decides alone: 50 to the enemy grunt inside, and to no
    // other.
    Abilities::cast(&mut game, caster, ActionTarget::Point(point(1, 0)));
    game.run_until(1);
    assert_eq!(held(&game), inside);
    game.run_until(2);
    let healths = units.map(|unit| Stats::health(&game, unit));
    assert_eq!(healths, [500, 500, 450, 500, 500, 500, 500]);
    // It ends in tick 3, and its modifiers with it.
    game.run_until(3);
    assert_eq!(held(&game), inside);
    game.run_until(4);
    assert_eq!(held(&game), [const { Vec::new() }; 7]);
    assert_eq!(Abilities::failed_calls(&game), []);
}
