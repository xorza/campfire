use bevy_replicon::server::visibility::filters_mask::FilterBit;
use bevy_replicon::server::visibility::registry::FilterRegistry;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;

use super::*;
use crate::network_visibility_plugin::NetworkVisibilityPlugin;
use crate::room_id::RoomId;
use crate::visibility_ext::VisibilityExt;

fn inheritance_app() -> App {
    let mut app = App::new();
    app.add_plugins(InheritancePlugin);
    app
}

fn value<T: Aspect>(app: &App, entity: Entity) -> Option<T> {
    app.world().get::<T>(entity).cloned()
}

fn is_overridden<A: 'static>(app: &App, entity: Entity) -> bool {
    app.world().get::<Overridden<A>>(entity).is_some()
}

fn insert(app: &mut App, entity: Entity, bundle: impl Bundle) {
    app.world_mut().entity_mut(entity).insert(bundle);
    app.world_mut().flush();
}

fn remove<B: Bundle>(app: &mut App, entity: Entity) {
    app.world_mut().entity_mut(entity).remove::<B>();
    app.world_mut().flush();
}

fn spawn(app: &mut App, bundle: impl Bundle) -> Entity {
    let entity = app.world_mut().spawn(bundle).id();
    app.world_mut().flush();
    entity
}

/// Three different values of each inherited component.
fn replicates(app: &mut App) -> [Replicate; 3] {
    let link = app.world_mut().spawn_empty().id();
    [
        Replicate::to_links([]),
        Replicate::default(),
        Replicate::to_links([link]),
    ]
}

fn predictions(app: &mut App) -> [PredictionTarget; 3] {
    let link = app.world_mut().spawn_empty().id();
    [
        PredictionTarget::to_links([]),
        PredictionTarget::default(),
        PredictionTarget::to_links([link]),
    ]
}

fn interpolations(app: &mut App) -> [InterpolationTarget; 3] {
    let link = app.world_mut().spawn_empty().id();
    [
        InterpolationTarget::to_links([]),
        InterpolationTarget::default(),
        InterpolationTarget::to_links([link]),
    ]
}

fn rooms() -> [Rooms; 3] {
    [
        Rooms::single(RoomId(0)),
        Rooms::single(RoomId(1)),
        Rooms::single(RoomId(2)),
    ]
}

/// Runs each check once, on a fresh app, with three different values of its component.
fn for_each_aspect(
    check_replicate: fn(App, [Replicate; 3]),
    check_prediction: fn(App, [PredictionTarget; 3]),
    check_interpolation: fn(App, [InterpolationTarget; 3]),
    check_rooms: fn(App, [Rooms; 3]),
) {
    let mut app = inheritance_app();
    let values = replicates(&mut app);
    check_replicate(app, values);
    let mut app = inheritance_app();
    let values = predictions(&mut app);
    check_prediction(app, values);
    let mut app = inheritance_app();
    let values = interpolations(&mut app);
    check_interpolation(app, values);
    check_rooms(inheritance_app(), rooms());
}

/// A joiner without the aspect follows the root, one with the root's value follows it too, and
/// one with another value, or with the marker already, keeps what it has.
fn join<T: Aspect>(mut app: App, [a, b, _]: [T; 3]) {
    let root = spawn(&mut app, a.clone());
    let plain = spawn(&mut app, ReplicateLike { root });
    let equal = spawn(&mut app, (a.clone(), ReplicateLike { root }));
    let own = spawn(&mut app, (b.clone(), ReplicateLike { root }));
    let marked = spawn(
        &mut app,
        (Overridden::<T>::default(), ReplicateLike { root }),
    );

    assert_eq!(value::<T>(&app, plain), Some(a.clone()));
    assert!(!is_overridden::<T>(&app, plain));
    assert_eq!(value::<T>(&app, equal), Some(a));
    assert!(!is_overridden::<T>(&app, equal));
    assert_eq!(value::<T>(&app, own), Some(b));
    assert!(is_overridden::<T>(&app, own));
    assert_eq!(value::<T>(&app, marked), None);
    assert!(is_overridden::<T>(&app, marked));
}

#[test]
fn a_joiner_keeps_a_differing_aspect_and_follows_the_rest() {
    for_each_aspect(join, join, join, join);
}

