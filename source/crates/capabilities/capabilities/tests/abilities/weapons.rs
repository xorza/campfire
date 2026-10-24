use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_abilities::internals::scaling_stats;
use campfire_abilities::*;
use campfire_actions::{
    ActionData, ActionKeys, ActionRange, ActionSlots, ActionTarget, AttackTarget, EffectData,
    EffectTo, LaunchLists, ListedEffect, ProgressDues, RangeField, SlotKind, SpawnEffectData,
    Targeting, TimedSlots,
};
use campfire_areas::{Area, AreaData, AreaInsideData, Areas, LandEffectData};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_combat::{Damage, DamageCause, DamageEffectData, DamagePass, PassEntry};
use campfire_common::{DeclaredName, Tick};
use campfire_math::{Num, Vec3};
use campfire_projectiles::{ProjectileData, Projectiles};
use campfire_script::internals::FailedCall;
use campfire_script::{ApiError, FailureKind, ScriptLimits};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimWorld, StableId};
use campfire_stats::{ModifierData, StatBook, StatRule, StatValue, Stats, StatsData};
use campfire_units::{Life, UnitTypeCore, Units, UnitsWorld};
use campfire_values::{
    Block, DamageKind, FilterData, Number, PackageIndex, Rank, Ranked, Stat, Team, TypeScope,
    UnitType,
};

use crate::abilities::{effect, int, param, sapper, scaling};

/// A match of stats, combat, abilities, projectiles and areas whose `fighter` type has 40 attack
/// damage and 1 armor, which its weapons read as their damage and their rate, with a homing
/// `bolt` of 30 m a second; `mark`, a shield of the `sap` param of the action that applies it;
/// `ward`, a shield of 100; and `blast` and `ring`, areas of 2 m on enemies, of no delay and no
/// duration.
fn weapon_match() -> (Match, UnitType) {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Abilities,
        Capability::Projectiles,
        Capability::Areas,
    ];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    let fighter = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "fighter",
        &UnitTypeCore::default(),
    );
    Abilities::give_caster(&mut game, fighter, 100, 20);
    let flat = |value| StatValue {
        base: Num::int(value),
        per_level: Num::ZERO,
    };
    let [attack_damage, armor] = ["attack_damage", "armor"].map(|name| Stat::named(name).unwrap());
    let growth = StatsData::from([(attack_damage, flat(40)), (armor, flat(1))]);
    let rules: BTreeMap<_, _> = scaling_stats()
        .map(|stat| (stat, StatRule::default()))
        .into();
    let mut book = StatBook::new(&rules, Num::int(6));
    book.add_type(fighter, &growth).unwrap();
    Stats::load_book(&mut game, book);
    let bolt = Units::load_type(&mut game, TypeScope::Mode, "bolt", &UnitTypeCore::default());
    let flight = ProjectileData {
        homing: true,
        ..ProjectileData::flying(Num::int(30))
    };
    Projectiles::load_type(&mut game, bolt, &flight);
    for name in ["blast", "ring"] {
        let area = Units::load_type(&mut game, TypeScope::Mode, name, &UnitTypeCore::default());
        let data = AreaData {
            radius: Num::int(2),
            delay_ms: 0,
            duration_ms: 0,
            affects: None,
            inside: AreaInsideData::default(),
        };
        Areas::load_type(&mut game, area, PackageIndex::MODE, &data);
    }
    for (name, shield) in [("mark", param("sap")), ("ward", int(100))] {
        let data = ModifierData {
            shield: Some(shield),
            ..ModifierData::default()
        };
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, &data, None);
    }
    (game, fighter)
}

