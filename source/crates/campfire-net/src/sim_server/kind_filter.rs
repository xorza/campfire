use std::fmt::Debug;
use std::marker::PhantomData;

use bevy_app::App;
use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Add, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Has, With};
use bevy_ecs::system::{Commands, Query};
use bevy_replicon::prelude::{AppVisibilityExt, FilterScope, SingleComponent, VisibilityFilter};
use campfire_capabilities::{
    CapabilitySet, DataKind, Declared, Experience, Inventory, ModifierClocks, Owner, Player,
    PlayerResources, Points, Pools, Progress, Release, Replication, Respawn, Route, SlotTimes,
    SpawnPoint, StateTypes, SyncOwner, SyncTeam, Team, UnitState,
};
use campfire_common::PlayerSlot;
use campfire_sim::SimResource;

use crate::sim_server::player_link::PlayerLink;

/// A kind of state that only its owner's client, and with `ALLIES` its team's other clients,
/// receive, and the types of the kind, which its filter hides from every other client. A test
/// checks each scope holds exactly the types the capabilities' lists give the kind.
pub(crate) trait FilteredKind: Send + Sync + Debug + 'static {
    const KIND: DataKind;
    /// Whether the clients of the entity's team receive it, beside its owner's.
    const ALLIES: bool;
    type Scope: FilterScope;
}

/// What only the owner's prediction reads.
#[derive(Debug)]
pub(crate) struct PredictionKind;

impl FilteredKind for PredictionKind {
    const KIND: DataKind = DataKind::Prediction;
    const ALLIES: bool = false;
    type Scope = (SpawnPoint, Route, Progress, ModifierClocks, Respawn);
}

/// A unit's pools other than its life.
#[derive(Debug)]
pub(crate) struct PoolsKind;

impl FilteredKind for PoolsKind {
    const KIND: DataKind = DataKind::Pools;
    const ALLIES: bool = false;
    type Scope = SingleComponent<Pools>;
}

/// A unit's slots' ready ticks, charges and toggles.
#[derive(Debug)]
pub(crate) struct CooldownsKind;

impl FilteredKind for CooldownsKind {
    const KIND: DataKind = DataKind::Cooldowns;
    const ALLIES: bool = false;
    type Scope = SingleComponent<SlotTimes>;
}

/// A unit's inventory.
#[derive(Debug)]
pub(crate) struct InventoryKind;

impl FilteredKind for InventoryKind {
    const KIND: DataKind = DataKind::Inventory;
    const ALLIES: bool = false;
    type Scope = SingleComponent<Inventory>;
}

/// A unit's experience and points.
#[derive(Debug)]
pub(crate) struct ProgressionKind;

impl FilteredKind for ProgressionKind {
    const KIND: DataKind = DataKind::Progression;
    const ALLIES: bool = false;
    type Scope = (Experience, Points);
}

/// A player's resources.
#[derive(Debug)]
pub(crate) struct ResourcesKind;

impl FilteredKind for ResourcesKind {
    const KIND: DataKind = DataKind::Resources;
    const ALLIES: bool = false;
    type Scope = SingleComponent<PlayerResources>;
}

/// A unit's script state fields of `sync = "owner"`.
#[derive(Debug)]
pub(crate) struct UnitStateOwnerKind;

impl FilteredKind for UnitStateOwnerKind {
    const KIND: DataKind = DataKind::UnitStateOwner;
    const ALLIES: bool = false;
    type Scope = SingleComponent<UnitState<SyncOwner>>;
}

/// A unit's script state fields of `sync = "team"`.
#[derive(Debug)]
pub(crate) struct UnitStateTeamKind;

impl FilteredKind for UnitStateTeamKind {
    const KIND: DataKind = DataKind::UnitStateTeam;
    const ALLIES: bool = true;
    type Scope = SingleComponent<UnitState<SyncTeam>>;
}

/// Each entity's owner, a unit's or a player's entity's slot, its team, and whether it holds the
/// filter of `K`.
type Owners<'w, 's, K> = Query<
    'w,
    's,
    (
        Option<&'static Owner>,
        Option<&'static Player>,
        Option<&'static Team>,
        Has<KindFilter<K>>,
    ),
