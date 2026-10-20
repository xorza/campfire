use super::*;
use crate::receive_map_view::ReceiveMapView;
use crate::send_map_view::SendMapView;
use bevy_ecs::component::Component;
use bevy_ecs::entity::{EntityMapper, hash_map::EntityHashMap};
use bevy_ecs::query::With;
use bevy_ecs::world::World;

fn local_pair() -> (SendEntityMap, ReceiveEntityMap, Entity, Entity) {
    let local = Entity::from_raw_u32(1).unwrap();
    let remote = Entity::from_raw_u32(2).unwrap();
    let mut send = SendEntityMap::default();
    send.0.insert(local, remote);
    let mut receive = ReceiveEntityMap::default();
    receive.0.insert(remote, local);
    (send, receive, local, remote)
}

/// A client's send view writes the server's entity, from the shared map before the link's own
/// pairs; an entity neither holds goes as the placeholder, and a server's view keeps each entity.
#[test]
fn a_send_view_maps_only_on_a_client() {
    let (send, _, local, remote) = local_pair();
    let other = Entity::from_raw_u32(7).unwrap();
    let unknown = Entity::from_raw_u32(3).unwrap();
    let shared_remote = Entity::from_raw_u32(9).unwrap();
    let shared = EntityHashMap::from_iter([(other, shared_remote), (local, shared_remote)]);

    let mut view = SendMapView::Lookup {
        shared: None,
        local: &send,
    };
    assert_eq!(view.get_mapped(local), remote);
    assert_eq!(view.get_mapped(unknown), Entity::PLACEHOLDER);

    let mut view = SendMapView::Lookup {
        shared: Some(&shared),
        local: &send,
    };
    assert_eq!(view.get_mapped(local), shared_remote);
    assert_eq!(view.get_mapped(other), shared_remote);
    let only_other = EntityHashMap::from_iter([(other, shared_remote)]);
    let mut view = SendMapView::Lookup {
        shared: Some(&only_other),
        local: &send,
    };
    assert_eq!(view.get_mapped(local), remote);

    let mut view = SendMapView::AsIs;
    assert_eq!(view.get_mapped(local), local);
    assert_eq!(view.get_mapped(unknown), unknown);
}

/// The receive side of the same contract: a client looks up the shared map, then the link's own
/// pairs, else the placeholder; a server's view keeps each entity.
#[test]
fn a_receive_view_maps_only_on_a_client() {
    let (_, receive, local, remote) = local_pair();
    let unknown = Entity::from_raw_u32(3).unwrap();
    let shared_local = Entity::from_raw_u32(9).unwrap();
    let shared = EntityHashMap::from_iter([(remote, shared_local)]);

    let mut view = ReceiveMapView::Lookup {
        shared: None,
        local: &receive,
    };
    assert_eq!(view.get_mapped(remote), local);
    assert_eq!(view.get_mapped(unknown), Entity::PLACEHOLDER);

    let mut view = ReceiveMapView::Lookup {
        shared: Some(&shared),
        local: &receive,
    };
    assert_eq!(view.get_mapped(remote), shared_local);
}

#[derive(Component)]
struct Marked;

/// A server's view keeps an entity the check holds or the link's map knows as its own, and makes
/// any other the placeholder: an unmarked one, a despawned one, and the placeholder itself.
#[test]
fn a_checked_view_keeps_only_what_the_check_holds_or_the_link_knows() {
    let mut world = World::new();
    let marked = world.spawn(Marked).id();
    let own = world.spawn_empty().id();
    let unmarked = world.spawn_empty().id();
    let despawned = world.spawn(Marked).id();
    world.despawn(despawned);
    let mut known = SendEntityMap::default();
    known.insert(own, Entity::from_raw_u32(1).unwrap());
    let mut state = world.query_filtered::<(), With<Marked>>();
    let query = state.query(&world);

    let mut view = ReceiveMapView::Checked {
        known: &known,
        replicated: &query,
    };
    assert_eq!(view.get_mapped(marked), marked);
    assert_eq!(view.get_mapped(own), own);
    for refused in [unmarked, despawned, Entity::PLACEHOLDER] {
        assert_eq!(view.get_mapped(refused), Entity::PLACEHOLDER, "{refused:?}");
    }
}

/// Remapping remote 1 from local 10 to local 20 drops local 10's reverse entry; mapping
/// remote 2 to local 20 then drops remote 1. Each side ends with only the pair (2, 20).
#[test]
fn a_remap_keeps_the_bijection() {
    let entity = |index| Entity::from_raw_u32(index).unwrap();
    let mut map = RemoteEntityMap::default();
    map.insert(entity(1), entity(10));
    map.insert(entity(1), entity(20));
    assert_eq!(map.local_to_remote().get(&entity(10)), None);
    assert_eq!(map.local_to_remote().get(&entity(20)), Some(&entity(1)));

    map.insert(entity(2), entity(20));
    assert_eq!(map.remote_to_local().get(&entity(1)), None);
    assert_eq!(map.remote_to_local().len(), 1);
    assert_eq!(map.local_to_remote().len(), 1);
    assert_eq!(map.get_local(entity(2)), Some(entity(20)));
}
