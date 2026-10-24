use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::sync::Arc;

use bevy_ecs::world::Mut;
use campfire_abilities::internals::MANA;
use campfire_abilities::*;
use campfire_actions::{
    ActionData, ActionKeys, ActionRange, ActionSlots, ActionTarget, Actions, EffectTo,
    ListedEffect, RangeField, SlotKind, SlotTimes, Targeting, TimedSlots,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_combat::{
    CombatBook, CombatRefusal, Damage, DamageCause, DamageEffectData, DamagePass, PassEntry,
    RecentAttackers,
};
use campfire_common::{DeclaredName, PlayerSlot, StateHash, Tick, Ticks};
use campfire_declarations::internals::BookLoad;
use campfire_math::{Num, Vec3};
use campfire_progression::{
    Experience, Progression, ProgressionRefusal, Thresholds, TrackData, TrackSet, XpEffectData,
};
use campfire_projectiles::{ProjectileData, ProjectileDeliveryData, Projectiles};
use campfire_script::internals::FailedCall;
use campfire_script::{ApiError, CapabilityRefusal, FailureKind, ScriptLimits};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, EntityIndex, PredictionStarted, SimTick, SimWorld, Unpredicted};
use campfire_stats::{
    Applier, Hold, IntervalRevival, ModifierBook, ModifierClocks, ModifierData, Modifiers, Pools,
    PurgeEffectData, Stats,
};
use campfire_units::internals::Spawn;
use campfire_units::{Dead, Life, Owner, TagData, UnitTypeCore, Units};
use campfire_values::{
    Block, DamageKind, FilterData, PackageIndex, Rank, Ranked, Team, TrackId, TypeScope, UnitTags,
};

use crate::abilities::{
    LASH_OUT, abilities_match, abilities_match_with_modifiers, effect, half_meters, int, lash_out,
    param, scripted, stat_attack, striker,
};

