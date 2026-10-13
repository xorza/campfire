use std::fmt::Debug;
use std::marker::PhantomData;

use bevy_app::App;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query, Res};
use bevy_ecs::world::DeferredWorld;
use bevy_replicon::prelude::{AppVisibilityExt, FilterScope, SingleComponent, VisibilityFilter};
use campfire_capabilities::{
    Builder, CapabilitySet, DataKind, Experience, Gatherer, Inventory, Life, ModeState,
    ModifierClocks, Modifiers, OnDeath, OnPath, Owner, PathWalker, Player, PlayerModifiers,
    PlayerResources, PlayerSupply, Points, Pools, Progress, Recipients, Release, Replication,
    Respawn, Route, Site, SlotTimes, SpawnPoint, StateTypes, SyncAll, SyncOwner, SyncTeam, Team,
    TrainQueue, UnitState, UseGroup,
};
use campfire_common::PlayerSlot;
use campfire_sim::SimResource;

use crate::sim_server::match_visibility::MatchVisibility;
use crate::sim_server::scope_union::ScopeUnion;
use crate::sim_server::viewer::Viewer;

/// A kind of state whose audience a mode may set, or that the owner alone receives, and the
/// types of the kind, which its filter hides from every client outside the kind's audience in
/// the match. A test checks each scope holds exactly the types the capabilities' lists give the
/// kind.
pub(crate) trait FilteredKind: Send + Sync + Debug + 'static {
    const KIND: DataKind;
    type Scope: FilterScope;
}

