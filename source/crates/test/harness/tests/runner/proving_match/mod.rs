//! The proving match plays through every capability with no failed call, and proves what it is
//! there to prove: each capability it names takes part.

use std::any::TypeId;

use bevy_ecs::component::Component;
use bevy_ecs::world::{EntityRef, World};
use campfire_actions::ActionProgress;
use campfire_areas::Area;
use campfire_capabilities::{MatchKind, MatchSetup, Release};
use campfire_common::SegmentSeed;
use campfire_declarations::{CapabilitySet, MatchFacts, Role};
use campfire_harness::{ArchetypeWatch, FixedMatch, MatchRecord, MatchUnits, ProvingMatch};
use campfire_math::{Num, Vec3};
use campfire_mode::internals::ModeWorldInternals;
use campfire_navigation::UnpartedPairs;
use campfire_production::TrainQueue;
use campfire_progression::Experience;
use campfire_projectiles::Projectile;
use campfire_runner::{MatchBuild, Session, SessionWorld};
use campfire_script::StateValue;
use campfire_sim::internals::{SimWorldInternals, StageClock};
use campfire_sim::{EntityIndex, MatchRate, Position, SimWorld, StableId};
use campfire_stats::{Level, Modifiers};
use campfire_test_packages::TestMode;
use campfire_units::{Body, Dead, Lifespan, MoveStep, Owner, SeenBy};
use campfire_values::{Team, TeamSet};

mod sweep;

/// What the match showed over its ticks.
#[derive(Debug, Default)]
struct Seen {
    projectiles: usize,
    areas: usize,
    /// The ticks in which a hero carried a modifier.
    modified_heroes: usize,
    /// The units of each player that are neither hero, barracks, projectile nor area, after the
    /// first trains end.
    trained: [usize; 2],
    /// The ticks the ward stood after, with its place and the teams that saw it; and the ticks
    /// after which north saw south's barracks.
    ward: Vec<(u64, Vec3, TeamSet)>,
    /// The ticks the sage's wisp stood after, with its place.
    wisp: Vec<(u64, Vec3)>,
    barracks_watched: Vec<u64>,
    /// The mode's state after each tick that changed it.
    states: Vec<(u64, Vec<StateValue>)>,
    /// The boundaries a save was due at, each by the tick it comes before.
    saves: Vec<u64>,
    /// Whether the lancer, after each tick, reached the boulder's box with its weapon, and the
    /// least circle that holds it, while the boulder stood.
    lancer_reach: Vec<(u64, bool, bool)>,
    /// The tick the lancer first wound up an attack on the boulder.
    first_strike: Option<u64>,
    /// The walkers left inside the boulder's box after a tick.
    sunk: Vec<(u64, StableId)>,
    /// After each tick around the lancer's two attacks on the sage, whether the lancer's attack
    /// target was the sage, and whether north saw her.
    sage_attacked: Vec<(u64, bool, bool)>,
}

/// The removed-component messages of a world after a tick: those it holds, and those it was
/// ever given, as their counts say.
#[derive(Debug)]
struct Removals {
    held: usize,
    written: usize,
}

impl Removals {
    fn of(world: &World) -> Self {
        let (mut held, mut written) = (0, 0);
        for (_, messages) in world.removed_components().iter() {
            held += messages.len();
            written += messages.oldest_message_count() + messages.len();
        }
        Self { held, written }
    }
}

/// The lancer's weapon's range, its `attack` action's in its package.
const LANCER_RANGE: Num = Num::int(4);

/// The hero of player `player_slot` in `world`, by its stable id.
fn hero(world: &World, player_slot: u32) -> (StableId, EntityRef<'_>) {
    let id = MatchUnits::of(world).hero(player_slot);
    (id, world.entity_ref_of(id).unwrap())
}

/// Asserts of tick `tick` of `fixed`, a match of `set`, whose failed script calls its log check
/// fails on, that each part a unit holds is state or derived, as a list declares it: a part none declares would be held by
/// no hash, snapshot or client; that each resource is declared, its scratch clear; and that each
/// entity is of a kind.
fn assert_sound(fixed: &FixedMatch, tick: u64, set: CapabilitySet) {
    let world = fixed.game();
    let unlisted = Release::unlisted_components(set, world);
    assert!(unlisted.is_empty(), "tick {tick}: {unlisted:?}");
    let unlisted = Release::unlisted_resources(set, world, &[TypeId::of::<Session>()]);
    assert!(unlisted.is_empty(), "tick {tick}: {unlisted:?}");
    let kindless = Release::entities_of_no_kind(world);
    assert!(kindless.is_empty(), "tick {tick}: {kindless:?}");
    // Collision parted every pair it met (04-capabilities/navigation.md, Collision leaves no
    // overlap).
    let unparted = world
        .get_resource::<UnpartedPairs>()
        .map(|pairs| pairs.get());
    assert_eq!(unparted, Some(0), "tick {tick}");
}