>;

/// On each entity that holds a type of kind `K`: the slot that owns it, if one does, a player's
/// entity its own slot, and its team, if it has one. Replicon reads it to send the kind's types to the clients `K` names
/// alone. Immutable, as replicon requires: a change of owner or team inserts a new one.
#[derive(Component, Debug)]
#[component(immutable)]
pub(crate) struct KindFilter<K: FilteredKind> {
    owner: Option<PlayerSlot>,
    team: Option<Team>,
    kind: PhantomData<K>,
}

impl<K: FilteredKind> KindFilter<K> {
    const fn new(owner: Option<&Owner>, team: Option<&Team>) -> KindFilter<K> {
        KindFilter {
            owner: match owner {
                Some(owner) => Some(owner.slot()),
                None => None,
            },
            team: team.copied(),
            kind: PhantomData,
        }
    }

    /// The filter of an entity of `owner` or, a player's entity, of `player`, and of `team`.
    fn of(owner: Option<&Owner>, player: Option<&Player>, team: Option<&Team>) -> KindFilter<K> {
        let owner = owner
            .copied()
            .or(player.map(|player| Owner::new(player.slot())));
        KindFilter::new(owner.as_ref(), team)
    }

    /// The filter of `K`, and the observers that follow its entity's owner and team.
    fn register_kind(app: &mut App) {
        app.add_visibility_filter::<KindFilter<K>>();
        KindFilter::<K>::follow(app);
    }

    /// Inserts the filter of `K` again as its entity's owner or team changes, or as the entity
    /// loses either, on an entity that holds a filter. Each insert of a filter keeps to an entity
    /// that still lives: it applies at the next flush, after a despawn queued before it, and a
    /// despawn removes the owner and team too.
    fn follow(app: &mut App) {
        KindFilter::<K>::follow_insert::<Owner>(app);
        KindFilter::<K>::follow_insert::<Team>(app);
        app.add_observer(
            |removed: On<'_, '_, Remove, Owner>,
             units: Query<'_, '_, Option<&Team>, With<KindFilter<K>>>,
             mut commands: Commands<'_, '_>| {
                if let Ok(team) = units.get(removed.entity) {
                    commands
                        .entity(removed.entity)
                        .try_insert(KindFilter::<K>::new(None, team));
                }
            },
        );
        app.add_observer(
            |removed: On<'_, '_, Remove, Team>,
             units: Query<'_, '_, Option<&Owner>, With<KindFilter<K>>>,
             mut commands: Commands<'_, '_>| {
                if let Ok(owner) = units.get(removed.entity) {
                    commands
                        .entity(removed.entity)
                        .try_insert(KindFilter::<K>::new(owner, None));
                }
            },
        );
    }

    /// Inserts the filter of `K` again as its entity's `C`, its owner or its team, is inserted.
    fn follow_insert<C: Component>(app: &mut App) {
        app.add_observer(
            |inserted: On<'_, '_, Insert, C>,
             units: Owners<'_, '_, K>,
             mut commands: Commands<'_, '_>| {
                if let Ok((owner, player, team, true)) = units.get(inserted.entity) {
                    commands
                        .entity(inserted.entity)
                        .try_insert(KindFilter::<K>::of(owner, player, team));
                }
            },
        );
    }

    /// Puts the filter of `K` on each entity that gains `C`, one of `K`'s types, unless it holds
    /// it already.
    fn observe<C: Replication>(app: &mut App) {
        assert_eq!(C::KIND, K::KIND, "{} is of its filter's kind", C::NAME);
        app.add_observer(
            |added: On<'_, '_, Add, C>,
             units: Owners<'_, '_, K>,
             mut commands: Commands<'_, '_>| {
                if let Ok((owner, player, team, false)) = units.get(added.entity) {
                    commands
                        .entity(added.entity)
                        .try_insert(KindFilter::<K>::of(owner, player, team));
                }
            },
        );
    }
}