#[test]
fn a_passive_is_held_while_its_ability_has_a_rank_and_is_ready() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    // Before any system builds its queries, as a predicting client registers it.
    Unpredicted::register(&mut game);
    Abilities::load_stats(&mut game);
    // A guard whose shield is Lash Out's damage at its rank: 75, then 100.
    let guard = ModifierData {
        shield: Some(param("damage")),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, PackageIndex::MODE, "guard", &guard, None);
    let mut data = lash_out();
    data.keys.passive_modifier = Some(DeclaredName::new("guard").unwrap());
    data.keys.passive_while_ready = true;
    let action = Abilities::load(&mut game, "lash_out", &data, LASH_OUT);
    let caster = Abilities::caster(&mut game, action, 0);
    let entity = game.unit_entity(caster);
    let id = Stats::modifier(&game, PackageIndex::MODE, "guard").unwrap();
    let shield = |game: &Match| {
        let modifiers = game.get::<Modifiers>(entity).unwrap();
        let held = modifiers.get(id, Some(caster))?;
        assert!(held.lifetime.held_by(Hold::Passive) && held.lifetime.until().is_none());
        let clocks = game.get::<ModifierClocks>(entity).unwrap();
        clocks.shield_of(modifiers, id, Some(caster))
    };
    let learn = |game: &mut Match| {
        let mut slots = game.get_mut::<ActionSlots>(entity).unwrap();
        slots.learn(0);
        game.step();
    };
    // Unlearned, none; at rank 1, the shield of 75; at rank 2, applied again, 100.
    game.step();
    assert_eq!(shield(&game), None);
    learn(&mut game);
    assert_eq!(shield(&game), Some(Num::int(75)));
    learn(&mut game);
    assert_eq!(shield(&game), Some(Num::int(100)));
    // A cast puts it on cooldown for 9000 ms at rank 2, 270 ticks: gone from that tick, back in
    // the 270th after it.
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(shield(&game), None);
    for _ in 0..269 {
        game.step();
    }
    assert_eq!(shield(&game), None);
    game.step();
    assert_eq!(shield(&game), Some(Num::int(100)));
    // As on a predicting client, which held the caster as the server sent it: a cooldown that
    // came while it was held takes the guard away in the first tick it is predicted again, and
    // not only as the cooldown ends, though the times kept the change tick they had then.
    game.entity_mut(entity).insert(Unpredicted);
    let now = game.now();
    game.get_mut::<SlotTimes>(entity)
        .unwrap()
        .cool_down(0, now.after(Ticks::new(100)));
    game.step();
    assert_eq!(shield(&game), Some(Num::int(100)));
    game.entity_mut(entity).remove::<Unpredicted>();
    game.trigger(PredictionStarted { entity });
    game.step();
    assert_eq!(shield(&game), None);

    // A weapon's passive holds as well, in a match with no abilities: a ward of 40 from its
    // first tick.
    let mut game = Abilities::match_of(
        ScriptLimits::ROOMY,
        &[Capability::Stats, Capability::Combat],
    );
    Abilities::load_stats(&mut game);
    let ward = ModifierData {
        shield: Some(int(40)),
        ..guard
    };
    Stats::load_modifier(&mut game, PackageIndex::MODE, "ward", &ward, None);
    let mut claws = lash_out();
    claws.keys.script = None;
    claws.keys.cooldown_ms = None;
    claws.keys.cost = BTreeMap::new();
    claws.keys.params = BTreeMap::new();
    claws.keys.targeting = Targeting::Unit(FilterData::parse("enemies").unwrap());
    claws.keys.range = Some(Ranked::One(RangeField::Range(ActionRange::meters(
        Num::int(2),
    ))));
    claws.section = Arc::new(stat_attack());
    claws.keys.passive_modifier = Some(DeclaredName::new("ward").unwrap());
    let claws = Actions::load(&mut game, PackageIndex::MODE, "claws", &claws, None, 1).unwrap();
    let slots = TimedSlots::new(ActionSlots::new([(claws, SlotKind::new(0), Rank::new(1))]));
    let beast = Abilities::spawn_combatant(&mut game, (0, 0, 0), Team::new(0), slots);
    let entity = game.unit_entity(beast);
    game.step();
    let ward = Stats::modifier(&game, PackageIndex::MODE, "ward").unwrap();
    let modifiers = game.get::<Modifiers>(entity).unwrap();
    let clocks = game.get::<ModifierClocks>(entity).unwrap();
    // The beast carries the ward once, of its claws' passive, as its own.
    let carried = modifiers.get(ward, Some(beast)).map(|carried| {
        let instance = carried.instance;
        (instance.source, instance.action, instance.stacks)
    });
    assert_eq!(carried, Some((Some(beast), Some(claws), 1)));
    assert_eq!(
        clocks.shield_of(modifiers, ward, Some(beast)),
        Some(Num::int(40))
    );

    // Two passives that wait for their slots each come back at their own slot's tick: slot 0's
    // guard 10 ticks on, slot 1's ward 50 ticks on, though slot 1 comes later in the unit's slots.
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    for (name, shield) in [("guard", 75), ("ward", 40)] {
        let shield = ModifierData {
            shield: Some(int(shield)),
            ..ModifierData::default()
        };
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, &shield, None);
    }
    let waiting = |name: &str, modifier: &str| {
        let mut data = lash_out();
        data.keys.passive_modifier = Some(DeclaredName::new(modifier).unwrap());
        data.keys.passive_while_ready = true;
        (name.to_owned(), data)
    };
    let [first, second] = [waiting("lash_out", "guard"), waiting("bulwark", "ward")]
        .map(|(name, data)| Abilities::load(&mut game, &name, &data, LASH_OUT));
    let slots = TimedSlots::new(ActionSlots::new([
        (first, SlotKind::new(0), Rank::new(1)),
        (second, SlotKind::new(0), Rank::new(1)),
    ]));
    let parts = (Owner::new(PlayerSlot::new(0)), slots);
    let caster = Spawn::named(&game, "caster", Vec3::ZERO)
        .parts(parts)
        .place(&mut game);
    let entity = game.unit_entity(caster);
    game.step();
    let [guard, ward] =
        ["guard", "ward"].map(|name| Stats::modifier(&game, PackageIndex::MODE, name).unwrap());
    let held = |game: &Match, id| {
        let modifiers = game.get::<Modifiers>(entity).unwrap();
        modifiers.get(id, Some(caster)).is_some()
    };
    assert!(held(&game, guard) && held(&game, ward));
    let now = game.now();
    {
        let mut times = game.get_mut::<SlotTimes>(entity).unwrap();
        times.cool_down(0, now.after(Ticks::new(10)));
        times.cool_down(1, now.after(Ticks::new(50)));
    }
    game.step();
    assert!(!held(&game, guard) && !held(&game, ward));
    while game.now() < now.after(Ticks::new(10)) {
        game.step();
    }
    game.step();
    assert!(held(&game, guard) && !held(&game, ward));
    while game.now() < now.after(Ticks::new(50)) {
        game.step();
    }
    game.step();
    assert!(held(&game, guard) && held(&game, ward));
}