#[test]
fn a_weapons_on_hit_list_follows_each_attack_that_reaches_its_target() {
    // A melee attack on a target 1 m out strikes as its windup of no ticks ends, in tick 0; a
    // bolt launched then flies 1 m a tick from tick 1 and reaches a target 4 m out in tick 4; a
    // cast whose script
    // calls `ctx.attack_hit` strikes with the caster's weapon in tick 0. Each deals 40, 500 → 460;
    // then its list shields the target by `mark`, from the attacker with the weapon's `sap` of
    // 40, at once, and queues its `bite` of 10 + 0.5 × 40 = 30 at the end of the pass, which the
    // shield absorbs: 460 left, and 10 of the shield. An extra attack takes the first weapon
    // whose filter selects its target: past a first weapon for allies alone, which would mark
    // nothing, the sapper still strikes.
    let extra = "fn on_resolve(ctx, caster, target) { ctx.attack_hit(target); }";
    for (row, ranged, by_cast, hits_in) in [
        ("melee", false, false, 0),
        ("ranged", true, false, 4),
        ("extra", false, true, 0),
        ("extra past a weapon for allies", false, true, 0),
    ] {
        let (mut game, fighter) = weapon_match();
        let weapon = Abilities::load_weapon(&mut game, "sapper", &sapper(ranged));
        let for_allies = (row == "extra past a weapon for allies").then(|| {
            let data = ActionData {
                keys: ActionKeys {
                    targeting: Targeting::Unit(FilterData::parse("allies").unwrap()),
                    ..sapper(false).keys
                },
                on_hit: Vec::new(),
                ..sapper(false)
            };
            Abilities::load_weapon(&mut game, "salve", &data)
        });
        let out = if ranged { 4 } else { 1 };
        let target = Abilities::spawn_combatant(&mut game, (out, 0, 0), Team::new(1), ());
        let attacker = if by_cast {
            let data = ActionData {
                keys: ActionKeys {
                    range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                        Num::int(5),
                    )))),
                    ..Abilities::strike().keys
                },
                ..Abilities::strike()
            };
            let cast = Abilities::load(&mut game, "swing", &data, extra);
            let weapons = for_allies.into_iter().chain([weapon]);
            let slots = TimedSlots::new(ActionSlots::new(
                [(cast, SlotKind::new(0), Rank::new(1))]
                    .into_iter()
                    .chain(weapons.map(|weapon| (weapon, SlotKind::new(1), Rank::new(1)))),
            ));
            let caster = Abilities::fighter(&mut game, fighter, slots);
            Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
            caster
        } else {
            let slots =
                TimedSlots::new(ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))]));
            let attacker = Abilities::fighter(&mut game, fighter, slots);
            ProgressDues::change_unit(&mut game, attacker, |progress, dues| {
                progress.set_attack_target(Some(AttackTarget::ordered(target)), dues);
            });
            attacker
        };
        if by_cast {
            assert_eq!(Abilities::failed_calls(&mut game), [], "{row}");
        }
        while game.now() < Tick::new(hits_in) {
            assert_eq!(
                (
                    Stats::health(&game, target),
                    Abilities::mark(&game, target, attacker)
                ),
                (500, None),
                "{row} before its hit"
            );
            game.step();
            assert_eq!(Abilities::failed_calls(&mut game), [], "{row}");
        }
        if !by_cast {
            game.step();
            assert_eq!(Abilities::failed_calls(&mut game), [], "{row}");
        }
        assert_eq!(
            (
                Stats::health(&game, target),
                Abilities::mark(&game, target, attacker)
            ),
            (460, Some(Num::int(10))),
            "{row}"
        );
    }
}

/// `damage` true damage of `amount`.
fn true_of(amount: Number) -> Arc<dyn ListedEffect> {
    Arc::new(DamageEffectData {
        amount,
        kind: DeclaredName::new("true").unwrap(),
    })
}

/// A launch of `area` with its lists `on_hit` and `on_end`.
fn launch(area: &str, on_hit: Vec<EffectData>, on_end: Vec<EffectData>) -> Arc<dyn ListedEffect> {
    Arc::new(LandEffectData {
        area: DeclaredName::new(area).unwrap(),
        lists: LaunchLists { on_hit, on_end },
    })
}

