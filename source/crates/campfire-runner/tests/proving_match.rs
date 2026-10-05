//! The proving match plays through every capability with no failed call, and proves what it is
//! there to prove: each capability it names takes part.

use bevy_ecs::component::Component;
use campfire_capabilities::{
    Area, Dead, Experience, Level, Lifespan, Modifiers, Owner, Projectile, ScriptFailures, SeenBy,
    Team, TeamSet, TrainQueue,
};
use campfire_math::{Num, Vec3};
use campfire_runner::internals::{FixedMatch, Golden, ProvingMatch, RestoreTarget};
use campfire_sim::internals::Draws;
use campfire_sim::{EntityIndex, Position};

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
    ward: Vec<(u64, Position, TeamSet)>,
    barracks_watched: Vec<u64>,
}

fn look(fixed: &FixedMatch, tick: u64, seen: &mut Seen) {
    let world = fixed.runner().world();
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
            let place = *unit.get::<Position>().unwrap();
            seen.ward
                .push((tick, place, unit.get::<SeenBy>().unwrap().get()));
        }
        let north = Team::new(0);
        if unit.contains::<TrainQueue>()
            && unit.get::<Team>() == Some(&Team::new(1))
            && unit.get::<SeenBy>().unwrap().get().contains(north)
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
            seen.trained[usize::try_from(owner.slot().get()).unwrap()] += 1;
        }
    }
}

#[test]
fn the_proving_match_plays_every_capability_with_no_failed_call() {
    let proving = ProvingMatch::load();
    let mut fixed = proving.start();
    let mut seen = Seen::default();
    let mut golden = Golden::new(proving.packages(), ProvingMatch::PLAYERS);
    for tick in 0..ProvingMatch::TICKS {
        ProvingMatch::play_tick(&mut fixed, tick);
        golden.record(fixed.runner());
        let failures = fixed.runner().world().non_send::<ScriptFailures>();
        assert!(
            failures.get().is_empty(),
            "tick {tick}: {:?}",
            failures.get()
        );
        look(&fixed, tick, &mut seen);
    }
    golden.check("proving");
    let world = fixed.runner().world();
    // Production: the orders of tick 2 train in 1.5 s, 30 ticks. North's two barracks competed
    // for gold for one guard, so north has one guard, and south one.
    assert_eq!(seen.trained, [1, 1]);
    // Projectiles and areas flew and lay; a hero carried a modifier, the snare's or the aura's.
    assert!(
        seen.projectiles > 0 && seen.areas > 0 && seen.modified_heroes > 0,
        "{seen:?}"
    );
    // Progression: a hero gained a level from the kills near it.
    let levels: Vec<u32> = world
        .resource::<EntityIndex>()
        .iter()
        .filter_map(|(_, entity)| {
            let unit = world.entity(entity);
            unit.contains::<Experience>()
                .then(|| unit.get::<Level>().unwrap().get())
        })
        .collect();
    assert!(levels.iter().any(|&level| level > 1), "{levels:?}");
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
    let at = Position::new(Vec3::new(Num::int(12), Num::ZERO, Num::int(9))).unwrap();
    let north_alone =
        |teams: TeamSet| (0..3).all(|team| teams.contains(Team::new(team)) == (team == 0));
    assert!(
        seen.ward
            .iter()
            .all(|&(_, place, teams)| place == at && north_alone(teams))
    );
    assert_eq!(seen.barracks_watched, (300..=339).collect::<Vec<_>>());
}

/// A component no system reads, which moves the unit that carries it to another archetype.
#[derive(Component, Debug)]
struct Inert;