impl<K: FilteredKind> VisibilityFilter for KindFilter<K> {
    type ClientComponent = PlayerLink;
    type Scope = K::Scope;

    fn is_visible(&self, _client: Entity, link: Option<&PlayerLink>) -> bool {
        link.is_some_and(|link| {
            let owns = Some(link.slot()) == self.owner;
            owns || (K::ALLIES && Some(link.team()) == self.team)
        })
    }
}

/// The filters of the kinds that not every client that sees an entity receives.
#[derive(Debug)]
pub(crate) struct KindFilters;

impl KindFilters {
    /// Registers the filter of every kind that not every client that sees an entity receives,
    /// and, from the lists of `set`, the observers that put a kind's filter on each
    /// entity that gains one of its types: no system adds a filter, so none can forget one.
    pub(crate) fn register(app: &mut App, set: CapabilitySet) {
        KindFilter::<PredictionKind>::register_kind(app);
        KindFilter::<PoolsKind>::register_kind(app);
        KindFilter::<CooldownsKind>::register_kind(app);
        KindFilter::<InventoryKind>::register_kind(app);
        KindFilter::<ProgressionKind>::register_kind(app);
        KindFilter::<ResourcesKind>::register_kind(app);
        KindFilter::<UnitStateOwnerKind>::register_kind(app);
        KindFilter::<UnitStateTeamKind>::register_kind(app);
        Release::state_types(set, &mut FilterObservers(app));
    }
}

/// Reads the capabilities' lists for `KindFilters::register`: each type of a kind with a filter
/// gets the observer that puts its kind's filter on it.
#[derive(Debug)]
struct FilterObservers<'a>(&'a mut App);

impl FilterObservers<'_> {
    fn filter<C: Replication>(&mut self) {
        match C::KIND {
            DataKind::Prediction => KindFilter::<PredictionKind>::observe::<C>(self.0),
            DataKind::Pools => KindFilter::<PoolsKind>::observe::<C>(self.0),
            DataKind::Cooldowns => KindFilter::<CooldownsKind>::observe::<C>(self.0),
            DataKind::Inventory => KindFilter::<InventoryKind>::observe::<C>(self.0),
            DataKind::Progression => KindFilter::<ProgressionKind>::observe::<C>(self.0),
            DataKind::Resources => KindFilter::<ResourcesKind>::observe::<C>(self.0),
            DataKind::UnitStateOwner => KindFilter::<UnitStateOwnerKind>::observe::<C>(self.0),
            DataKind::UnitStateTeam => KindFilter::<UnitStateTeamKind>::observe::<C>(self.0),
            DataKind::Unit
            | DataKind::Match
            | DataKind::ModeState
            | DataKind::PlayerModifiers
            | DataKind::Life
            | DataKind::Modifiers
            | DataKind::UnitStateAll
            | DataKind::Server => {}
        }
    }
}