#[test]
fn a_cast_applies_a_modifier_from_its_caster_with_its_abilitys_params() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    let mark = ModifierData {
        duration_ms: Some(int(1000)),
        shield: Some(param("damage")),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, PackageIndex::MODE, "mark", &mark, None);
    // Package 1 names its own `mark`, of no shield.
    let other = ModifierData {
        shield: None,
        ..mark
    };
    Stats::load_modifier(&mut game, PackageIndex::new(1), "mark", &other, None);
    let marker = r#"
fn on_resolve(ctx, caster, target) {
    let m = ctx.add_modifier(caster, "mark");
    if m.stacks != 1 { throw "a new modifier's handle has one stack"; }
}
"#;
    let action = Abilities::load(&mut game, "lash_out", &lash_out(), marker);
    // Package 1's own Lash Out, whose script adds package 1's `mark`, loads with the books.
    let own = r#"
fn on_resolve(ctx, caster, target) {
    if caster.has_modifier("mark") { throw "the caster carries no mark of package 1"; }
    ctx.add_modifier(caster, "mark");
}
"#;
    let script = Units::compile_hooked(&mut game, own).unwrap();
    let theirs = Actions::load(
        &mut game,
        PackageIndex::new(1),
        "lash_out",
        &lash_out(),
        Some(script),
        5,
    );
    let caster = Abilities::caster(&mut game, action, 3);
    let entity = game.unit_entity(caster);
    let t = game.resource::<SimTick>().start();
    Abilities::cast(&mut game, caster, ActionTarget::None);
    // From the caster, by Lash Out at rank 3: a shield of its damage there, 125; 1000 ms at 30
    // ticks a second, 30 ticks, so it ends as tick t + 31 starts.
    let id = Stats::modifier(&game, PackageIndex::MODE, "mark").unwrap();
    let modifiers = game.get::<Modifiers>(entity).unwrap();
    let held = modifiers.get(id, Some(caster)).unwrap();
    assert_eq!(
        (held.action, held.rank),
        (Some(action), Rank::new(3).unwrap())
    );
    let clocks = game.get::<ModifierClocks>(entity).unwrap();
    assert_eq!(
        (
            clocks.shield_of(modifiers, id, Some(caster)),
            held.lifetime.until()
        ),
        (Some(Num::int(125)), Some(Tick::new(t.get() + 31)))
    );

    // A cast of package 1's ability means package 1's names: the caster carries package 0's
    // `mark`, not its own, and the cast adds its own beside it.
    let slots = TimedSlots::new(ActionSlots::new([(
        theirs.unwrap(),
        SlotKind::new(0),
        Rank::new(1),
    )]));
    game.entity_mut(entity).insert(slots);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(Abilities::failed_calls(&mut game), []);
    let book = game.resource::<ModifierBook>();
    let ids = [0, 1].map(|package| {
        book.names()
            .named((PackageIndex::new(package), "mark"))
            .unwrap()
    });
    let modifiers = game.get::<Modifiers>(entity).unwrap();
    let clocks = game.get::<ModifierClocks>(entity).unwrap();
    let carried = ids.map(|id| modifiers.get(id, Some(caster)).is_some());
    let shields = ids.map(|id| clocks.shield_of(modifiers, id, Some(caster)));
    assert_eq!(
        (carried, shields),
        ([true, true], [Some(Num::int(125)), None])
    );
}