/// The match plays alike whatever order Bevy's queries give units in. A query walks each
/// archetype's units in the order they joined it; so each tick, before it runs, every unit joins
/// the archetype with `Inert` anew, from the highest stable id down, and queries meet the units of
/// one archetype in reverse id order. Both goldens must still hold.
#[test]
fn the_proving_match_plays_alike_in_reverse_query_order() {
    let proving = ProvingMatch::load();
    let mut fixed = proving.start();
    let mut golden = Golden::new(proving.packages(), ProvingMatch::PLAYERS);
    let mut units = Vec::new();
    for tick in 0..ProvingMatch::TICKS {
        let world = fixed.runner_mut().world_mut();
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
        ProvingMatch::play_tick(&mut fixed, tick);
        golden.record(fixed.runner());
    }
    golden.check("proving");
}

/// A snapshot of the proving match restores into a match built from its packages, to the same
/// hash; and a snapshot with any byte flipped never panics as it restores or as it plays on, and
/// what restores snapshots again to the same bytes: no two byte strings restore to one state.
#[test]
fn a_snapshot_restores_and_a_flawed_one_never_panics() {
    let proving = ProvingMatch::load();
    let mut fixed = proving.start();
    // Past the first trains, with heroes, guards, creeps, projectiles and modifiers about.
    for tick in 0..60 {
        ProvingMatch::play_tick(&mut fixed, tick);
    }
    let mut target = RestoreTarget::new(proving.packages(), ProvingMatch::PLAYERS);
    let mut bytes = Vec::new();
    target.snapshot(fixed.runner().world(), &mut bytes);
    target.restore(&bytes).unwrap();
    assert_eq!(target.hash(), fixed.runner().state_hash());

    // Every byte, each flipped three ways.
    let mut again = Vec::new();
    for at in 0..bytes.len() {
        for flip in [0x01, 0x80, 0xFF] {
            let mut flawed = bytes.clone();
            flawed[at] ^= flip;
            if target.restore(&flawed).is_ok() {
                again.clear();
                target.snapshot_own(&mut again);
                assert_eq!(again, flawed, "byte {at} flipped by {flip:#04x}");
                // A value its checks let through still plays: no system panics on it.
                for _ in 0..5 {
                    target.run_tick();
                }
            }
        }
    }
}

/// The draws of each state type: enough to reach each edge of its numbers, few enough to keep
/// the test fast.
const DRAWS: usize = 16;

/// Drawn values of each state type in turn, each a value its decode accepts with every number at
/// an edge such as the limit, put in place of one holder's in a snapshot of the proving match:
/// each restores or is refused, and what restores plays on with no panic. A byte flip seldom
/// makes a value that decodes, so this reaches the checks the flips do not.
#[test]
fn a_snapshot_of_drawn_values_restores_or_is_refused_and_never_panics() {
    let proving = ProvingMatch::load();
    let mut fixed = proving.start();
    for tick in 0..60 {
        ProvingMatch::play_tick(&mut fixed, tick);
    }
    let mut target = RestoreTarget::new(proving.packages(), ProvingMatch::PLAYERS);
    let mut base = Vec::new();
    target.snapshot(fixed.runner().world(), &mut base);
    let mut draws = Draws::new(7);
    let (mut restored, mut refused) = (0, 0);
    let mut flawed = Vec::new();
    for name in target.state_names() {
        for _ in 0..DRAWS {
            target.restore(&base).unwrap();
            if !target.scramble(name, &mut draws) {
                continue;
            }
            flawed.clear();
            target.snapshot_own(&mut flawed);
            if target.restore(&flawed).is_ok() {
                restored += 1;
                for _ in 0..5 {
                    target.run_tick();
                }
            } else {
                refused += 1;
            }
        }
    }
    // The draws reach both outcomes, so the checks are exercised, not passed over.
    assert!(
        restored > 0 && refused > 0,
        "{restored} restored, {refused} refused"
    );
}

#[test]
fn every_order_the_proving_match_relies_on_is_stated() {
    // Bevy's sync points order the systems they lie between, so a pair that only one of them
    // keeps apart passes the ambiguity check until the systems around it change.
    let proving = ProvingMatch::load();
    let mut target = RestoreTarget::new(proving.packages(), ProvingMatch::PLAYERS);
    if let Err(error) = target.build_without_sync_points() {
        panic!("{error}");
    }
}