/// Declares a filtered kind of each name, of its `DataKind` and its scope; the registration of
/// every filter; the dispatch of a state type to its kind's filter, whose match is exhaustive, so
/// a new `DataKind` is filtered or named as unfiltered; and the test's list of the kinds.
macro_rules! filtered_kinds {
    ($($(#[$doc:meta])* $name:ident: $kind:ident = $scope:ty;)+) => {
        $(
            $(#[$doc])*
            #[derive(Debug)]
            pub(crate) struct $name;

            impl FilteredKind for $name {
                const KIND: DataKind = DataKind::$kind;
                type Scope = $scope;
            }
        )+

        impl DataFilters {
            /// Registers the filter of each filtered kind.
            fn register_kinds(app: &mut App) {
                $(DataFilter::<$name>::register_kind(app);)+
            }
        }

        impl FilterRequirements<'_> {
            /// Makes `C` require the filter of its kind, when its kind has one.
            fn require<C: Replication>(&mut self) {
                match C::KIND {
                    $(DataKind::$kind => DataFilter::<$name>::required_by::<C>(self.0),)+
                    // Every client that holds the entity receives these, and vision decides who
                    // holds it; the server's kind is never sent.
                    DataKind::Unit | DataKind::Match | DataKind::Server => {
                        debug_assert_eq!(C::KIND.settable(), Recipients::NONE);
                    }
                }
            }
        }

        /// Each filtered kind's name, and the listed types of its kind its scope lacks with the
        /// count of its scope's types no list gives it, as `tests::scope_of` finds them.
        #[cfg(test)]
        fn scopes(kinds: &tests::Kinds) -> Vec<(&'static str, (Vec<&'static str>, usize))> {
            vec![$((stringify!($name), tests::scope_of::<$name>(kinds))),+]
        }
    };
}

filtered_kinds! {
    /// A unit's life pool.
    LifeKind: Life = SingleComponent<Life>;
    /// A unit's pools other than its life.
    PoolsKind: Pools = SingleComponent<Pools>;
    /// A unit's modifiers.
    ModifiersKind: Modifiers = SingleComponent<Modifiers>;
    /// A unit's slots' ready ticks, charges and toggles.
    CooldownsKind: Cooldowns = SingleComponent<SlotTimes>;
    /// A unit's experience and points.
    ProgressionKind: Progression = (Experience, Points);
    /// A unit's inventory.
    InventoryKind: Inventory = (Inventory, UseGroup);
    /// The mode's script state fields of `sync = "all"`.
    ModeStateKind: ModeState = SingleComponent<ModeState<SyncAll>>;
    /// A player's resources.
    ResourcesKind: Resources = SingleComponent<PlayerResources>;
    /// A player's supply.
    SupplyKind: Supply = SingleComponent<PlayerSupply>;
    /// A producer's train queue.
    QueuesKind: Queues = SingleComponent<TrainQueue>;
    /// The modifiers a player holds for its units.
    PlayerModifiersKind: PlayerModifiers = SingleComponent<PlayerModifiers>;
    /// What only the owner's prediction reads.
    PredictionKind: Prediction = ScopeUnion<
        (SpawnPoint, Route, Progress, ModifierClocks, Respawn),
        (OnDeath, Builder, Gatherer, Site, PathWalker, OnPath),
    >;
    /// A unit's script state fields of `sync = "owner"`.
    UnitStateOwnerKind: UnitStateOwner = SingleComponent<UnitState<SyncOwner>>;
    /// A unit's script state fields of `sync = "team"`.
    UnitStateTeamKind: UnitStateTeam = SingleComponent<UnitState<SyncTeam>>;
    /// A unit's script state fields of `sync = "all"`.
    UnitStateAllKind: UnitStateAll = SingleComponent<UnitState<SyncAll>>;
}

/// Each entity's owner, a unit's or a player's entity's slot, and its team, of the entities that
/// hold the filter of `K`.
type Owners<'w, 's, K> = Query<
    'w,
    's,
    (
        Option<&'static Owner>,
        Option<&'static Player>,
        Option<&'static Team>,
    ),
    With<DataFilter<K>>,
>;

/// On each entity that holds a type of kind `K`: the slot that owns it, if one does, a player's
/// entity its own slot, its team, if it has one, and the recipients beside the owner that receive
/// the kind in the match. Replicon reads it to send the kind's types to them alone. Immutable, as
/// replicon requires: a change of owner or team inserts a new one. Each type of the kind requires
/// it, so it comes in the move that gives an entity the type, closed, to no recipient, until the
/// hook of its addition puts its value in place.
#[derive(Component, Debug)]
#[component(immutable)]
pub(crate) struct DataFilter<K: FilteredKind> {
    owner: Option<PlayerSlot>,
    team: Option<Team>,
    audience: Recipients,
    kind: PhantomData<K>,
}

impl<K: FilteredKind> Default for DataFilter<K> {
    fn default() -> DataFilter<K> {
        DataFilter {
            owner: None,
            team: None,
            audience: Recipients::NONE,
            kind: PhantomData,
        }
    }
}

impl<K: FilteredKind> DataFilter<K> {
    const fn new(
        owner: Option<&Owner>,
        team: Option<&Team>,
        visibility: &MatchVisibility,
    ) -> DataFilter<K> {
        DataFilter {
            owner: match owner {
                Some(owner) => Some(owner.slot()),
                None => None,
            },
            team: team.copied(),
            audience: visibility.0.audience(K::KIND),
            kind: PhantomData,
        }
    }

    /// The filter of an entity of `owner` or, a player's entity, of `player`, and of `team`.
    fn of(
        owner: Option<&Owner>,
        player: Option<&Player>,
        team: Option<&Team>,
        visibility: &MatchVisibility,
    ) -> DataFilter<K> {
        let owner = owner
            .copied()
            .or_else(|| player.map(|player| Owner::new(player.slot())));
        DataFilter::new(owner.as_ref(), team, visibility)
    }

    /// The filter of `K`, as replicon reads it and as `keep` keeps it.
    fn register_kind(app: &mut App) {
        app.add_visibility_filter::<DataFilter<K>>();
        DataFilter::<K>::keep(app);
    }

    /// The hook that gives a new filter of `K` its value, and the observers that follow its
    /// entity's owner and team.
    fn keep(app: &mut App) {
        app.world_mut()
            .register_component_hooks::<DataFilter<K>>()
            .on_add(DataFilter::<K>::fill);
        DataFilter::<K>::follow(app);
    }

    /// Makes `C`, one of `K`'s types, require the filter of `K`.
    fn required_by<C: Replication>(app: &mut App) {
        assert_eq!(C::KIND, K::KIND, "{} is of its filter's kind", C::NAME);
        app.world_mut()
            .register_required_components::<C, DataFilter<K>>();
    }

    /// The hook that gives a filter its entity's owner and team as it comes, closed, with a type of
    /// its kind: a filter is immutable, so a hook cannot write it, and its insert replaces it in
    /// place, with no move between archetypes, before replicon next reads it.
    fn fill(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = world.entity(context.entity);
        let filter = DataFilter::<K>::of(
            entity.get::<Owner>(),
            entity.get::<Player>(),
            entity.get::<Team>(),
            world.resource::<MatchVisibility>(),
        );
        world.commands().entity(context.entity).try_insert(filter);
    }

    /// Inserts the filter of `K` again as its entity's owner or team changes, or as the entity
    /// loses either, on an entity that holds a filter, as it spawns too, beside the hook. Each insert of a filter keeps to an entity
    /// that still lives: it applies at the next flush, after a despawn queued before it, and a
    /// despawn removes the owner and team too.
    fn follow(app: &mut App) {
        DataFilter::<K>::follow_insert::<Owner>(app);
        DataFilter::<K>::follow_insert::<Team>(app);
        app.add_observer(
            |removed: On<'_, '_, Remove, Owner>,
             units: Query<'_, '_, Option<&Team>, With<DataFilter<K>>>,
             visibility: Res<'_, MatchVisibility>,
             mut commands: Commands<'_, '_>| {
                if let Ok(team) = units.get(removed.entity) {
                    commands
                        .entity(removed.entity)
                        .try_insert(DataFilter::<K>::new(None, team, &visibility));
                }
            },
        );
        app.add_observer(
            |removed: On<'_, '_, Remove, Team>,
             units: Query<'_, '_, Option<&Owner>, With<DataFilter<K>>>,
             visibility: Res<'_, MatchVisibility>,
             mut commands: Commands<'_, '_>| {
                if let Ok(owner) = units.get(removed.entity) {
                    commands
                        .entity(removed.entity)
                        .try_insert(DataFilter::<K>::new(owner, None, &visibility));
                }
            },
        );
    }

    /// Inserts the filter of `K` again as its entity's `C`, its owner or its team, is inserted.
    fn follow_insert<C: Component>(app: &mut App) {
        app.add_observer(
            |inserted: On<'_, '_, Insert, C>,
             units: Owners<'_, '_, K>,
             visibility: Res<'_, MatchVisibility>,
             mut commands: Commands<'_, '_>| {
                if let Ok((owner, player, team)) = units.get(inserted.entity) {
                    commands
                        .entity(inserted.entity)
                        .try_insert(DataFilter::<K>::of(owner, player, team, &visibility));
                }
            },
        );
    }
}

impl<K: FilteredKind> VisibilityFilter for DataFilter<K> {
    type ClientComponent = Viewer;
    type Scope = K::Scope;

    fn is_visible(&self, _client: Entity, viewer: Option<&Viewer>) -> bool {
        viewer.is_some_and(|viewer| {
            let recipient = viewer.recipient(self.owner, self.team);
            self.audience.contains(recipient)
        })
    }
}

/// The filters of the kinds whose audience a mode may set, or that only their owner receives.
#[derive(Debug)]
pub(crate) struct DataFilters;

impl DataFilters {
    /// Registers the filter of every kind whose audience a mode may set or that only its owner
    /// receives, and makes each type the lists of `set` give such a kind require its filter: no
    /// system adds a filter, so none can forget one, and a filter comes with the type it hides.
    pub(crate) fn register(app: &mut App, set: CapabilitySet) {
        DataFilters::register_kinds(app);
        Release::state_types(set, &mut FilterRequirements(app));
    }
}

/// Reads the capabilities' lists for `DataFilters::register`: each type of a kind with a filter
/// requires its data filter.
#[derive(Debug)]
struct FilterRequirements<'a>(&'a mut App);

