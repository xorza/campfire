use std::fmt::{self, Debug};
use std::marker::PhantomData;

use bevy_app::App;
use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query, Res};
use bevy_ecs::world::{DeferredWorld, World};
use bevy_replicon::prelude::{AppVisibilityExt, FilterScope, SingleComponent, VisibilityFilter};
use bevy_replicon::shared::replication::registry::ReplicationRegistry;
use bevy_replicon::shared::replication::registry::receive_fns::MutWrite;
use bevy_replicon::shared::replication::visibility::VisibilityScope;
use campfire_capabilities::Release;
use campfire_common::Seat;
use campfire_declare::CapabilitySet;
use campfire_players::Player;
use campfire_sim::SimResource;
use campfire_state_types::{
    DataKind, Declared, Recipients, Replication, Sending, SendingWays, StateTypes,
};
use campfire_units::Owner;
use campfire_values::Team;

use crate::sim_server::match_visibility::MatchVisibility;
use crate::sim_server::viewer::Viewer;

/// A kind of state whose audience a mode may set, or that the owner alone receives, whose filter
/// hides the kind's types from every client outside its audience in the match.
pub(crate) trait FilteredKind: Send + Sync + Debug + 'static {
    const KIND: DataKind;
}

/// Declares a filtered kind of each name, of its `DataKind`; the registration of every filter;
/// the dispatch of a state type to its kind's filter, whose match is exhaustive, so a new
/// `DataKind` is filtered or named as unfiltered; and the test's list of the kinds.
macro_rules! filtered_kinds {
    ($($(#[$doc:meta])* $name:ident: $kind:ident;)+) => {
        $(
            $(#[$doc])*
            #[derive(Debug)]
            pub(crate) struct $name;

            impl FilteredKind for $name {
                const KIND: DataKind = DataKind::$kind;
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

        /// The filtered kinds, in the order of their declaration.
        #[cfg(test)]
        const FILTERED: &[DataKind] = &[$(DataKind::$kind),+];
    };
}

filtered_kinds! {
    /// A unit's life pool.
    LifeKind: Life;
    /// A unit's pools other than its life.
    PoolsKind: Pools;
    /// A unit's modifiers.
    ModifiersKind: Modifiers;
    /// A unit's seats' ready ticks, charges and toggles.
    CooldownsKind: Cooldowns;
    /// A unit's experience and points.
    ProgressionKind: Progression;
    /// A unit's inventory.
    InventoryKind: Inventory;
    /// The mode's script state fields of `sync = "all"`.
    ModeStateKind: ModeState;
    /// A player's resources.
    ResourcesKind: Resources;
    /// A player's supply.
    SupplyKind: Supply;
    /// A producer's train queue.
    QueuesKind: Queues;
    /// The modifiers a player holds for its units.
    PlayerModifiersKind: PlayerModifiers;
    /// What only the owner's prediction reads.
    PredictionKind: Prediction;
    /// A unit's script state fields of `sync = "owner"`.
    UnitStateOwnerKind: UnitStateOwner;
    /// A unit's script state fields of `sync = "team"`.
    UnitStateTeamKind: UnitStateTeam;
    /// A unit's script state fields of `sync = "all"`.
    UnitStateAllKind: UnitStateAll;
    /// A player's script state fields of `sync = "owner"`.
    PlayerStateOwnerKind: PlayerStateOwner;
    /// A player's script state fields of `sync = "team"`.
    PlayerStateTeamKind: PlayerStateTeam;
    /// A player's script state fields of `sync = "all"`.
    PlayerStateAllKind: PlayerStateAll;
}

/// The types of `K`'s kind in the state lists of every capability of the release, as one scope,
/// which the filter of `K` hides together. The lists give it, so no type of the kind goes to a
/// client outside its audience.
#[derive(Debug)]
pub(crate) struct KindScope<K>(PhantomData<K>);

impl<K: FilteredKind> FilterScope for KindScope<K> {
    fn visibility_scope(world: &mut World, registry: &mut ReplicationRegistry) -> VisibilityScope {
        let mut scope = ScopeTypes {
            kind: K::KIND,
            world,
            registry,
            scope: None,
        };
        Release::state_types(Release::capabilities(), &mut scope);
        scope
            .scope
            .expect("the release's lists give each filtered kind a type")
    }
}

/// Reads the capabilities' lists for `KindScope`: each sent type of `kind` joins `scope`.
struct ScopeTypes<'a> {
    kind: DataKind,
    world: &'a mut World,
    registry: &'a mut ReplicationRegistry,
    /// The components of the types read so far; none before the first.
    scope: Option<VisibilityScope>,
}

impl Debug for ScopeTypes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeTypes")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl ScopeTypes<'_> {
    /// Adds `C` to the scope when it is of its kind.
    fn add<C: Replication + Component<Mutability: MutWrite<C>>>(&mut self) {
        if C::KIND != self.kind {
            return;
        }
        let one = SingleComponent::<C>::visibility_scope(self.world, self.registry);
        match (&mut self.scope, one) {
            (None, one) => self.scope = Some(one),
            (Some(VisibilityScope::Components(held)), VisibilityScope::Components(mask)) => {
                *held |= &mask;
            }
            _ => unreachable!("a component's scope holds components"),
        }
    }
}

impl StateTypes for ScopeTypes<'_> {
    fn component<C: Replication>(&mut self) {
        <C::Sending as Sending<C>>::visit(self);
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.add::<C>();
    }

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {
        self.component::<C>();
    }
}

impl SendingWays for ScopeTypes<'_> {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.add::<C>();
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.add::<C>();
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
        self.add::<C>();
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.add::<C>();
    }

    // No client receives it, so no filter hides it.
    fn server<C: Replication>(&mut self, _: Declared) {}
}