#[test]
fn a_launch_lands_its_area_where_it_reaches_and_runs_the_lists_it_holds_once_each() {
    // A cannon within 2 m, of no windup, whose `on_hit` launches a blast, which deals its `splash`
    // param, half the attacker's 40 attack damage, 20, to each enemy within 2 m, and whose
    // `on_end` launches a ring where the attacker stands, which deals 5 to each enemy within
    // 2 m of it.
    let (mut game, fighter) = weapon_match();
    let ring = launch(
        "ring",
        vec![effect(true_of(int(5)), EffectTo::Reached)],
        Vec::new(),
    );
    let blast = launch(
        "blast",
        vec![effect(true_of(param("splash")), EffectTo::Reached)],
        vec![effect(ring, EffectTo::Source)],
    );
    let splash = scaling(
        Ranked::One(Num::ZERO),
        0,
        &[("attack_damage", Num::HALF)],
        &[],
    );
    let cannon = ActionData {
        keys: ActionKeys {
            params: BTreeMap::from([(DeclaredName::new("splash").unwrap(), splash)]),
            ..sapper(false).keys
        },
        on_hit: vec![effect(blast, EffectTo::Reached)],
        ..sapper(false)
    };
    let weapon = Abilities::load_weapon(&mut game, "cannon", &cannon);
    let attacker = Abilities::fighter(
        &mut game,
        fighter,
        TimedSlots::new(ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))])),
    );
    let [target, near, far] = [1, 2, 4].map(|x| {
        let position = Vec3::from((Num::int(x) + Num::HALF * i64::from(x == 2), 0, 0));
        Abilities::spawn_combatant(&mut game, position, Team::new(1), ())
    });
    ProgressDues::change_unit(&mut game, attacker, |progress, dues| {
        progress.set_attack_target(Some(AttackTarget::ordered(target)), dues);
    });
    // Tick 0: the attack deals 40 in Resolve, and its list launches the blast on the target,
    // 1 m out, which lands then. Tick 1: the blast triggers in Hit, a tick after it landed; it
    // reaches the target and the unit 1.5 m past it, not the one 3 m past, and deals 20 to each;
    // it ends at its trigger, and its `on_end` lands the ring on the attacker in that Hit. Tick
    // 2: the ring reaches the target, 1 m from the attacker, alone, and deals 5. Each list runs
    // once: the next attack is 30 ticks after the first.
    let mut lives = Vec::new();
    for _ in 0..5 {
        game.step();
        lives.push([target, near, far].map(|id| Stats::health(&game, id)));
    }
    assert_eq!(Abilities::failed_calls(&mut game), []);
    assert_eq!(
        lives,
        [
            [460, 500, 500],
            [440, 480, 500],
            [435, 480, 500],
            [435, 480, 500],
            [435, 480, 500],
        ]
    );
    let mut areas = game.query::<&Area>();
    assert_eq!(areas.iter(&game).count(), 0);

    // The attacker despawns after tick 0, as a dead creep does: the blast it launched still
    // triggers in tick 1, and its `on_end` launches the ring where the gone attacker would stand,
    // which lands nothing, as an area whose source is gone does.
    let (mut game, fighter) = weapon_match();
    let weapon = Abilities::load_weapon(&mut game, "cannon", &cannon);
    let attacker = Abilities::fighter(
        &mut game,
        fighter,
        TimedSlots::new(ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))])),
    );
    let target = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
    ProgressDues::change_unit(&mut game, attacker, |progress, dues| {
        progress.set_attack_target(Some(AttackTarget::ordered(target)), dues);
    });
    game.step();
    let entity = game.unit_entity(attacker);
    game.despawn(entity);
    let mut lives = Vec::new();
    for _ in 0..3 {
        game.step();
        lives.push(Stats::health(&game, target));
    }
    // The blast's splash of 0 + 0.5 × the attack damage of a source that is gone, which has no
    // stats, is 0: the target keeps the 460 the attack left it, in every tick, and no call fails.
    assert_eq!(Abilities::failed_calls(&mut game), []);
    assert_eq!(lives, [460, 460, 460]);
    let mut areas = game.query::<&Area>();
    assert_eq!(areas.iter(&game).count(), 0);
}