/// A match in which a strike stuns its target for 100 ms through the mode's `stunned` tag: each
/// tick's state hash, and whether the target's tags block its moving.
fn stun_run() -> Vec<(StateHash, bool)> {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    let stun = ModifierData {
        script: None,
        tags: vec![DeclaredName::new("stunned").unwrap()],
        ..scripted(None, &[])
    };
    Stats::load_modifier(&mut game, PackageIndex::MODE, "stun", &stun, None);
    // Every type loads before the tag book takes their tags.
    let target_type = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "target",
        &UnitTypeCore::default(),
    );
    Abilities::give_combatant(&mut game, target_type);
    let stunned = TagData {
        blocks: vec![Block::Move, Block::Attack, Block::Cast, Block::Use],
        ..TagData::default()
    };
    let effects = BTreeMap::from([(DeclaredName::new("stunned").unwrap(), stunned)]);
    Units::seal_tags(&mut game, &effects);
    let script = r#"fn on_resolve(ctx, caster, target) { ctx.add_modifier(target, "stun", 100); }"#;
    let strike = Abilities::load(&mut game, "strike", &Abilities::strike(), script);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy = Spawn::of(target_type, (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let entity = game.unit_entity(enemy);
    let mut seen = Vec::new();
    for tick in 0..8 {
        if tick == 2 {
            Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
        } else {
            game.step();
        }
        let stunned = game
            .get::<UnitTags>(entity)
            .is_some_and(|tags| tags.properties.blocks(Block::Move));
        seen.push((game.state_hash(), stunned));
    }
    seen
}

#[test]
fn a_stun_a_script_applies_blocks_its_target_alike_in_every_run() {
    // The strike resolves in tick 2's Hit and stuns for 100 ms, 3 ticks: the stun holds through
    // tick 2 + 3 = 5, and ends as tick 6 starts.
    let first = stun_run();
    let stunned: Vec<_> = first.iter().map(|&(_, stunned)| stunned).collect();
    assert_eq!(
        stunned,
        [false, false, true, true, true, true, false, false]
    );
    assert_eq!(stun_run(), first);
}

#[test]
fn a_purge_ends_the_applications_of_the_modifiers_that_grant_its_tag() {
    // Two casters' lists purge `stunned` in tick 0, each from its own target. The first carries
    // a stun from each caster and a slow from the second: both stuns end, whatever their source,
    // and the slow, which grants `slowed`, stays, so its tags no longer stop it moving. The
    // second carries a stun from the first caster that its passive holds and an application
    // renewed, and a stun from the second caster: the application ends, the hold keeps the first
    // stun, as its passive would apply it again, and the second ends.
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    for (name, tag) in [("slow", "slowed"), ("stun", "stunned")] {
        let data = ModifierData {
            tags: vec![DeclaredName::new(tag).unwrap()],
            ..ModifierData::default()
        };
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, &data, None);
    }
    // Every type loads before the tag book takes their tags.
    let target = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "target",
        &UnitTypeCore::default(),
    );
    Abilities::give_combatant(&mut game, target);
    let stunned = TagData {
        blocks: vec![Block::Move],
        ..TagData::default()
    };
    let effects = BTreeMap::from([(DeclaredName::new("stunned").unwrap(), stunned)]);
    Units::seal_tags(&mut game, &effects);
    let purge = Arc::new(PurgeEffectData {
        tag: DeclaredName::new("stunned").unwrap(),
    });
    let data = ActionData {
        keys: ActionKeys {
            script: None,
            ..Abilities::strike().keys
        },
        on_resolve: vec![effect(purge, EffectTo::Reached)],
        ..Abilities::strike()
    };
    let action = Actions::load(&mut game, PackageIndex::MODE, "cleanse", &data, None, 1).unwrap();
    Actions::load_effect_lists(&mut game, action, PackageIndex::MODE, &data);
    let [first, second] = [(); 2].map(|()| Abilities::caster(&mut game, action, 1));
    let freed = Spawn::of(target, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let held = Spawn::of(target, (-1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    for source in [first, second] {
        Abilities::give_from(&mut game, freed, source, "stun");
    }
    Abilities::give_from(&mut game, freed, second, "slow");
    let [stun, slow] =
        ["stun", "slow"].map(|name| Stats::modifier(&game, PackageIndex::MODE, name).unwrap());
    Stats::give_modifier(&mut game, held, stun, Applier::from_unit(first).passive());
    Abilities::give_from(&mut game, held, first, "stun");
    Abilities::give_from(&mut game, held, second, "stun");
    game.step();
    let stopped = |game: &Match, id| {
        let entity = game.unit_entity(id);
        game.get::<UnitTags>(entity)
            .is_some_and(|tags| tags.properties.blocks(Block::Move))
    };
    assert_eq!([stopped(&game, freed), stopped(&game, held)], [true, true]);
    Abilities::casts(
        &mut game,
        &[
            (first, ActionTarget::Unit(freed)),
            (second, ActionTarget::Unit(held)),
        ],
    );
    assert_eq!(Abilities::failed_calls(&mut game), []);
    assert_eq!(
        Stats::carried_modifiers(&game, freed),
        [(slow, Some(second))]
    );
    assert_eq!(Stats::carried_modifiers(&game, held), [(stun, Some(first))]);
    assert_eq!([stopped(&game, freed), stopped(&game, held)], [false, true]);
}

#[test]
fn a_cast_heals_and_restores_and_a_negative_amount_fails_it() {
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let mender = "
fn on_resolve(ctx, caster, target) {
    ctx.heal(caster, 30);
    ctx.restore(caster, \"mana\", num(20));
}
";
    let action = Abilities::load(&mut game, "lash_out", &lash_out(), mender);
    let caster = Abilities::caster(&mut game, action, 1);
    game.part_mut::<Life>(caster).take(Num::int(460));
    game.part_mut::<Pools>(caster).take(MANA, Num::int(50));
    // From 40 health and 50 mana: 30 healed and 20 restored as the effects apply, then the
    // cost of 35: 70 and 35.
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(
        (Stats::health(&game, caster), Abilities::pool(&game, caster)),
        (70, 35)
    );

    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let negative = "fn on_resolve(ctx, caster, target) { ctx.restore(caster, \"mana\", 5); ctx.heal(caster, -1); }";
    let action = Abilities::load(&mut game, "lash_out", &lash_out(), negative);
    let caster = Abilities::caster(&mut game, action, 1);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    let failed = FailedCall {
        id: Some(caster),
        hook: Release::hook("on_resolve"),
        kind: FailureKind::Api(CombatRefusal::NegativeHeal.refused()),
    };
    assert_eq!(Abilities::failed_calls(&mut game), [failed]);
    assert_eq!(Abilities::pool(&game, caster), 100);
}

/// A match of stats, combat, abilities, progression and `more`, with the stats the scaling
/// params name and the track `valor`, a level at 50, which is not the unit's level.
fn with_valor(more: &[Capability]) -> (Match, TrackId) {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Abilities,
        Capability::Progression,
    ];
    let declared: Vec<Capability> = declared.into_iter().chain(more.iter().copied()).collect();
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    let valor = TrackData {
        levels: Thresholds::new([Num::int(50)]).unwrap(),
        level: false,
    };
    let tracks = BTreeMap::from([(DeclaredName::new("valor").unwrap(), valor)]);
    Progression::load_tracks(&mut game, &tracks);
    (game, TrackId::new(0))
}

/// 10 experience on `valor`.
fn valor_xp() -> Arc<dyn ListedEffect> {
    Arc::new(XpEffectData {
        track: DeclaredName::new("valor").unwrap(),
        amount: int(10),
    })
}

/// The `damage` param as true damage.
fn true_damage() -> Arc<dyn ListedEffect> {
    Arc::new(DamageEffectData {
        amount: param("damage"),
        kind: DeclaredName::new("true").unwrap(),
    })
}

#[test]
fn an_xp_effect_to_a_unit_without_the_track_fails_its_cast_as_add_xp_does() {
    // Strike's list deals its 50 damage to the unit it aims at, then gives it 10 experience on
    // `valor`: a unit with the track takes both, and the cast pays its 10 mana; a unit without
    // the track fails the cast, which changes nothing, its damage, cost and cooldown included.
    let (mut game, valor) = with_valor(&[]);
    let data = ActionData {
        keys: ActionKeys {
            script: None,
            ..Abilities::strike().keys
        },
        on_resolve: vec![
            effect(true_damage(), EffectTo::Reached),
            effect(valor_xp(), EffectTo::Reached),
        ],
        ..Abilities::strike()
    };
    let action = Actions::load(&mut game, PackageIndex::MODE, "strike", &data, None, 1).unwrap();
    Actions::load_effect_lists(&mut game, action, PackageIndex::MODE, &data);
    let tracked = Abilities::spawn_combatant(
        &mut game,
        (1, 0, 0),
        Team::new(1),
        Experience::new(TrackSet::from([valor])),
    );
    let untracked = Abilities::spawn_combatant(&mut game, (-1, 0, 0), Team::new(1), ());
    let [first, second] = [(); 2].map(|()| Abilities::caster(&mut game, action, 1));
    Abilities::cast(&mut game, first, ActionTarget::Unit(tracked));
    assert_eq!(Abilities::failed_calls(&mut game), []);
    let xp = game.part::<Experience>(tracked).get(valor).unwrap().xp;
    assert_eq!(
        (
            Stats::health(&game, tracked),
            xp,
            Abilities::pool(&game, first)
        ),
        (450, Num::int(10), 90)
    );
    Abilities::cast(&mut game, second, ActionTarget::Unit(untracked));
    let failed = FailedCall {
        id: Some(second),
        hook: Release::hook("on_resolve"),
        kind: FailureKind::Api(ProgressionRefusal::NoTrack.refused()),
    };
    assert_eq!(Abilities::failed_calls(&mut game), [failed]);
    assert_eq!(
        (
            Stats::health(&game, untracked),
            Abilities::pool(&game, second)
        ),
        (500, 100)
    );
    assert_eq!(Abilities::time(&game, second).ready_at, Tick::ZERO);
}

#[test]
fn an_xp_effect_to_a_source_that_despawned_fails_its_hit() {
    // A bolt at 0.5 m a tick whose hit deals its 50 damage to the unit it reaches and gives its
    // caster 10 experience on `valor`. The caster despawns as the bolt flies: the hit fails, and
    // the enemy 4 m out takes no damage.
    let (mut game, valor) = with_valor(&[Capability::Projectiles]);
    let bolt = Units::load_type(&mut game, TypeScope::Mode, "bolt", &UnitTypeCore::default());
    let flight = ProjectileData {
        width: half_meters(1),
        range: Some(Num::int(6)),
        ..ProjectileData::flying(Num::int(15))
    };
    Projectiles::load_type(&mut game, bolt, &flight);
    let data = ActionData {
        keys: ActionKeys {
            script: None,
            targeting: Targeting::Direction,
            range: None,
            ..Abilities::strike().keys
        },
        delivery: Some(Arc::new(ProjectileDeliveryData {
            projectile: DeclaredName::new("bolt").unwrap(),
            count: NonZeroU8::new(1).unwrap(),
            spread_deg: Num::ZERO,
        })),
        on_hit: vec![
            effect(true_damage(), EffectTo::Reached),
            effect(valor_xp(), EffectTo::Source),
        ],
        ..Abilities::strike()
    };
    let action = Actions::load(&mut game, PackageIndex::MODE, "bolt", &data, None, 1).unwrap();
    Actions::load_effect_lists(&mut game, action, PackageIndex::MODE, &data);
    let caster = Abilities::caster(&mut game, action, 1);
    game.insert_parts(caster, Experience::new(TrackSet::from([valor])));
    let enemy = Abilities::spawn_combatant(&mut game, (4, 0, 0), Team::new(1), ());
    Abilities::cast(
        &mut game,
        caster,
        ActionTarget::Point(Vec3::from((10, 0, 0))),
    );
    let entity = game.unit_entity(caster);
    game.despawn(entity);
    let mut failed = Vec::new();
    while game.now() < Tick::new(16) {
        game.step();
        failed.extend(Abilities::failed_calls(&mut game));
    }
    let hit = FailedCall {
        id: Some(caster),
        hook: Release::hook("on_hit"),
        kind: FailureKind::Api(ProgressionRefusal::NoTrack.refused()),
    };
    assert_eq!(failed, [hit]);
    assert_eq!(Stats::health(&game, enemy), 500);
}

#[test]
fn combat_events_reach_each_modifier_once_in_their_order() {
    // Every hook fails, so the failures list the calls in the order they ran.
    let log = r#"
fn on_attack(ctx, m, target) { throw "attack"; }
fn on_attack_hit(ctx, m, d) { throw "hit"; }
fn on_damage_taken(ctx, m, d) { throw "taken"; }
fn on_kill(ctx, m, victim) { throw "kill"; }
fn on_takedown(ctx, m, victim) { throw "takedown"; }
fn on_interval(ctx, m) { throw "interval"; }
"#;
    let mut game = abilities_match_with_modifiers(
        log,
        &[
            ("log", scripted(None, &[])),
            ("pulse", scripted(Some(100), &[])),
        ],
    );
    CombatBook::amend(&mut game, |_, book| {
        book.set_assist_window(Some(Ticks::new(10)));
    });
    let striker = striker(500).load(&mut game);
    let victim = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
    let origin = Vec3::from((0, 0, 0));
    let attacker = Abilities::attacker(&mut game, striker, origin, Team::new(0), victim);
    let assister = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(0), ());
    let bystander = Abilities::spawn_combatant(&mut game, (9, 0, 0), Team::new(1), ());
    for unit in [attacker, assister, victim] {
        Abilities::give(&mut game, unit, "log");
    }
    Abilities::give(&mut game, bystander, "pulse");
    // The attacker strikes for 500 as its windup of no ticks ends, in tick 0; the assister
    // struck the victim in tick 0 too.
    let target = game.unit_entity(victim);
    game.resource_scope(|world, index: Mut<'_, EntityIndex>| {
        let mut attackers = world.get_mut::<RecentAttackers>(target).unwrap();
        attackers.record(assister, Tick::new(0), &index);
    });
    game.step();
    assert_eq!(Stats::health(&game, victim), 0);
    assert_eq!(
        Abilities::calls(&mut game),
        [
            (attacker, Release::hook("on_attack")),
            (attacker, Release::hook("on_attack_hit")),
            (victim, Release::hook("on_damage_taken")),
            (attacker, Release::hook("on_kill")),
            (attacker, Release::hook("on_takedown")),
            (assister, Release::hook("on_takedown")),
        ]
    );
    // The pulse of 100 ms, 3 ticks at 30 a second, applied in tick 0: in ticks 3 and 6 alone.
    for tick in 1..=6 {
        game.step();
        let expected = if tick % 3 == 0 {
            vec![(bystander, Release::hook("on_interval"))]
        } else {
            Vec::new()
        };
        assert_eq!(Abilities::calls(&mut game), expected, "tick {tick}");
    }
}