/// Each entity's owner, a unit's or a player's entity's seat, and its team, of the entities that
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

/// On each entity that holds a type of kind `K`: the seat that owns it, if one does, a player's
/// entity its own seat, its team, if it has one, and the recipients beside the owner that receive
/// the kind in the match. Replicon reads it to send the kind's types to them alone. Immutable, as
/// replicon requires: a change of owner or team inserts a new one. Each type of the kind requires
/// it, so it comes in the move that gives an entity the type, closed, to no recipient, until the
/// hook of its addition puts its value in place.
#[derive(Component, Debug)]
#[component(immutable)]
pub(crate) struct DataFilter<K: FilteredKind> {
    owner: Option<Seat>,
    team: Option<Team>,
    audience: Recipients,
    kind: PhantomData<K>,
}

impl<K: FilteredKind> Default for DataFilter<K> {
    fn default() -> Self {
        Self {
            owner: None,
            team: None,
            audience: Recipients::NONE,
            kind: PhantomData,
        }
    }
}

impl<K: FilteredKind> DataFilter<K> {
    const fn new(owner: Option<&Owner>, team: Option<&Team>, visibility: &MatchVisibility) -> Self {
        Self {
            owner: match owner {
                Some(owner) => Some(owner.seat()),
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
    ) -> Self {
        let owner = owner
            .copied()
            .or_else(|| player.map(|player| Owner::new(player.seat())));
        Self::new(owner.as_ref(), team, visibility)
    }

    /// The filter of `K`, as replicon reads it and as `keep` keeps it.
    fn register_kind(app: &mut App) {
        app.add_visibility_filter::<Self>();
        Self::keep(app);
    }

    /// The hook that gives a new filter of `K` its value, and the observers that follow its
    /// entity's owner and team.
    fn keep(app: &mut App) {
        app.world_mut()
            .register_component_hooks::<Self>()
            .on_add(Self::fill);
        Self::follow(app);
    }

    /// Makes `C`, one of `K`'s types, require the filter of `K`.
    fn required_by<C: Replication>(app: &mut App) {
        assert_eq!(C::KIND, K::KIND, "{} is of its filter's kind", C::NAME);
        app.world_mut().register_required_components::<C, Self>();
    }

    /// The hook that gives a filter its entity's owner and team as it comes, closed, with a type of
    /// its kind: a filter is immutable, so a hook cannot write it, and its insert replaces it in
    /// place, with no move between archetypes, before replicon next reads it.
    fn fill(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = world.entity(context.entity);
        let filter = Self::of(
            entity.get::<Owner>(),
            entity.get::<Player>(),
            entity.get::<Team>(),
            world.resource::<MatchVisibility>(),
        );
        world.commands().entity(context.entity).try_insert(filter);
    }

    /// Inserts the filter of `K` again as its entity's owner or team changes, or as the entity
    /// loses either, on an entity that holds a filter, as it spawns too, beside the hook. Each
    /// insert of a filter keeps to an entity that still lives: it applies at the next flush,
    /// after a despawn queued before it, and a despawn removes the owner and team too.
    fn follow(app: &mut App) {
        Self::follow_insert::<Owner>(app);
        Self::follow_insert::<Team>(app);
        // A remove's observer runs while its entity holds the component still.
        app.add_observer(
            |removed: On<'_, '_, Remove, Owner>,
             units: Owners<'_, '_, K>,
             visibility: Res<'_, MatchVisibility>,
             mut commands: Commands<'_, '_>| {
                if let Ok((_, player, team)) = units.get(removed.entity) {
                    commands.entity(removed.entity).try_insert(Self::of(
                        None,
                        player,
                        team,
                        &visibility,
                    ));
                }
            },
        );
        app.add_observer(
            |removed: On<'_, '_, Remove, Team>,
             units: Owners<'_, '_, K>,
             visibility: Res<'_, MatchVisibility>,
             mut commands: Commands<'_, '_>| {
                if let Ok((owner, player, _)) = units.get(removed.entity) {
                    commands.entity(removed.entity).try_insert(Self::of(
                        owner,
                        player,
                        None,
                        &visibility,
                    ));
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
                    commands.entity(inserted.entity).try_insert(Self::of(
                        owner,
                        player,
                        team,
                        &visibility,
                    ));
                }
            },
        );
    }
}

impl<K: FilteredKind> VisibilityFilter for DataFilter<K> {
    type ClientComponent = Viewer;
    type Scope = KindScope<K>;

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
        Self::register_kinds(app);
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
    use bevy_ecs::world::Mut;
    use campfire_codec::Toml;
    use campfire_progression::{Experience, Points};
    use campfire_sim::{OnAny, SimComponent};
    use campfire_state_types::SentOnChange;
    use campfire_units::{SyncAll, UnitState};
    use campfire_values::TeamSet;
    use campfire_visibility::{ModeVisibility, Visibility};
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

    /// The components of `S` as replicon registers them in `app`, as the text of their mask,
    /// which replicon gives no other way to compare.
    fn mask<S: FilterScope>(app: &mut App) -> String {
        let world = app.world_mut();
        world.init_resource::<ReplicationRegistry>();
        world.resource_scope(|world, mut registry: Mut<'_, ReplicationRegistry>| {
            let VisibilityScope::Components(mask) = S::visibility_scope(world, &mut registry)
            else {
                unreachable!("a components scope");
            };
            format!("{mask:?}")
        })
    }

    #[test]
    fn each_filtered_kinds_scope_holds_the_types_the_lists_give_it() {
        // Progression's kind holds its experience and its points; that of a unit's script state
        // fields of `sync = "all"`, their one component alone.
        let mut app = App::new();
        let progression = mask::<KindScope<ProgressionKind>>(&mut app);
        assert_eq!(progression, mask::<(Experience, Points)>(&mut app));
        let all = mask::<KindScope<UnitStateAllKind>>(&mut app);
        assert_ne!(all, progression);
        assert_eq!(all, mask::<SingleComponent<UnitState<SyncAll>>>(&mut app));
        // Every kind is filtered once but the three the dispatch names as unfiltered.
        for kind in DataKind::ALL {
            let filters = FILTERED
                .iter()
                .filter(|&&filtered| filtered == kind)
                .count();
            let unfiltered = matches!(kind, DataKind::Unit | DataKind::Match | DataKind::Server);
            assert_eq!(filters, usize::from(!unfiltered), "{kind:?}");
        }
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
        let (seat, team) = (Seat::new, Team::new);
        let hero = world.spawn((Owner::new(seat(1)), team(0), Stray)).id();
        world.flush();
        assert_eq!(held(world, hero), Some((Some(seat(1)), Some(team(0)))));
        world.entity_mut(hero).insert(Owner::new(seat(2)));
        world.flush();
        assert_eq!(held(world, hero), Some((Some(seat(2)), Some(team(0)))));
        world.entity_mut(hero).insert(team(1));
        world.flush();
        assert_eq!(held(world, hero), Some((Some(seat(2)), Some(team(1)))));
        world.entity_mut(hero).remove::<Owner>();
        world.flush();
        assert_eq!(held(world, hero), Some((None, Some(team(1)))));
        world.entity_mut(hero).insert(Owner::new(seat(2)));
        world.entity_mut(hero).remove::<Team>();
        world.flush();
        assert_eq!(held(world, hero), Some((Some(seat(2)), None)));
        // A unit no player owns, as a creep; and one that holds no type of the kind.
        let creep = world.spawn((team(1), Stray)).id();
        let tower = world.spawn((Owner::new(seat(1)), team(0))).id();
        world.flush();
        assert_eq!(held(world, creep), Some((None, Some(team(1)))));
        assert_eq!(held(world, tower), None);
        // A player's own entity: its seat is its owner, through each change of its team.
        let own = world.spawn((Player::new(seat(3)), team(0), Stray)).id();
        world.flush();
        assert_eq!(held(world, own), Some((Some(seat(3)), Some(team(0)))));
        world.entity_mut(own).remove::<Team>();
        world.flush();
        assert_eq!(held(world, own), Some((Some(seat(3)), None)));
        world.entity_mut(own).insert((Owner::new(seat(4)), team(1)));
        world.entity_mut(own).remove::<Owner>();
        world.flush();
        assert_eq!(held(world, own), Some((Some(seat(3)), Some(team(1)))));
        world.entity_mut(hero).insert(Owner::new(seat(1)));
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
        let player = |seat, team| {
            Some(Viewer::Player {
                seat: Seat::new(seat),
                team: Team::new(team),
                out: false,
            })
        };
        let spectator = Some(Viewer::Spectator(Watching::Teams(TeamSet::ALL)));
        let client = Entity::PLACEHOLDER;
        let (owner, team) = (Owner::new(Seat::new(1)), Team::new(0));
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
