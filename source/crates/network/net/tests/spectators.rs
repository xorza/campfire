//! Spectators (design 14, D15): a link with no slot, admitted while the session holds a place
//! for it, watches every unit through any fog, or the teams' views it chooses, and holds of each
//! entity what its kind's audience gives spectators. It predicts nothing.

use std::collections::BTreeSet;

use bevy_app::App;
use bevy_ecs::query::{Allow, With};
use campfire_net::internals::{InProcessMatch, LinkModel, MatchSetup};
use campfire_net::{
    CloseCode, JoinRefused, JoinState, LinkLossCause, LinkLost, Loss, WatchRequest,
};
use campfire_sim::{EntityIndex, StableId, Unpredicted};
use campfire_stats::Life;
use campfire_units::{SpawnPoint, UnitsWorld};
use campfire_values::{Team, TeamSet};
use lightyear::prelude::Replicate;

/// Steps a spectator may take to link, answer the offer and watch.
const JOIN_STEPS: usize = 300;

/// A duo of the lane mode whose host gives `visibility` and admits `spectators`.
const fn duo(visibility: &'static [&'static str], spectators: u32) -> MatchSetup {
    MatchSetup {
        visibility,
        max_spectators: spectators,
        ..MatchSetup::duo(LinkModel::PERFECT, InProcessMatch::SEED_CHAIN)
    }
}

fn step_until(local: &mut InProcessMatch, done: impl Fn(&InProcessMatch) -> bool) {
    for _ in 0..JOIN_STEPS {
        if done(local) {
            return;
        }
        local.step();
    }
    assert!(done(local), "not done after {JOIN_STEPS} steps");
}

fn state(local: &InProcessMatch, client: usize) -> &JoinState {
    local.client(client).world().resource::<JoinState>()
}

/// The stable ids of the entities `app` holds, predicted or not.
fn held(app: &mut App) -> BTreeSet<StableId> {
    let world = app.world_mut();
    let mut ids = world.query_filtered::<&StableId, Allow<Unpredicted>>();
    ids.iter(world).copied().collect()
}

/// The stable ids of the entities the server replicates.
fn replicated(app: &mut App) -> BTreeSet<StableId> {
    let world = app.world_mut();
    let mut ids = world.query_filtered::<&StableId, With<Replicate>>();
    ids.iter(world).copied().collect()
}

/// A spectator that joins `local` and steps until it watches; its index.
fn watching(local: &mut InProcessMatch) -> usize {
    let spectator = local.add_spectator();
    step_until(local, |local| state(local, spectator).watching().is_some());
    for _ in 0..5 {
        local.step();
    }
    spectator
}

#[test]
fn a_spectator_watches_every_unit_holds_what_spectators_may_and_only_one_finds_a_place() {
    let mut local = InProcessMatch::new(duo(&[], 1));
    local.start_match();
    let spectator = watching(&mut local);

    // Every entity the server replicates, a unit no team sees among them, as the west player,
    // who sees less, shows; and nothing it predicts.
    let all = replicated(local.server_mut());
    let watched = held(local.client_mut(spectator));
    assert_eq!(watched, all);
    let west = held(local.client_mut(0));
    assert!(west.is_subset(&all) && west.len() < all.len(), "{west:?}");
    let world = local.client_mut(spectator).world_mut();
    assert_eq!(world.query::<&StableId>().iter(world).count(), 0);

    // A hero's life, which every client that holds it receives, and not its spawn point, its
    // owner's alone; the teams' relations, on the match's entity.
    let hero = local.avatar(0);
    let world = local.client(spectator).world();
    let hero = world.entity(world.resource::<EntityIndex>().get(hero).unwrap());
    assert!(hero.contains::<Life>() && !hero.contains::<SpawnPoint>());
    assert!(world.held_relations().is_some());

    // The one place is taken: a second spectator is refused, told, and stops.
    let second = local.add_spectator();
    step_until(&mut local, |local| state(local, second).loss().is_some());
    assert_eq!(state(&local, second).loss(), Some(Loss::JoinRefused));
    assert_eq!(local.log().take::<JoinRefused>().len(), 1);
    assert_eq!(
        local.log().take::<LinkLost>(),
        [LinkLost {
            cause: LinkLossCause::ClientClosed(CloseCode::JoinRefused)
        }]
    );
}

#[test]
fn a_spectator_of_the_teams_views_watches_the_teams_it_chooses() {
    // Every team's view together, as it joins; then the west's alone, as the west player sees
    // it.
    let mut local = InProcessMatch::new(duo(&["spectators=teams"], 1));
    local.start_match();
    let spectator = watching(&mut local);
    let west = held(local.client_mut(0));
    let east = held(local.client_mut(1));
    let both: BTreeSet<StableId> = west.union(&east).copied().collect();
    assert_eq!(held(local.client_mut(spectator)), both);
    assert_ne!(west, both, "the teams see apart");

    let west_alone = TeamSet::from([Team::new(0)]);
    local
        .client_mut(spectator)
        .world_mut()
        .insert_resource(WatchRequest(west_alone));
    for _ in 0..5 {
        local.step();
    }
    assert_eq!(held(local.client_mut(spectator)), held(local.client_mut(0)));

    // Its link fails and comes back: the place its old link held is free again, and it watches
    // the west's view alone again, as it chose.
    local.cut_link(spectator);
    step_until(&mut local, |local| state(local, spectator).rejoining());
    local.mend_link(spectator);
    step_until(&mut local, |local| {
        state(local, spectator).watching().is_some()
    });
    for _ in 0..5 {
        local.step();
    }
    assert_eq!(held(local.client_mut(spectator)), held(local.client_mut(0)));
}