/// A blast's `on_hit` list spawns a fighter on what it reaches, of the attacker's allegiance as
/// it struck: the attacker despawned after tick 0, as a dead creep does, so its row is gone as
/// the blast triggers in tick 1, and the spawn holds its team and player still.
#[test]
fn a_list_spawns_for_its_attacker_once_the_attacker_is_gone() {
    let (mut game, fighter) = weapon_match();
    let spawn: Arc<dyn ListedEffect> = Arc::new(SpawnEffectData {
        unit_type: DeclaredName::new("fighter").unwrap(),
        duration_ms: None,
    });
    let blast = launch("blast", vec![effect(spawn, EffectTo::Reached)], Vec::new());
    let cannon = ActionData {
        on_hit: vec![effect(blast, EffectTo::Reached)],
        ..sapper(false)
    };
    let weapon = Abilities::load_weapon(&mut game, "cannon", &cannon);
    let attacker = Abilities::fighter(
        &mut game,
        fighter,
        TimedSlots::new(ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))])),
    );
    let target = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
    ProgressDues::change_unit(&mut game, attacker, |progress, dues| {
        progress.set_attack_target(Some(AttackTarget::ordered(target)), dues);
    });
    let allegiance = game.allegiance_of(attacker).unwrap();
    game.step();
    let entity = game.unit_entity(attacker);
    game.despawn(entity);
    game.step();
    assert_eq!(Abilities::failed_calls(&mut game), []);
    let mut fighters = game.query::<(&StableId, &UnitType)>();
    let spawned = fighters
        .iter(&game)
        .filter(|&(_, &unit_type)| unit_type == fighter)
        .map(|(&id, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(spawned.len(), 1);
    assert_eq!(game.allegiance_of(spawned[0]), Some(allegiance));
}

#[test]
fn a_weapons_list_runs_one_link_down_for_each_attack_that_reaches_a_living_target() {
    // Attacks of 40 by the sapper, queued straight into the pass: on a living target at depth 0,
    // and at depth 14, the list runs at depths 1 and 15, as above; on a target whose tags block
    // damage, or at zero life, the attack does nothing and no list follows; at depth 15 the list
    // would run at 16, past the chain's limit, so it fails, recorded for the attacker, and
    // applies nothing. A target warded by 100 takes nothing of the attack, all absorbed, and the
    // list still follows: its mark of 40 holds, and the bite of 30 comes off one of the shields.
    let (mut game, fighter) = weapon_match();
    let weapon = Abilities::load_weapon(&mut game, "sapper", &sapper(false));
    let attacker = Abilities::fighter(
        &mut game,
        fighter,
        TimedSlots::new(ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))])),
    );
    let mut target = |x| Abilities::spawn_combatant(&mut game, (x, 0, 0), Team::new(1), ());
    let [living, blocked, spent, deep, deepest, warded] = [1, 2, 3, 4, 5, 6].map(&mut target);
    // A tick first, whose stats derive each new unit's tags, as for every unit that spawns: the
    // blocked target's tags, set after it, stay.
    game.step();
    Units::set_blocks(&mut game, blocked, &[Block::Damage]);
    game.part_mut::<Life>(spent).take(Num::int(500));
    Abilities::give_from(&mut game, warded, warded, "ward");
    let allegiance = game.allegiance_of(attacker).unwrap();
    let attack = |target, depth| Damage {
        source: Some(attacker),
        target,
        amount: Num::int(40),
        kind: DamageKind::new(0),
        cause: DamageCause::Attack {
            roll: Num::ZERO,
            rank: Rank::FIRST,
            allegiance,
        },
        action: Some(weapon),
        depth,
        hit: None,
    };
    for (target, depth) in [
        (living, 0),
        (blocked, 0),
        (spent, 0),
        (deep, 14),
        (deepest, 15),
        (warded, 0),
    ] {
        DamagePass::queue(&mut game, PassEntry::Damage(attack(target, depth)));
    }
    game.step();
    let shields =
        [living, blocked, spent, deep, deepest].map(|id| Abilities::mark(&game, id, attacker));
    let marked = Some(Num::int(10));
    assert_eq!(shields, [marked, None, None, marked, None]);
    let lives = [living, blocked, spent, deep, deepest, warded].map(|id| Stats::health(&game, id));
    assert_eq!(lives, [460, 500, 0, 460, 460, 500]);
    assert_eq!(Abilities::mark(&game, warded, attacker), marked);
    let failed = FailedCall {
        id: Some(attacker),
        hook: Release::hook("on_hit"),
        kind: FailureKind::Api(ApiError::ChainTooDeep),
    };
    assert_eq!(Abilities::failed_calls(&mut game), [failed]);
}
