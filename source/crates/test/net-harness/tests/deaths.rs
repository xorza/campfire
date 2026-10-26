//! A death over the network: a client that holds a unit learns its death, through the server's
//! longest frame too.

use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::Allow;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut};
use campfire_harness::MatchUnits;
use campfire_math::Num;
use campfire_navigation::Progress;
use campfire_net_emulator::LinkModel;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_sim::{SimTick, SimWorld, StableId, Unpredicted};
use campfire_units::{Dead, Life, Owner};
use campfire_values::Team;

/// The units the client saw gain `Dead`, in the order it saw them.
#[derive(Resource, Debug, Default)]
struct SeenDead(Vec<StableId>);

/// Records the unit `Dead` reached.
fn see(
    added: On<'_, '_, Add<Dead>>,
    ids: Query<'_, '_, &StableId, Allow<Unpredicted>>,
    mut seen: ResMut<'_, SeenDead>,
) {
    if let Ok(&id) = ids.get(added.entity) {
        seen.0.push(id);
    }
}

#[test]
fn a_death_reaches_the_client_through_the_servers_longest_frame() {
    let mut local = InProcessMatch::new(InProcessSetup::duo(
        LinkModel::perfect(),
        InProcessMatch::SEED_CHAIN,
    ));
    local.start_match();
    let client = local.client_mut(0);
    client.init_resource::<SeenDead>();
    client.add_observer(see);
    // A west creep, which the client sees walk and whose type despawns at death.
    let creep = |local: &InProcessMatch| {
        let mut units = MatchUnits::of(local.server().world()).all();
        let found = units.find(|(_, unit)| {
            unit.contains::<Progress>()
                && !unit.contains::<Owner>()
                && unit.get::<Team>() == Some(&Team::new(0))
        });
        found.map(|(id, unit)| (id, unit.id()))
    };
    while creep(&local).is_none() {
        local.step();
    }
    let (id, entity) = creep(&local).unwrap();
    local.step();
    assert!(local.client(0).world().entity_of(id).is_some());

    // It dies in the first tick of a frame of 9 ticks, the most a server frame runs: the frame's
    // send holds its death, and the client learns it.
    local.server_fixed_update(move |world| {
        world
            .get_mut::<Life>(entity)
            .unwrap()
            .take(Num::int(1_000_000));
    });
    let before = local.server().world().resource::<SimTick>().start();
    local.stall_server(9);
    let ran = local.server().world().resource::<SimTick>().start().get() - before.get();
    assert_eq!(ran, SimTick::FRAME_MOST.get());
    assert!(local.unit(End::Server, id).contains::<Dead>());
    local.step();
    assert_eq!(local.client(0).world().resource::<SeenDead>().0, [id]);
    // It goes at the end of the 9th tick after its death, on the server and then on the client.
    local.step();
    assert!(local.server().world().entity_of(id).is_none());
    local.step();
    assert!(local.client(0).world().entity_of(id).is_none());
}