/// A root's insert, replace and removal reach the members that follow it, absence included, and
/// never an override.
fn root_writes<T: Aspect>(mut app: App, [a, b, c]: [T; 3]) {
    let root = spawn(&mut app, a.clone());
    let plain = spawn(&mut app, ReplicateLike { root });
    let own = spawn(&mut app, (c.clone(), ReplicateLike { root }));

    insert(&mut app, root, b.clone());
    assert_eq!(value::<T>(&app, plain), Some(b));
    assert_eq!(value::<T>(&app, own), Some(c.clone()));

    remove::<T>(&mut app, root);
    assert_eq!(value::<T>(&app, plain), None);
    assert!(!is_overridden::<T>(&app, plain));
    assert_eq!(value::<T>(&app, own), Some(c.clone()));

    insert(&mut app, root, a.clone());
    assert_eq!(value::<T>(&app, plain), Some(a));
    assert_eq!(value::<T>(&app, own), Some(c));
    assert!(is_overridden::<T>(&app, own));
}

#[test]
fn root_writes_reach_the_members_that_follow() {
    for_each_aspect(root_writes, root_writes, root_writes, root_writes);
}

/// A member write that differs from the root's overrides it, a removal included; one equal to
/// the root's follows it again, and so does the removal of the marker.
fn member_writes<T: Aspect>(mut app: App, [a, b, c]: [T; 3]) {
    let root = spawn(&mut app, a.clone());
    let member = spawn(&mut app, ReplicateLike { root });

    insert(&mut app, member, b.clone());
    assert!(is_overridden::<T>(&app, member));
    insert(&mut app, root, c.clone());
    assert_eq!(value::<T>(&app, member), Some(b.clone()));

    insert(&mut app, member, c);
    assert!(!is_overridden::<T>(&app, member));
    insert(&mut app, root, a.clone());
    assert_eq!(value::<T>(&app, member), Some(a));

    remove::<T>(&mut app, member);
    assert!(is_overridden::<T>(&app, member));
    insert(&mut app, root, b.clone());
    assert_eq!(value::<T>(&app, member), None);

    remove::<Overridden<T>>(&mut app, member);
    assert_eq!(value::<T>(&app, member), Some(b));
    assert!(!is_overridden::<T>(&app, member));
}

#[test]
fn a_member_write_overrides_when_it_differs_from_the_root() {
    for_each_aspect(member_writes, member_writes, member_writes, member_writes);
}

/// A member moved to another root follows the new root's value, or its absence, and an override
/// moves unchanged.
fn reattach<T: Aspect>(mut app: App, [a, b, c]: [T; 3]) {
    let root_a = spawn(&mut app, a);
    let root_b = spawn(&mut app, b.clone());
    let root_none = spawn(&mut app, ());
    let plain = spawn(&mut app, ReplicateLike { root: root_a });
    let own = spawn(&mut app, (c.clone(), ReplicateLike { root: root_a }));

    insert(&mut app, plain, ReplicateLike { root: root_b });
    insert(&mut app, own, ReplicateLike { root: root_b });
    assert_eq!(value::<T>(&app, plain), Some(b));
    assert!(!is_overridden::<T>(&app, plain));
    assert_eq!(value::<T>(&app, own), Some(c.clone()));
    assert!(is_overridden::<T>(&app, own));

    insert(&mut app, plain, ReplicateLike { root: root_none });
    insert(&mut app, own, ReplicateLike { root: root_none });
    assert_eq!(value::<T>(&app, plain), None);
    assert!(!is_overridden::<T>(&app, plain));
    assert_eq!(value::<T>(&app, own), Some(c));
}

#[test]
fn a_relinked_member_follows_its_new_root() {
    for_each_aspect(reattach, reattach, reattach, reattach);
}

/// An entity that leaves its root keeps its overrides, which lose their marker, and drops or
/// keeps each copy it followed as `T::ON_DETACH` says.
fn detach<T: Aspect>(mut app: App, [a, b, _]: [T; 3]) {
    let root = spawn(&mut app, a.clone());
    let plain = spawn(&mut app, ReplicateLike { root });
    let own = spawn(&mut app, (b.clone(), ReplicateLike { root }));

    remove::<ReplicateLike>(&mut app, plain);
    remove::<ReplicateLike>(&mut app, own);
    let kept = match T::ON_DETACH {
        OnDetach::Drop => None,
        OnDetach::Keep => Some(a),
    };
    assert_eq!(value::<T>(&app, plain), kept);
    assert_eq!(value::<T>(&app, own), Some(b));
    assert!(!is_overridden::<T>(&app, own));
}

