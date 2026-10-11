use bevy_app::{App, FixedUpdate};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Add, RemovedComponents};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Changed, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::system::{Commands, Query, Res, SystemParam};
use campfire_capabilities::{EntityKind, KindAudience, KindVisitor, Owner, Release, SeenBy};
use campfire_sim::StableId;
use lightyear::prelude::{NetworkTarget, PredictionTarget, Replicate};
use tracing::debug;

use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::team_rooms::TeamRooms;

/// The entities of the kind `K` not replicated yet, with their owner and the teams that see them.
type NewOfKind<'w, 's, K> = Query<
    'w,
    's,
    (Entity, Option<&'static Owner>, Option<&'static SeenBy>),
    (<K as EntityKind>::Only, Without<Replicate>),
>;

/// The replicated entities of the kind `K` whose seers changed this tick.
type SeersChanged<'w, 's, K> = Query<
    'w,
    's,
    (Entity, &'static StableId, &'static SeenBy),
    (<K as EntityKind>::Only, Changed<SeenBy>, With<Replicate>),
>;

/// The replicated entities of the kind `K`, with their owner.
type OwnedOfKind<'w, 's, K> =
    Query<'w, 's, (Entity, &'static Owner), (<K as EntityKind>::Only, With<Replicate>)>;

/// The replicated entities of the kind `K`.
type ReplicatedOfKind<'w, 's, K> = Query<'w, 's, (), (<K as EntityKind>::Only, With<Replicate>)>;

/// The replicated entities of the kind `K` whose owner changed this tick.
type OwnerChanged<'w, 's, K> = Query<
    'w,
    's,
    (Entity, &'static Owner),
    (<K as EntityKind>::Only, Changed<Owner>, With<Replicate>),
>;

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

    /// Replicates each new entity of `K` to the links of `K`'s audience, and to its owner's link
    /// as the client that predicts it, when `K` says its owner predicts, again as its owner
    /// changes: the link of an owner with no seat predicts nothing, nor does any for an entity
    /// that loses its owner, and the client that predicted it takes it as the server sends it,
    /// unpredicted. An entity whose kind vision limits stands in the rooms of the teams that see
    /// it, from the tick it replicates in, and again as they change, so a client never receives one its team did not see: a
    /// projectile shows where it flies, not where its source stands. Without vision no entity has
    /// `SeenBy`, and every seated client receives every entity; a link with no seat stands in no
    /// room, and receives none. What its owner alone receives, a filter of its state's kind keeps
    /// from every other link ([`KindFilters`](crate::sim_server::kind_filter::KindFilters)).
    fn show<K: EntityKind>(
        rooms: Res<'_, TeamRooms>,
        links: Query<'_, '_, (Entity, &PlayerLink)>,
        new: NewOfKind<'_, '_, K>,
        changed: SeersChanged<'_, '_, K>,
        mut owners: OwnerChanges<'_, '_, K>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, owner, seen) in &new {
            let seen = match K::AUDIENCE {
                KindAudience::Seers => seen,
                KindAudience::EverySeat => None,
            };
            let mut replicated = commands.entity(entity);
            replicated.insert((
                Replicate::to_clients(NetworkTarget::All),
                rooms.seeing(seen),
            ));
            let link = owner
                .filter(|_| K::OWNER_PREDICTS)
                .and_then(|owner| KindShows::link_of(&links, *owner));
            if let Some(link) = link {
                replicated.insert(PredictionTarget::manual(vec![link]));
            }
        }
        if K::AUDIENCE == KindAudience::Seers {
            for (entity, &id, seen) in &changed {
                debug!(entity = id.get(), "the teams that see an entity changed");
                commands.entity(entity).insert(rooms.seeing(Some(seen)));
            }
        }
        if K::OWNER_PREDICTS {
            for (entity, owner) in &owners.changed {
                match KindShows::link_of(&links, *owner) {
                    Some(link) => commands
                        .entity(entity)
                        .insert(PredictionTarget::manual(vec![link])),
                    None => commands.entity(entity).remove::<PredictionTarget>(),
                };
            }
            for entity in owners.removed.read() {
                if owners.replicated.contains(entity) {
                    commands.entity(entity).remove::<PredictionTarget>();
                }
            }
        }
    }

    /// The link seated in the slot of `owner`, if one is.
    fn link_of(links: &Query<'_, '_, (Entity, &PlayerLink)>, owner: Owner) -> Option<Entity> {
        links
            .iter()
            .find(|(_, link)| link.slot() == owner.slot())
            .map(|(link, _)| link)
    }

    /// Makes the link seated as `added` says the client that predicts each replicated entity of
    /// `K` its slot owns, as a player takes their seat again.
    fn predict_seated<K: EntityKind>(
        added: On<'_, '_, Add, PlayerLink>,
        links: Query<'_, '_, &PlayerLink>,
        owned: OwnedOfKind<'_, '_, K>,
        mut commands: Commands<'_, '_>,
    ) {
        let link = added.entity;
        let slot = links.get(link).expect("an added link").slot();
        for (entity, owner) in &owned {
            if owner.slot() == slot {
                commands
                    .entity(entity)
                    .insert(PredictionTarget::manual(vec![link]));
            }
        }
    }
}

impl KindVisitor for KindShows<'_> {
    fn kind<K: EntityKind>(&mut self) {
        self.app
            .add_systems(FixedUpdate, KindShows::show::<K>.in_set(Shows));
        if K::OWNER_PREDICTS {
            self.app.add_observer(KindShows::predict_seated::<K>);
        }
    }
}
