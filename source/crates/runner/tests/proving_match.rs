//! The proving match plays through every capability with no failed call, and proves what it is
//! there to prove: each capability it names takes part.

use campfire_capabilities::{
    Area, Dead, Experience, Level, Modifiers, Owner, Projectile, ScriptFailures, Team, TrainQueue,
};
use campfire_runner::{FixedMatch, ProvingMatch};
use campfire_sim::EntityIndex;

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
    for tick in 0..ProvingMatch::TICKS {
        ProvingMatch::play_tick(&mut fixed, tick);
        let failures = fixed.runner().world().non_send::<ScriptFailures>();
        assert!(
            failures.get().is_empty(),
            "tick {tick}: {:?}",
            failures.get()
        );
        look(&fixed, tick, &mut seen);
    }
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
}