fn look(fixed: &FixedMatch, tick: u64, seen: &mut Seen) {
    let world = fixed.game();
    look_at_boulder(fixed, tick, seen);
    if matches!(tick, 4 | 5 | 28 | 29) {
        let ((_, lancer), (sage, sage_unit)) = (hero(world, 0), hero(world, 1));
        let target = lancer.get::<ActionProgress>().unwrap().attack_target();
        let north_sees = sage_unit.get::<SeenBy>().unwrap().get().unwrap();
        seen.sage_attacked.push((
            tick,
            target == Some(sage),
            north_sees.contains(Team::new(0)),
        ));
    }
    let state = world.mode_state();
    let state = state.as_slice();
    if seen.states.last().is_none_or(|(_, last)| last != state) {
        seen.states.push((tick, state.to_vec()));
    }
    for (_, entity) in world.resource::<EntityIndex>().iter() {
        let unit = world.entity(entity);
        seen.projectiles += usize::from(unit.contains::<Projectile>());
        seen.areas += usize::from(unit.contains::<Area>());
        let hero = unit.contains::<Experience>();
        if hero
            && unit
                .get::<Modifiers>()
                .is_some_and(|m| *m != Modifiers::default())
        {
            seen.modified_heroes += 1;
        }
        if unit.contains::<Lifespan>() {
            let position = unit.get::<Position>().unwrap().get();
            if unit
                .get::<Owner>()
                .is_some_and(|owner| owner.player_slot().get() == 1)
            {
                seen.wisp.push((tick, position));
            } else {
                seen.ward
                    .push((tick, position, unit.get::<SeenBy>().unwrap().get().unwrap()));
            }
        }
        let north = Team::new(0);
        if unit.contains::<TrainQueue>()
            && unit.get::<Team>() == Some(&Team::new(1))
            && unit.get::<SeenBy>().unwrap().get().unwrap().contains(north)
        {
            seen.barracks_watched.push(tick);
        }
        if tick == 40
            && let Some(owner) = unit.get::<Owner>()
            && !hero
            && !unit.contains::<TrainQueue>()
            && !unit.contains::<Projectile>()
            && !unit.contains::<Area>()
        {
            seen.trained[usize::try_from(owner.player_slot().get()).unwrap()] += 1;
        }
    }
}

/// What the boulder, a box of 2.4 by 1.2 m turned 30°, saw of the units round it after `tick`.
fn look_at_boulder(fixed: &FixedMatch, tick: u64, seen: &mut Seen) {
    let world = fixed.game();
    let units = || {
        world
            .resource::<EntityIndex>()
            .iter()
            .map(|(id, entity)| (id, world.entity(entity)))
    };
    let Some((boulder, stone)) = units()
        .find(|(_, unit)| unit.get::<Team>() == Some(&Team::new(2)) && !unit.contains::<Dead>())
    else {
        return;
    };
    let (at, body) = (
        stone.get::<Position>().unwrap().get(),
        stone.get::<Body>().unwrap(),
    );
    for (id, unit) in units() {
        let walks = unit.contains::<MoveStep>() && !unit.contains::<Dead>();
        if let (true, Some(walker)) = (walks, unit.get::<Body>())
            && walker.sinks_into(unit.get::<Position>().unwrap().get(), body, at)
        {
            seen.sunk.push((tick, id));
        }
    }
    let (_, lancer) = hero(world, 0);
    if lancer.contains::<Dead>() {
        return;
    }
    let (from, own) = (
        lancer.get::<Position>().unwrap().get(),
        lancer.get::<Body>(),
    );
    let box_reach = Body::reaches(from, own, LANCER_RANGE, at, Some(body));
    let bound_reach = Body::reaches_bound(from, own, LANCER_RANGE, at, Some(body));
    seen.lancer_reach.push((tick, box_reach, bound_reach));
    let strikes = lancer
        .get::<ActionProgress>()
        .and_then(ActionProgress::attacking)
        == Some(boulder);
    if strikes && seen.first_strike.is_none() {
        seen.first_strike = Some(tick);
    }
}