#[test]
fn an_interval_runs_nothing_while_its_carrier_is_dead_and_comes_back_by_its_policy() {
    // A pulse of 100 ms, 3 ticks at 30 a second, held as a passive from tick 0, comes in tick 3.
    // Its carrier is dead in ticks 4 to 12: no hook runs, and its 6, 9 and 12 pass. Back in tick
    // 13, no interval bursts to catch up, and the policy says when it comes: `continue` on its
    // own clock, 15, then 18; `reset` a whole interval after the return, 16, then 19;
    // `fire_and_reset` at the return, 13, then 16 and 19.
    let log = r#"fn on_interval(ctx, m) { throw "interval"; }"#;
    for (policy, expected) in [
        (IntervalRevival::Continue, &[3, 15, 18][..]),
        (IntervalRevival::Reset, &[3, 16, 19]),
        (IntervalRevival::FireAndReset, &[3, 13, 16, 19]),
    ] {
        let pulse = ModifierData {
            interval_revival: policy,
            ..scripted(Some(100), &[])
        };
        let mut game = abilities_match_with_modifiers(log, &[("pulse", pulse)]);
        let mut data = lash_out();
        data.keys.passive_modifier = Some(DeclaredName::new("pulse").unwrap());
        let action = Abilities::load(&mut game, "lash_out", &data, LASH_OUT);
        let caster = Abilities::caster(&mut game, action, 1);
        let entity = game.unit_entity(caster);
        let mut came = Vec::new();
        for tick in 0..20 {
            if tick == 4 {
                game.entity_mut(entity).insert(Dead);
            }
            if tick == 13 {
                game.entity_mut(entity).remove::<Dead>();
            }
            game.step();
            if Abilities::calls(&mut game).contains(&(caster, Release::hook("on_interval"))) {
                came.push(tick);
            }
        }
        assert_eq!(came, expected, "{policy:?}");
    }

    // A match of stats alone, with no combat, runs its modifiers' intervals too: a pulse given
    // in tick 0 comes in ticks 3 and 6.
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &[Capability::Stats]);
    Abilities::load_stats(&mut game);
    let script = Units::compile_hooked(&mut game, log).unwrap();
    let pulse = scripted(Some(100), &[]);
    Stats::load_modifier(&mut game, PackageIndex::MODE, "pulse", &pulse, Some(script));
    let unit = Spawn::named(&game, "caster", Vec3::ZERO).place(&mut game);
    Abilities::give(&mut game, unit, "pulse");
    let came: Vec<u64> = (0..7)
        .filter(|_| {
            game.step();
            Abilities::calls(&mut game).contains(&(unit, Release::hook("on_interval")))
        })
        .collect();
    assert_eq!(came, [3, 6]);
}

