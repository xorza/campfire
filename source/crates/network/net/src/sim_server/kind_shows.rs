use std::fmt;
use std::marker::PhantomData;

use bevy_app::{App, FixedUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Add, RemovedComponents};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Changed, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::system::{Commands, If, Query, Res, ResMut, SystemParam};
use campfire_capabilities::Release;
use campfire_declare::{EntityKind, KindAudience, KindVisitor};
use campfire_net_connection::NetworkTarget;
use campfire_net_replication::{PredictionTarget, Replicate, Rooms};
use campfire_sim::StableId;
use campfire_units::{Owner, SeenBy};
use tracing::debug;

use crate::sim_server::match_visibility::MatchVisibility;
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::team_rooms::TeamRooms;

/// The entities of the kind `K` a show takes, with their owner and the teams that see them.
type NewOfKind<'w, 's, K> =
    Query<'w, 's, (Option<&'static Owner>, Option<&'static SeenBy>), With<K>>;

/// The entities of the kind `K` that came since its last show, which the show takes. Scratch.
#[derive(Resource)]
struct Unshown<K> {
    entities: Vec<Entity>,
    kind: PhantomData<K>,
}

impl<K> Default for Unshown<K> {
    fn default() -> Self {
        Self {
            entities: Vec::new(),
            kind: PhantomData,
        }
    }
}

impl<K> fmt::Debug for Unshown<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Unshown")
            .field("entities", &self.entities)
            .finish()
    }
}

/// The replicated entities of the kind `K` whose seers changed this tick.
type SeersChanged<'w, 's, K> = Query<
    'w,
    's,
    (Entity, &'static StableId, &'static SeenBy),
    (With<K>, Changed<SeenBy>, With<Replicate>),
>;