#[test]
fn the_proving_match_plays_every_capability_with_no_failed_call() {
    let proving = ProvingMatch::load();
    let mut fixed = proving.start();
    let mut seen = Seen::default();
    let mut record = MatchRecord::of(fixed.game_mut(), proving.packages(), ProvingMatch::PLAYERS);
    let capabilities = proving.packages().manifest().capabilities;
    // The match's setup clears none, so the first tick's messages hold its removals too.
    let start = Removals::of(fixed.game());
    let (mut written, mut removed) = (start.written - start.held, false);
    let mut watch = ArchetypeWatch::of(fixed.game_mut());
    for tick in 0..ProvingMatch::TICKS {
        proving.play_tick(&mut fixed);
        // The removed-component messages hold this tick's removals alone, as an `App`'s update
        // keeps them: every one written before the tick is gone.
        let removals = Removals::of(fixed.game());
        assert_eq!(removals.written - removals.held, written, "tick {tick}");
        removed |= removals.held > 0;
        written = removals.written;
        record.tick(fixed.game_mut());
        watch.tick(fixed.game(), tick);
        assert_sound(&fixed, tick, capabilities);
        look(&fixed, tick, &mut seen);
        if fixed.game().is_save_due() {
            seen.saves.push(tick + 1);
        }
    }
    record.check("proving");
    watch.assert_kept();
    assert!(removed, "no tick removed a component");
    let world = fixed.game();
    // The mode's hooks saw player 1 join in tick 1 and leave in tick 400; its state fields, by
    // name, are `joined` and `left`.
    let state = |joined, left| vec![StateValue::Int(joined), StateValue::Int(left)];
    assert_eq!(
        seen.states,
        [
            (0, state(-1, -1)),
            (ProvingMatch::JOIN, state(1, -1)),
            (ProvingMatch::LEAVE, state(1, 1)),
        ]
    );
    // Production: the orders of tick 2 train in 1.5 s, 30 ticks. North's two barracks competed
    // for gold for one guard, so north has one guard, and south one.
    assert_eq!(seen.trained, [1, 1]);
    // Orders at hidden units: an order applies in the Inputs stage, as the last tick's Vision
    // stage left the units seen. North did not see the sage after tick 4, so the lancer's attack
    // on her in tick 5 did nothing; it saw her after tick 28, so the same attack in tick 29 took.
    assert_eq!(
        seen.sage_attacked,
        [
            (4, false, false),
            (5, false, false),
            (28, false, true),
            (29, true, true)
        ]
    );
    // Projectiles and areas flew and lay; a hero carried a modifier, the snare's or the aura's.
    // As tick sums: of the projectiles in flight, the areas laid and the heroes modified.
    assert_eq!(
        [seen.projectiles, seen.areas, seen.modified_heroes],
        [189, 76, 654]
    );
    // Progression: each hero gained a level from the kills near it, to 2.
    let levels: Vec<u32> = world
        .resource::<EntityIndex>()
        .iter()
        .filter_map(|(_, entity)| {
            let unit = world.entity(entity);
            unit.contains::<Experience>()
                .then(|| unit.get::<Level>().unwrap().get())
        })
        .collect();
    assert_eq!(levels, [2, 2]);
    // Navigation: the boulder, a static body, fell.
    let boulder = world
        .resource::<EntityIndex>()
        .iter()
        .find(|(_, entity)| world.entity(*entity).get::<Team>() == Some(&Team::new(2)));
    assert!(
        boulder.is_none_or(|(_, entity)| world.entity(entity).contains::<Dead>()),
        "the boulder stands"
    );
    // A ward: north's barracks posts it at (12, 9) in tick 300, its order's, and its 2000 ms are
    // 40 ticks at 20 a second, 300 to 339: it despawns as tick 339 ends, so the state after each
    // of ticks 300 to 338 holds it. It is stealthed, so only north sees it, though south's
    // barracks sees its cell; and it sees 4 m: the barracks' cell center, (14.5, 7.5), is √8.5 ≈
    // 2.92 m off, so north sees the barracks in the Vision stages of ticks 300 to 339, and no unit
    // of north sees it otherwise.
    let ticks: Vec<u64> = seen.ward.iter().map(|&(tick, ..)| tick).collect();
    assert_eq!(ticks, (300..=338).collect::<Vec<_>>());
    let position = Vec3::from((12, 0, 9));
    let north_alone =
        |teams: TeamSet| (0..3).all(|team| teams.contains(Team::new(team)) == (team == 0));
    assert!(
        seen.ward
            .iter()
            .all(|&(_, place, teams)| place == position && north_alone(teams))
    );
    assert_eq!(seen.barracks_watched, (300..=339).collect::<Vec<_>>());
    // A summon: the sage's wisp, a unit type its own package declares, posted at (2, 6) in tick
    // 100, its order's, stands its 3000 ms, 60 ticks, from 100 to 159, so the state after each
    // of ticks 100 to 158 holds it.
    let position = Vec3::from((2, 0, 6));
    assert_eq!(
        seen.wisp,
        (100..=158).map(|tick| (tick, position)).collect::<Vec<_>>()
    );
    // A save is due after tick 0, whose first wave asked for one, and at each 12 s of 20 Hz,
    // every 240 ticks, from the start.
    assert_eq!(seen.saves, [1, 240, 480]);
    // The boulder is a box: the lancer, sent off its long side to (-3, 4) and back at it in tick
    // 150, walked 0.25 m a tick into reach. The least circle that holds the box, 1.34 m round its
    // center, came into its weapon's 4 m after tick 156; the long side, 0.6 m from the center,
    // 0.74 m nearer, three steps later, after tick 159; and the lancer stopped there and wound up
    // in tick 160. So the reach measured the box, not its bound. No walker stood inside the box
    // after any tick.
    let reach_from = |pick: fn(&(u64, bool, bool)) -> bool| {
        let after_order = seen.lancer_reach.iter().filter(|&&(tick, ..)| tick >= 150);
        after_order
            .filter(|entry| pick(entry))
            .map(|&(tick, ..)| tick)
            .next()
    };
    assert_eq!(reach_from(|&(_, _, bound)| bound), Some(156));
    assert_eq!(reach_from(|&(_, boxed, _)| boxed), Some(159));
    assert_eq!(seen.first_strike, Some(160));
    assert_eq!(seen.sunk, []);
}