impl StateTypes for FilterObservers<'_> {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.filter::<C>();
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.filter::<C>();
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
        self.filter::<C>();
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.filter::<C>();
    }

    fn server<C: Replication>(&mut self, _: Declared) {}

    fn derived<C: Component>(&mut self) {}

    fn resource<R: SimResource>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {
        self.component::<C>();
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::collections::BTreeSet;

    use bevy_ecs::world::World;
    use campfire_capabilities::SentOnChange;
    use campfire_sim::{Capability, OnAny, SimComponent};
    use serde::{Deserialize, Serialize};

    use super::*;

    /// A state type of `progression` that no scope holds.
    #[derive(Component, Debug, Serialize, Deserialize)]
    struct Stray;

    impl SimComponent for Stray {
        const NAME: &'static str = "test.stray";
        type Host = OnAny;

        fn check(&self, _: &World, _: Entity) -> bool {
            true
        }
    }

    impl Replication for Stray {
        const KIND: DataKind = DataKind::Progression;
        type Sending = SentOnChange;
    }

    /// A scope's types, by their `TypeId`s.
    trait TypeList {
        fn type_ids() -> Vec<TypeId>;
    }

    impl<C: Component> TypeList for SingleComponent<C> {
        fn type_ids() -> Vec<TypeId> {
            vec![TypeId::of::<C>()]
        }
    }

    /// `TypeList` for the tuples replicon takes as scopes, 2 to 10 types.
    macro_rules! type_list {
        ($($C:ident),+) => {
            impl<$($C: Component),+> TypeList for ($($C,)+) {
                fn type_ids() -> Vec<TypeId> {
                    vec![$(TypeId::of::<$C>()),+]
                }
            }
        };
    }
    type_list!(C1, C2);
    type_list!(C1, C2, C3);
    type_list!(C1, C2, C3, C4);
    type_list!(C1, C2, C3, C4, C5);
    type_list!(C1, C2, C3, C4, C5, C6);
    type_list!(C1, C2, C3, C4, C5, C6, C7);
    type_list!(C1, C2, C3, C4, C5, C6, C7, C8);
    type_list!(C1, C2, C3, C4, C5, C6, C7, C8, C9);
    type_list!(C1, C2, C3, C4, C5, C6, C7, C8, C9, C10);

    /// Every type of the capabilities' lists, with its kind.
    #[derive(Debug, Default)]
    struct Kinds(Vec<(DataKind, TypeId, &'static str)>);

    impl Kinds {
        fn of_every_capability() -> Kinds {
            let declared: Vec<Capability> = Capability::ALL
                .into_iter()
                .filter(|&capability| capability != Capability::Mode)
                .collect();
            let mut kinds = Kinds::default();
            Release::state_types(CapabilitySet::new(&declared).unwrap(), &mut kinds);
            kinds
        }

        fn push<C: Replication>(&mut self) {
            self.0.push((C::KIND, TypeId::of::<C>(), C::NAME));
        }

        /// The names of the listed types of `kind` whose ids `has` says, sorted.
        fn names(&self, kind: DataKind, has: impl Fn(TypeId) -> bool) -> Vec<&'static str> {
            let mut names: Vec<_> = self
                .0
                .iter()
                .filter(|&&(of, id, _)| of == kind && has(id))
                .map(|&(_, _, name)| name)
                .collect();
            names.sort_unstable();
            names
        }
    }

    impl StateTypes for Kinds {
        fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
            self.push::<C>();
        }

        fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
            self.push::<C>();
        }

        fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
            self.push::<C>();
        }

        fn predicted<
            C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug,
        >(
            &mut self,
            _: Declared,
        ) {
            self.push::<C>();
        }

        fn server<C: Replication>(&mut self, _: Declared) {
            self.push::<C>();
        }

        fn derived<C: Component>(&mut self) {}

        fn resource<R: SimResource>(&mut self) {}

        fn sim_component<C: Replication>(&mut self) {
            self.component::<C>();
        }
    }

    /// The listed types of `K`'s kind its scope lacks, and the types of its scope no list gives
    /// that kind, by the names of the first and the count of the second: both empty.
    fn scope_of<K: FilteredKind>(kinds: &Kinds) -> (Vec<&'static str>, usize)
    where
        K::Scope: TypeList,
    {
        let scope: BTreeSet<TypeId> = K::Scope::type_ids().into_iter().collect();
        let lacking = kinds.names(K::KIND, |id| !scope.contains(&id));
        let listed = kinds.names(K::KIND, |id| scope.contains(&id));
        (lacking, scope.len() - listed.len())
    }

    #[test]
    fn each_filtered_kinds_scope_holds_exactly_the_types_the_lists_give_it() {
        // A type a list gives a kind its scope lacks would go to every client, and one its scope
        // holds of another kind would go to the kind's clients alone: each fails here by its
        // name.
        let kinds = Kinds::of_every_capability();
        assert_eq!(scope_of::<PredictionKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<PoolsKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<CooldownsKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<InventoryKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<ProgressionKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<ResourcesKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<UnitStateOwnerKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<UnitStateTeamKind>(&kinds), (vec![], 0));
        // The check itself: `Stray`, of the kind, which the scope lacks, fails by its name, and
        // `Points`, of the scope, which this list does not give, counts once.
        let mut stray = Kinds::default();
        stray.push::<Experience>();
        stray.push::<Stray>();
        assert_eq!(scope_of::<ProgressionKind>(&stray), (vec!["test.stray"], 1));
    }

    #[test]
    fn a_kinds_filter_follows_its_entitys_owner_and_team() {
        // `Stray`, of `progression`: its filter holds the owner and team the entity has as it
        // gains it, each it changes to, and none of one once it loses it; a despawn changes
        // nothing.
        let mut app = App::new();
        KindFilter::<ProgressionKind>::follow(&mut app);
        KindFilter::<ProgressionKind>::observe::<Stray>(&mut app);
        let world = app.world_mut();
        let held = |world: &World, entity| {
            world
                .get::<KindFilter<ProgressionKind>>(entity)
                .map(|filter| (filter.owner, filter.team))
        };
        let (slot, team) = (PlayerSlot::new, Team::new);
        let hero = world.spawn((Owner::new(slot(1)), team(0), Stray)).id();
        world.flush();
        assert_eq!(held(world, hero), Some((Some(slot(1)), Some(team(0)))));
        world.entity_mut(hero).insert(Owner::new(slot(2)));
        world.flush();
        assert_eq!(held(world, hero), Some((Some(slot(2)), Some(team(0)))));
        world.entity_mut(hero).insert(team(1));
        world.flush();
        assert_eq!(held(world, hero), Some((Some(slot(2)), Some(team(1)))));
        world.entity_mut(hero).remove::<Owner>();
        world.flush();
        assert_eq!(held(world, hero), Some((None, Some(team(1)))));
        world.entity_mut(hero).insert(Owner::new(slot(2)));
        world.entity_mut(hero).remove::<Team>();
        world.flush();
        assert_eq!(held(world, hero), Some((Some(slot(2)), None)));
        // A unit no player owns, as a creep; and one that holds no type of the kind.
        let creep = world.spawn((team(1), Stray)).id();
        let tower = world.spawn((Owner::new(slot(1)), team(0))).id();
        world.flush();
        assert_eq!(held(world, creep), Some((None, Some(team(1)))));
        assert_eq!(held(world, tower), None);
        world.entity_mut(hero).insert(Owner::new(slot(1)));
        world.flush();
        world.despawn(hero);
        world.flush();
        assert!(world.get_entity(hero).is_err());
    }

    #[test]
    fn only_the_owners_seated_link_and_for_a_team_kind_its_teams_hold_a_filtered_kind() {
        let link = |slot, team| PlayerLink::new(PlayerSlot::new(slot), Team::new(team));
        let client = Entity::PLACEHOLDER;
        let (player, team) = (Owner::new(PlayerSlot::new(1)), Team::new(0));
        let prediction = KindFilter::<PredictionKind>::new(Some(&player), Some(&team));
        let team_state = KindFilter::<UnitStateTeamKind>::new(Some(&player), Some(&team));
        // The owner holds both; an ally the team's kind alone; an enemy neither.
        for (link, holds) in [
            (link(1, 0), (true, true)),
            (link(0, 0), (false, true)),
            (link(2, 1), (false, false)),
        ] {
            let held = (
                prediction.is_visible(client, Some(&link)),
                team_state.is_visible(client, Some(&link)),
            );
            assert_eq!(held, holds, "{link:?}");
        }
        // A link with no seat, and a unit no player owns, as a creep: no link holds an owned
        // kind, and the creep's team holds its team's kind.
        assert!(!prediction.is_visible(client, None) && !team_state.is_visible(client, None));
        let creep = KindFilter::<PredictionKind>::new(None, Some(&team));
        assert!(!creep.is_visible(client, Some(&link(1, 0))));
        let creep = KindFilter::<UnitStateTeamKind>::new(None, Some(&team));
        assert!(creep.is_visible(client, Some(&link(1, 0))));
        assert!(!creep.is_visible(client, Some(&link(2, 1))));
    }
}