#[test]
fn a_hook_deals_damage_into_the_pass_and_a_chain_16_deep_fails() {
    // An extra attack on each attack that is not one, and a unit that hurts itself again, by its
    // modifier's `echo`, each time it takes damage. An attack and an extra attack name the
    // weapon, the tests' `weapon`; the damage of no action, and an echo's, name none.
    let hooks = r#"
fn on_attack_hit(ctx, m, d) {
    if !d.is_action("weapon") {
        throw "an attack names its weapon";
    }
    if !d.is_extra {
        ctx.attack_hit(d.target);
    }
}
fn on_damage_taken(ctx, m, d) {
    if d.action != () {
        throw "a damage of no action names none";
    }
    ctx.damage(m.carrier, "true", ctx.params.echo);
}
"#;
    let mut game = abilities_match_with_modifiers(
        hooks,
        &[
            ("double", scripted(None, &[])),
            ("echo", scripted(None, &[("echo", 1)])),
        ],
    );
    let striker = striker(30).load(&mut game);
    let victim = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
    let origin = Vec3::from((0, 0, 0));
    let attacker = Abilities::attacker(&mut game, striker, origin, Team::new(0), victim);
    let echoer = Abilities::spawn_combatant(&mut game, (9, 0, 0), Team::new(1), ());
    Abilities::give(&mut game, attacker, "double");
    Abilities::give(&mut game, echoer, "echo");
    DamagePass::queue(
        &mut game,
        PassEntry::Damage(Damage {
            source: None,
            target: echoer,
            amount: Num::int(10),
            kind: DamageKind::new(2),
            cause: DamageCause::Effect,
            action: None,
            depth: 0,
            hit: None,
        }),
    );
    game.step();
    // The attack's 30, then the extra attack's 30, which adds none: 500 − 60. The echoer's 10,
    // then an echo of 1 from each hook at depths 1 to 15; the one at 16 fails: 500 − 10 − 15.
    assert_eq!(Stats::health(&game, victim), 440);
    assert_eq!(Stats::health(&game, echoer), 475);
    let failed = FailedCall {
        id: Some(echoer),
        hook: Release::hook("on_damage_taken"),
        kind: FailureKind::Api(ApiError::ChainTooDeep),
    };
    assert_eq!(Abilities::failed_calls(&mut game), [failed]);
}