/// A component no system reads, which moves the unit that carries it to another archetype.
#[derive(Component, Debug)]
struct Inert;

/// The match plays alike whatever order Bevy's queries give units in, and with a bench's stage
/// probes in its schedule. A query walks each archetype's units in the order they joined it; so
/// each tick, before it runs, every unit joins the archetype with `Inert` anew, from the highest
/// stable id down, and queries meet the units of one archetype in reverse id order. Both goldens
/// must still hold.
#[test]
fn the_proving_match_plays_alike_in_reverse_query_order_and_with_stage_probes() {
    let proving = ProvingMatch::load();
    let mut fixed = proving.start();
    StageClock::install(fixed.game_mut());
    let mut record = MatchRecord::of(fixed.game_mut(), proving.packages(), ProvingMatch::PLAYERS);
    let mut units = Vec::new();
    for _ in 0..ProvingMatch::TICKS {
        let world = fixed.game_mut();
        units.clear();
        units.extend(
            world
                .resource::<EntityIndex>()
                .iter()
                .map(|(_, entity)| entity),
        );
        for &entity in units.iter().rev() {
            world.entity_mut(entity).remove::<Inert>().insert(Inert);
        }
        proving.play_tick(&mut fixed);
        record.tick(fixed.game_mut());
    }
    record.compare("proving");
}

#[test]
fn every_order_each_mode_relies_on_in_each_role_is_stated() {
    // Bevy's sync points order the systems they lie between, so a pair that only one of them
    // keeps apart passes the ambiguity check until the systems around it change: each mode of
    // the workspace, in each role, builds its tick with none, which panics on such a pair: a
    // server's with its mode, the rules alone, and a predicting client's, each of its own books.
    for mode in TestMode::ALL {
        let packages = mode.packages();
        let player_slots = u32::try_from(packages.manifest().player_slots()).unwrap();
        drop(MatchBuild::test_target_without_sync_points(
            &packages,
            player_slots,
        ));
        let manifest = packages.manifest();
        let rate = MatchRate::new(manifest.tick_hz.default());
        for (role, kind) in [
            (Role::Rules, MatchKind::Rules),
            (Role::Client, MatchKind::Predicting),
        ] {
            let setup = MatchSetup::new(SegmentSeed::ZERO, rate, role);
            let books = packages.books(rate);
            Release::build(&mut World::new(), setup.without_sync_points(), books, kind);
        }
    }
    let proving = ProvingMatch::load();
    let mut target =
        MatchBuild::test_target_without_sync_points(proving.packages(), ProvingMatch::PLAYERS);
    // Each game system sits in a set its module declares: one ordered by hand runs where Bevy
    // picks, and one between two stages before or after the probe there.
    let sets = Release::module_sets(target.resource::<MatchFacts>().set);
    assert_eq!(target.systems_outside(&sets), Vec::<String>::new());
}
