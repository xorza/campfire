use bevy_replicon::prelude::Remote;
use campfire_net_core::NetTick;
use campfire_net_core::TickInstant;

use super::*;

fn setup_app(interpolation_tick: NetTick) -> App {
    let mut app = App::new();
    let mut timeline = InterpolationTimeline::default();
    timeline.set_now(TickInstant::from(interpolation_tick));
    app.insert_resource(timeline);
    app.add_systems(Update, InterpolationPlugin::despawn_interpolated_entities);
    app
}

/// An entity the server despawned at tick 10 stays while the interpolation timeline shows tick
/// 9, and goes at tick 10 as a replicated despawn: its `Remote` child stays and loses its
/// `ChildOf`, and its local child goes with it.
#[test]
fn a_delayed_despawn_waits_for_the_interpolation_tick() {
    let mut app = setup_app(NetTick(9));
    let entity = app.world_mut().spawn(DelayedDespawn::at(NetTick(10))).id();
    let remote_child = app.world_mut().spawn((Remote, ChildOf(entity))).id();
    let local_child = app.world_mut().spawn(ChildOf(entity)).id();

    app.update();
    assert!(app.world().get_entity(entity).is_ok());

    app.world_mut()
        .resource_mut::<InterpolationTimeline>()
        .set_now(TickInstant::from(NetTick(10)));
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    assert!(app.world().get_entity(local_child).is_err());
    let remote_child = app.world().entity(remote_child);
    assert!(!remote_child.contains::<ChildOf>());
}