/// The replicated entities of the kind `K`, with their owner.
type OwnedOfKind<'w, 's, K> = Query<'w, 's, (Entity, &'static Owner), (With<K>, With<Replicate>)>;

/// The replicated entities of the kind `K`.
type ReplicatedOfKind<'w, 's, K> = Query<'w, 's, (), (With<K>, With<Replicate>)>;

/// The replicated entities of the kind `K` whose owner changed this tick.
type OwnerChanged<'w, 's, K> =
    Query<'w, 's, (Entity, &'static Owner), (With<K>, Changed<Owner>, With<Replicate>)>;

/// The entities of the kind `K` whose owner changed, or went, this tick, for a kind its owner
/// predicts. No `Debug`: `RemovedComponents` has none.
#[derive(SystemParam)]
struct OwnerChanges<'w, 's, K: EntityKind> {
    changed: OwnerChanged<'w, 's, K>,
    removed: RemovedComponents<'w, 's, Owner>,
    replicated: ReplicatedOfKind<'w, 's, K>,
}

/// The systems that show each entity to the links its kind says: one per kind of the core, each
/// after a sim tick, so a kind's audience and its owner's prediction are written once, on the
/// kind.
#[derive(Debug)]
pub(crate) struct KindShows<'a> {
    app: &'a mut App,
}

/// The set of every kind's show, in `FixedUpdate`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Shows;

impl KindShows<'_> {
    /// Adds into `app` each kind's show, in `Shows`, and for a kind its owner predicts, the
    /// prediction of each seated link's entities.
    pub(crate) fn register(app: &mut App) {
        Release::kinds(&mut KindShows { app });
    }

    /// Lists each new entity of `K` as its marker comes, for its show: it came with `Replicate`
    /// and no rooms, so no link receives it before its show puts it in its rooms.
    fn listed<K: EntityKind>(added: On<'_, '_, Add<K>>, mut unshown: ResMut<'_, Unshown<K>>) {
        unshown.entities.push(added.entity);
    }

    /// Shows each new entity of `K`, as `listed` lists it, to the links of `K`'s audience, after
    /// the sim tick it came in, and to its owner's link as the client that predicts it, when `K`
    /// says its owner predicts, again as its owner changes: the link of an owner with no seat
    /// predicts nothing, nor does any for an entity that loses its owner, and the client that
    /// predicted it takes it as the server sends it, unpredicted. Each part it writes came with
    /// the entity's marker, so a show moves no entity between archetypes. An entity whose kind
    /// vision limits stands in the rooms of the teams that see it, from the tick it replicates in,
    /// and again as they change, so a client never receives one its team did not see: a
    /// projectile shows where it flies, not where its source stands. Without vision no entity has
    /// `SeenBy`, and every seated client receives every entity. A spectator stands in the rooms
    /// its `Viewer` gives, and any other link with no seat in no room, so it receives none. What
    /// its owner alone receives, a filter of its state's kind keeps from every other link
    /// ([`DataFilters`](crate::sim_server::data_filter::DataFilters)).
    fn show<K: EntityKind>(
        rooms: Res<'_, TeamRooms>,
        If(visibility): If<Res<'_, MatchVisibility>>,
        links: Query<'_, '_, (Entity, &PlayerLink)>,
        (mut unshown, new): (ResMut<'_, Unshown<K>>, NewOfKind<'_, '_, K>),
        changed: SeersChanged<'_, '_, K>,
        mut owners: OwnerChanges<'_, '_, K>,
        mut commands: Commands<'_, '_>,
    ) {
        for entity in unshown.entities.drain(..) {
            let Ok((owner, seen)) = new.get(entity) else {
                continue;
            };
            let seen = match K::AUDIENCE {
                KindAudience::Seers => seen,
                KindAudience::EverySeat => None,
            };
            let mut replicated = commands.entity(entity);
            replicated.insert(rooms.seeing(seen, &visibility.0));
            let link = owner
                .filter(|_| K::OWNER_PREDICTS)
                .and_then(|owner| KindShows::link_of(&links, *owner));
            if let Some(link) = link {
                replicated.insert(PredictionTarget::to_links([link]));
            }
        }
        if K::AUDIENCE == KindAudience::Seers {
            for (entity, &id, seen) in &changed {
                debug!(entity = id.get(), "the teams that see an entity changed");
                commands
                    .entity(entity)
                    .insert(rooms.seeing(Some(seen), &visibility.0));
            }
        }
        if K::OWNER_PREDICTS {
            for (entity, owner) in &owners.changed {
                commands
                    .entity(entity)
                    .insert(PredictionTarget::to_links(KindShows::link_of(
                        &links, *owner,
                    )));
            }
            for entity in owners.removed.read() {
                if owners.replicated.contains(entity) {
                    commands
                        .entity(entity)
                        .insert(PredictionTarget::to_links([]));
                }
            }
        }
    }

    /// The link seated in the seat of `owner`, if one is.
    fn link_of(links: &Query<'_, '_, (Entity, &PlayerLink)>, owner: Owner) -> Option<Entity> {
        links
            .iter()
            .find(|(_, link)| link.seat() == owner.seat())
            .map(|(link, _)| link)
    }

    /// Makes the link seated as `added` says the client that predicts each replicated entity of
    /// `K` its seat owns, as a player takes their seat again.
    fn predict_seated<K: EntityKind>(
        added: On<'_, '_, Add<PlayerLink>>,
        links: Query<'_, '_, &PlayerLink>,
        owned: OwnedOfKind<'_, '_, K>,
        mut commands: Commands<'_, '_>,
    ) {
        let link = added.entity;
        let seat = links.get(link).expect("an added link").seat();
        for (entity, owner) in &owned {
            if owner.seat() == seat {
                commands
                    .entity(entity)
                    .insert(PredictionTarget::to_links([link]));
            }
        }
    }
}

impl KindVisitor for KindShows<'_> {
    fn kind<K: EntityKind>(&mut self) {
        let world = self.app.world_mut();
        world.register_required_components_with::<K, Replicate>(|| {
            Replicate::to_clients(NetworkTarget::All)
        });
        world.register_required_components::<K, Rooms>();
        if K::OWNER_PREDICTS {
            world.register_required_components_with::<K, PredictionTarget>(|| {
                PredictionTarget::to_links([])
            });
        }
        world.init_resource::<Unshown<K>>();
        self.app.add_observer(KindShows::listed::<K>);
        self.app
            .add_systems(FixedUpdate, KindShows::show::<K>.in_set(Shows));
        if K::OWNER_PREDICTS {
            self.app.add_observer(KindShows::predict_seated::<K>);
        }
    }

    fn state<K: EntityKind, S: Component>(&mut self) {}
}