/// Detaching never widens what an entity replicates: the copies that let it replicate go, the
/// copies that limit where it replicates stay.
#[test]
fn a_detached_entity_keeps_only_the_copies_that_limit_it() {
    assert_eq!(Replicate::ON_DETACH, OnDetach::Drop);
    assert_eq!(PredictionTarget::ON_DETACH, OnDetach::Drop);
    assert_eq!(InterpolationTarget::ON_DETACH, OnDetach::Drop);
    assert_eq!(Rooms::ON_DETACH, OnDetach::Keep);
    for_each_aspect(detach, detach, detach, detach);
}

/// App with visibility inheritance and one fake sender, without networking.
fn visibility_app() -> (App, Entity) {
    let mut app = inheritance_app();
    app.init_resource::<FilterRegistry>();
    app.init_resource::<ReplicationRegistry>();
    app.add_plugins(NetworkVisibilityPlugin);
    let sender = app.world_mut().spawn(ClientVisibility::default()).id();
    (app, sender)
}

/// The manual bit that hides `entity` from `sender`, if any.
fn hidden_by(app: &App, sender: Entity, entity: Entity) -> Option<FilterBit> {
    let bits = *app.world().resource::<VisibilityBits>();
    let visibility = app.world().get::<ClientVisibility>(sender).unwrap();
    let mask = visibility.get(entity);
    bits.iter().find(|&bit| mask.contains(bit))
}

#[test]
fn a_joiner_keeps_a_differing_manual_visibility_and_follows_otherwise() {
    let (mut app, sender) = visibility_app();
    let bits = *app.world().resource::<VisibilityBits>();
    let root = spawn(&mut app, ());
    app.world_mut().lose_visibility(root, sender);

    let plain = spawn(&mut app, ReplicateLike { root });
    assert_eq!(hidden_by(&app, sender, plain), Some(bits.while_visible));
    assert!(!is_overridden::<ManualVisibility>(&app, plain));

    let equal = spawn(&mut app, ());
    app.world_mut().lose_visibility(equal, sender);
    insert(&mut app, equal, ReplicateLike { root });
    assert_eq!(hidden_by(&app, sender, equal), Some(bits.while_visible));
    assert!(!is_overridden::<ManualVisibility>(&app, equal));

    let own = spawn(&mut app, ());
    app.world_mut().lose_visibility_retained(own, sender);
    insert(&mut app, own, ReplicateLike { root });
    assert_eq!(
        hidden_by(&app, sender, own),
        Some(bits.after_first_visibility)
    );
    assert!(is_overridden::<ManualVisibility>(&app, own));

    let marked = spawn(
        &mut app,
        (
            Overridden::<ManualVisibility>::default(),
            ReplicateLike { root },
        ),
    );
    assert_eq!(hidden_by(&app, sender, marked), None);
}

#[test]
fn a_relinked_member_follows_its_new_root_manual_visibility() {
    let (mut app, sender) = visibility_app();
    let bits = *app.world().resource::<VisibilityBits>();
    let root_a = spawn(&mut app, ());
    let root_b = spawn(&mut app, ());
    let plain = spawn(&mut app, ReplicateLike { root: root_a });
    let own = spawn(&mut app, ReplicateLike { root: root_a });
    app.world_mut().lose_visibility(root_a, sender);
    app.world_mut().gain_visibility(own, sender);
    assert_eq!(hidden_by(&app, sender, plain), Some(bits.while_visible));

    insert(&mut app, plain, ReplicateLike { root: root_b });
    insert(&mut app, own, ReplicateLike { root: root_b });
    assert_eq!(hidden_by(&app, sender, plain), None);
    assert!(is_overridden::<ManualVisibility>(&app, own));

    app.world_mut().lose_visibility_retained(root_b, sender);
    assert_eq!(
        hidden_by(&app, sender, plain),
        Some(bits.after_first_visibility)
    );
    assert_eq!(hidden_by(&app, sender, own), None);
}

/// Manual visibility limits where an entity replicates, so a detached entity keeps the state it
/// followed.
#[test]
fn a_detached_entity_keeps_its_manual_visibility() {
    let (mut app, sender) = visibility_app();
    let bits = *app.world().resource::<VisibilityBits>();
    let root = spawn(&mut app, ());
    let plain = spawn(&mut app, ReplicateLike { root });
    let own = spawn(&mut app, ReplicateLike { root });
    app.world_mut().lose_visibility(root, sender);
    app.world_mut().lose_visibility_always_present(own, sender);

    remove::<ReplicateLike>(&mut app, plain);
    remove::<ReplicateLike>(&mut app, own);
    assert_eq!(hidden_by(&app, sender, plain), Some(bits.while_visible));
    assert_eq!(hidden_by(&app, sender, own), Some(bits.always_present));
    assert!(!is_overridden::<ManualVisibility>(&app, own));
}
