use super::*;
use campfire_net_core::TickRate;

use crate::internals::protocol::CompA;
use crate::pre_spawned_plugin::PreSpawnedPlugin;
use crate::pre_spawned_receiver::PreSpawnedReceiver;
use crate::replication_receiver::ReplicationReceiver;

use bevy_app::{App, FixedMain, FixedUpdate};
use bevy_replicon::prelude::Signature;
use campfire_net_connection::Client;
use campfire_net_connection::Connected;
use campfire_net_connection::RemoteId;
use campfire_net_core::PeerId;
use campfire_net_timeline::TimelinePlugin;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(TimelinePlugin {
        tick_rate: TickRate::try_from(100).unwrap(),
    });
    app.init_resource::<ComponentRegistry>();
    app.add_plugins(PreSpawnedPlugin);
    app
}

fn connect_receiver(app: &mut App) {
    app.world_mut().spawn((
        Client,
        RemoteId(PeerId::Server),
        Connected,
        ReplicationReceiver,
    ));
}

#[derive(Resource, Default)]
struct Spawned(Vec<Entity>);

/// Runs `FixedMain` for `ticks` ticks, its `FixedUpdate` spawning `bundle` at the last one.
fn pre_spawn_at(app: &mut App, ticks: u32, bundle: impl Bundle + Clone) -> Entity {
    app.init_resource::<Spawned>();
    let last = NetTick(ticks);
    app.add_systems(
        FixedUpdate,
        move |timeline: Res<'_, LocalTimeline>,
              mut spawned: ResMut<'_, Spawned>,
              mut commands: Commands<'_, '_>| {
            if timeline.tick() == last {
                spawned.0.push(commands.spawn(bundle.clone()).id());
            }
        },
    );
    for _ in 0..ticks {
        app.world_mut().run_schedule(FixedMain);
    }
    let spawned = &app.world().resource::<Spawned>().0;
    assert_eq!(spawned.len(), 1);
    spawned[0]
}

/// A receiver's pre-spawn made in `FixedMain` at tick 3 is listed at 3 for the rollback and for
/// its expiry.
#[test]
fn client_pre_spawns_are_lifecycle_tracked() {
    let mut app = app();
    connect_receiver(&mut app);
    let pre_spawned = pre_spawn_at(&mut app, 3, PreSpawned::new(1));

    let receiver = app.world().resource::<PreSpawnedReceiver>();
    assert_eq!(receiver.unmatched, [(NetTick(3), pre_spawned)]);
    assert_eq!(receiver.rollback, [(NetTick(3), pre_spawned)]);
}

/// A receiver's pre-spawn outside `FixedMain` has no simulated tick, so a rollback could despawn
/// it with no replay that makes it again.
#[test]
#[should_panic(expected = "outside FixedMain, where no tick is simulated")]
fn a_receiver_pre_spawn_outside_fixed_main_panics() {
    let mut app = app();
    connect_receiver(&mut app);
    app.world_mut().spawn(PreSpawned::new(1));
}

/// A default hash holds the simulated tick, 5, and no other: the hashes at 4 and 6 differ. A
/// salted one holds its salt too, and a given hash is the hash from the start. The fixture
/// registers no component, so a default hash is that of the tick 5, the count 0, and the salt
/// after its tag 1, or the tag 0 alone.
#[test]
fn a_default_hash_holds_the_simulated_tick() {
    let expected = |salt: Option<u64>| {
        let mut hasher = DomainHasher::new(DEFAULT_HASH_DOMAIN);
        hasher.update(&[5, 0, 0, 0, 0, 0, 0, 0]);
        match salt {
            None => hasher.update(&[0]),
            Some(salt) => hasher.update(&[1]).update(&salt.to_le_bytes()),
        };
        hasher.finish().0
    };
    let hashes = |pre_spawned: PreSpawned, salt| {
        let mut app = app();
        let entity = pre_spawn_at(&mut app, 5, (CompA(1.0), pre_spawned));
        let world = app.world();
        let hash_at = |tick| {
            compute_default_hash(
                world.resource::<ComponentRegistry>(),
                world.components(),
                world.entity(entity).archetype(),
                tick,
                salt,
            )
        };
        let hash = world.get::<PreSpawned>(entity).unwrap().hash();
        assert_eq!(hash, Some(hash_at(NetTick(5))));
        assert_eq!(hash, Some(expected(salt)));
        assert_ne!(hash_at(NetTick(4)), hash_at(NetTick(5)));
        assert_ne!(hash_at(NetTick(6)), hash_at(NetTick(5)));
        hash
    };
    let unsalted = hashes(PreSpawned::default(), None);
    let salted = hashes(PreSpawned::default_with_salt(7), Some(7));
    assert_ne!(unsalted, salted);

    assert_eq!(PreSpawned::default().hash(), None);
    assert_eq!(PreSpawned::default_with_salt(7).hash(), None);
    assert_eq!(PreSpawned::new(3).hash(), Some(3));
}

#[test]
#[should_panic(expected = "gets a default PreSpawned hash outside FixedMain")]
fn a_default_hash_outside_fixed_main_panics() {
    let mut app = app();
    app.world_mut().spawn(PreSpawned::default());
}

/// A world with no connected client receiver, as an authoritative server, gives a pre-spawn
/// its signature but no bookkeeping, and takes an explicit hash outside `FixedMain`.
#[test]
fn pre_spawns_without_a_client_receiver_are_not_lifecycle_tracked() {
    let mut app = app();
    let pre_spawned = app.world_mut().spawn(PreSpawned::new(1)).id();
    app.update();

    assert!(app.world().get::<Signature>(pre_spawned).is_some());
    let receiver = app.world().resource::<PreSpawnedReceiver>();
    assert!(receiver.unmatched.is_empty() && receiver.rollback.is_empty());
}