impl StateTypes for FilterRequirements<'_> {
    fn component<C: Replication>(&mut self) {
        self.require::<C>();
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.require::<C>();
    }

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {
        self.component::<C>();
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::collections::BTreeSet;

    use bevy_ecs::world::World;
    use campfire_capabilities::{ModeVisibility, SentOnChange, TeamSet, Visibility};
    use campfire_codec::Toml;
    use campfire_sim::{Capability, OnAny, SimComponent};
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::sim_server::viewer::Watching;

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
    pub(super) trait TypeList {
        fn type_ids() -> Vec<TypeId>;
    }

    impl<C: Component> TypeList for SingleComponent<C> {
        fn type_ids() -> Vec<TypeId> {
            vec![TypeId::of::<C>()]
        }
    }

    impl<A: TypeList, B: TypeList> TypeList for ScopeUnion<A, B> {
        fn type_ids() -> Vec<TypeId> {
            let mut ids = A::type_ids();
            ids.extend(B::type_ids());
            ids
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
    pub(super) struct Kinds(Vec<(DataKind, TypeId, &'static str)>);

    impl Kinds {
        fn of_every_capability() -> Kinds {
            let declared: Vec<Capability> = Capability::ALL
                .into_iter()
                .filter(|&capability| capability != Capability::Mode)
                .collect();
            let mut kinds = Kinds::default();
            Release::state_types(Release::capability_set(&declared).unwrap(), &mut kinds);
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
        fn component<C: Replication>(&mut self) {
            self.push::<C>();
        }

        fn derived<C: Component>(&mut self) {}

        fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
            self.push::<C>();
        }

        fn resource<R: SimResource + Default>(&mut self) {}

        fn sim_component<C: Replication>(&mut self) {
            self.component::<C>();
        }
    }

    /// The listed types of `K`'s kind its scope lacks, and the types of its scope no list gives
    /// that kind, by the names of the first and the count of the second: both empty.
    pub(super) fn scope_of<K: FilteredKind>(kinds: &Kinds) -> (Vec<&'static str>, usize)
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
        let flawed: Vec<_> = scopes(&kinds)
            .into_iter()
            .filter(|(_, scope)| *scope != (vec![], 0))
            .collect();
        assert_eq!(flawed, []);
        // Every kind is filtered but the three the dispatch names as unfiltered.
        assert_eq!(scopes(&kinds).len(), DataKind::ALL.len() - 3);
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
        app.insert_resource(MatchVisibility(Visibility::DEFAULT));
        DataFilter::<ProgressionKind>::keep(&mut app);
        DataFilter::<ProgressionKind>::required_by::<Stray>(&mut app);
        let world = app.world_mut();
        let held = |world: &World, entity| {
            world
                .get::<DataFilter<ProgressionKind>>(entity)
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

    /// Whether each of `viewers` holds what `filter` filters, as replicon asks it for `client`.
    fn held<K: FilteredKind>(
        filter: &DataFilter<K>,
        viewers: &[Option<Viewer>],
        client: Entity,
    ) -> Vec<bool> {
        viewers
            .iter()
            .map(|viewer| filter.is_visible(client, viewer.as_ref()))
            .collect()
    }

    #[test]
    fn only_the_kinds_audience_in_the_match_holds_a_filtered_kind() {
        let player = |slot, team| {
            Some(Viewer::Player {
                slot: PlayerSlot::new(slot),
                team: Team::new(team),
                out: false,
            })
        };
        let spectator = Some(Viewer::Spectator(Watching::Teams(TeamSet::ALL)));
        let client = Entity::PLACEHOLDER;
        let (owner, team) = (Owner::new(PlayerSlot::new(1)), Team::new(0));
        // The owner of team 0's unit, an ally, an enemy, a spectator, and a link with no seat.
        let viewers = [player(1, 0), player(0, 0), player(2, 1), spectator, None];
        // The defaults: prediction to the owner alone; a team field to its team; life to all.
        let defaults = MatchVisibility(Visibility::DEFAULT);
        let prediction = DataFilter::<PredictionKind>::new(Some(&owner), Some(&team), &defaults);
        let team_state = DataFilter::<UnitStateTeamKind>::new(Some(&owner), Some(&team), &defaults);
        let life = DataFilter::<LifeKind>::new(Some(&owner), Some(&team), &defaults);
        assert_eq!(
            held(&prediction, &viewers, client),
            [true, false, false, false, false]
        );
        assert_eq!(
            held(&team_state, &viewers, client),
            [true, true, false, false, false]
        );
        assert_eq!(
            held(&life, &viewers, client),
            [true, true, true, true, false]
        );

        // A mode that keeps life from enemies, and shows the inventory to spectators: the
        // owner and its allies hold life still.
        let mode: ModeVisibility = Toml::parse(
            r#"
life = ["spectators"]
inventory = ["spectators"]
"#,
        )
        .unwrap();
        let set = MatchVisibility(*mode.settings());
        let life = DataFilter::<LifeKind>::new(Some(&owner), Some(&team), &set);
        let inventory = DataFilter::<InventoryKind>::new(Some(&owner), Some(&team), &set);
        assert_eq!(
            held(&life, &viewers, client),
            [true, true, false, true, false]
        );
        assert_eq!(
            held(&inventory, &viewers, client),
            [true, false, false, true, false]
        );
        // A unit no player owns, as a creep: its team's kind to its team, and nothing of the
        // owner's to anyone.
        let creep = DataFilter::<PredictionKind>::new(None, Some(&team), &defaults);
        assert_eq!(
            held(&creep, &viewers, client),
            [false, false, false, false, false]
        );
        let creep = DataFilter::<UnitStateTeamKind>::new(None, Some(&team), &defaults);
        assert_eq!(
            held(&creep, &viewers, client),
            [true, true, false, false, false]
        );
    }
}