#[test]
fn a_hook_heal_is_dealt_before_the_rest_of_the_pass() {
    // A unit that heals itself by its modifier's `mend`, twice, each time true damage reaches it.
    let hooks = r#"
fn on_damage_taken(ctx, m, d) {
    if d.kind == "true" {
        ctx.heal(m.carrier, ctx.params.mend);
        ctx.heal(m.carrier, ctx.params.mend);
    }
}
"#;
    let mut game =
        abilities_match_with_modifiers(hooks, &[("mend", scripted(None, &[("mend", 30)]))]);
    let carrier = Abilities::spawn_combatant(&mut game, (0, 0, 0), Team::new(1), ());
    Abilities::give(&mut game, carrier, "mend");
    for (amount, kind) in [(450, 2), (100, 0)] {
        DamagePass::queue(
            &mut game,
            PassEntry::Damage(Damage {
                source: None,
                target: carrier,
                amount: Num::int(amount),
                kind: DamageKind::new(kind),
                cause: DamageCause::Effect,
                action: None,
                depth: 0,
                hit: None,
            }),
        );
    }
    game.step();
    // 500 − 450, then the hook's two heals of 30 before the 100 queued after the 450:
    // 50 + 30 + 30 − 100. Dealt at the end of the queue, they would come after the 100, which
    // kills.
    assert_eq!(Stats::health(&game, carrier), 10);
    assert_eq!(Abilities::failed_calls(&mut game), []);
}
