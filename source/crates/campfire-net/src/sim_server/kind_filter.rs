use std::fmt::Debug;
use std::marker::PhantomData;

use bevy_app::App;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Add, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Has, With};
use bevy_ecs::system::{Commands, Query};
use bevy_replicon::prelude::{AppVisibilityExt, FilterScope, SingleComponent, VisibilityFilter};
use campfire_capabilities::{
    CapabilitySet, DataKind, Experience, Inventory, Kinded, ModifierClocks, Owner, Points,
    Progress, Respawn, Route, SpawnPoint, StateTypes,
};
use campfire_common::PlayerSlot;
use campfire_sim::SimResource;

use crate::sim_server::player_link::PlayerLink;

/// A kind of state its owner's client alone receives, and the types of the kind, which its
/// filter hides from every other client. A test checks each scope holds exactly the types the
/// capabilities' lists give the kind.
pub(crate) trait OwnedKind: Send + Sync + Debug + 'static {
    const KIND: DataKind;
    type Scope: FilterScope;
}

/// What only the owner's prediction reads.
#[derive(Debug)]
pub(crate) struct PredictionKind;

impl OwnedKind for PredictionKind {
    const KIND: DataKind = DataKind::Prediction;
    type Scope = (SpawnPoint, Route, Progress, ModifierClocks, Respawn);
}

/// A unit's inventory.
#[derive(Debug)]
pub(crate) struct InventoryKind;

impl OwnedKind for InventoryKind {
    const KIND: DataKind = DataKind::Inventory;
    type Scope = SingleComponent<Inventory>;
}

/// A unit's experience and points.
#[derive(Debug)]
pub(crate) struct ProgressionKind;

impl OwnedKind for ProgressionKind {
    const KIND: DataKind = DataKind::Progression;
    type Scope = (Experience, Points);
}

/// On each entity that holds a type of kind `K`: the slot that owns it, if one does. Replicon
/// reads it to send the kind's types to the owner's link alone. Immutable, as replicon requires:
/// a change of owner inserts a new one.
#[derive(Component, Debug)]
#[component(immutable)]
pub(crate) struct KindFilter<K: OwnedKind> {
    owner: Option<PlayerSlot>,
    kind: PhantomData<K>,
}

impl<K: OwnedKind> KindFilter<K> {
    const fn new(owner: Option<PlayerSlot>) -> KindFilter<K> {
        KindFilter {
            owner,
            kind: PhantomData,
        }
    }

    /// The filter of `K`, and the observers that follow its entity's owner.
    fn register_kind(app: &mut App) {
        app.add_visibility_filter::<KindFilter<K>>();
        KindFilter::<K>::follow_owner(app);
    }

    /// Inserts the filter of `K` again as its entity's owner changes, or as it loses its owner,
    /// on an entity that holds one. Each insert of a filter keeps to an entity that still lives:
    /// it applies at the next flush, after a despawn queued before it, and a despawn removes the
    /// owner too.
    fn follow_owner(app: &mut App) {
        app.add_observer(
            |inserted: On<'_, '_, Insert, Owner>,
             owners: Query<'_, '_, (&Owner, Has<KindFilter<K>>)>,
             mut commands: Commands<'_, '_>| {
                if let Ok((owner, true)) = owners.get(inserted.entity) {
                    commands
                        .entity(inserted.entity)
                        .try_insert(KindFilter::<K>::new(Some(owner.slot())));
                }
            },
        );
        app.add_observer(
            |removed: On<'_, '_, Remove, Owner>,
             filtered: Query<'_, '_, (), With<KindFilter<K>>>,
             mut commands: Commands<'_, '_>| {
                if filtered.contains(removed.entity) {
                    commands
                        .entity(removed.entity)
                        .try_insert(KindFilter::<K>::new(None));
                }
            },
        );
    }

    /// Puts the filter of `K` on each entity that gains `C`, one of `K`'s types, unless it holds
    /// it already.
    fn observe<C: Kinded>(app: &mut App) {
        assert_eq!(C::KIND, K::KIND, "{} is of its filter's kind", C::NAME);
        app.add_observer(
            |added: On<'_, '_, Add, C>,
             units: Query<'_, '_, (Option<&Owner>, Has<KindFilter<K>>)>,
             mut commands: Commands<'_, '_>| {
                if let Ok((owner, false)) = units.get(added.entity) {
                    let owner = owner.map(|owner| owner.slot());
                    commands
                        .entity(added.entity)
                        .try_insert(KindFilter::<K>::new(owner));
                }
            },
        );
    }
}

impl<K: OwnedKind> VisibilityFilter for KindFilter<K> {
    type ClientComponent = PlayerLink;
    type Scope = K::Scope;

    fn is_visible(&self, _client: Entity, link: Option<&PlayerLink>) -> bool {
        link.is_some_and(|link| Some(link.slot()) == self.owner)
    }
}

/// The filters of the kinds the owner alone receives.
#[derive(Debug)]
pub(crate) struct OwnedFilters;

impl OwnedFilters {
    /// Registers the filter of every kind the owner alone receives, and, from the lists of
    /// `capabilities`, the observers that put a kind's filter on each entity that gains one of
    /// its types: no system adds a filter, so none can forget one.
    pub(crate) fn register(app: &mut App, capabilities: CapabilitySet) {
        KindFilter::<PredictionKind>::register_kind(app);
        KindFilter::<InventoryKind>::register_kind(app);
        KindFilter::<ProgressionKind>::register_kind(app);
        capabilities.state_types(&mut FilterObservers(app));
    }
}

/// Reads the capabilities' lists for `OwnedFilters::register`: each type of a kind its owner alone
/// receives gets the observer that puts its kind's filter on it.
#[derive(Debug)]
struct FilterObservers<'a>(&'a mut App);

impl FilterObservers<'_> {
    fn filter<C: Kinded>(&mut self) {
        match C::KIND {
            DataKind::Prediction => KindFilter::<PredictionKind>::observe::<C>(self.0),
            DataKind::Inventory => KindFilter::<InventoryKind>::observe::<C>(self.0),
            DataKind::Progression => KindFilter::<ProgressionKind>::observe::<C>(self.0),
            DataKind::Unit | DataKind::Life | DataKind::Modifiers | DataKind::Server => {}
        }
    }
}

impl StateTypes for FilterObservers<'_> {
    fn component<C: Kinded + Component<Mutability = Mutable>>(&mut self) {
        self.filter::<C>();
    }

    fn component_once<C: Kinded + Component<Mutability = Mutable>>(&mut self) {
        self.filter::<C>();
    }

    fn predicted<C: Kinded + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
    ) {
        self.filter::<C>();
    }

    fn sim_predicted<C: Kinded + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
    ) {
        self.filter::<C>();
    }

    fn resource<R: SimResource>(&mut self) {}
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::collections::BTreeSet;

    use bevy_ecs::world::World;
    use campfire_capabilities::Team;
    use campfire_sim::{Capability, SimComponent};
    use serde::{Deserialize, Serialize};

    use super::*;

    /// A state type of `progression` that no scope holds.
    #[derive(Component, Debug, Serialize, Deserialize)]
    struct Stray;

    impl SimComponent for Stray {
        const NAME: &'static str = "test.stray";

        fn check(&self, _: &World, _: Entity) -> bool {
            true
        }
    }

    impl Kinded for Stray {
        const KIND: DataKind = DataKind::Progression;
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
            CapabilitySet::new(&declared)
                .unwrap()
                .state_types(&mut kinds);
            kinds
        }

        fn push<C: Kinded>(&mut self) {
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
        fn component<C: Kinded + Component<Mutability = Mutable>>(&mut self) {
            self.push::<C>();
        }

        fn component_once<C: Kinded + Component<Mutability = Mutable>>(&mut self) {
            self.push::<C>();
        }

        fn predicted<C: Kinded + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
            &mut self,
        ) {
            self.push::<C>();
        }

        fn sim_predicted<
            C: Kinded + Component<Mutability = Mutable> + Clone + PartialEq + Debug,
        >(
            &mut self,
        ) {
            self.push::<C>();
        }

        fn resource<R: SimResource>(&mut self) {}
    }

    /// The listed types of `K`'s kind its scope lacks, and the types of its scope no list gives
    /// that kind, by the names of the first and the count of the second: both empty.
    fn scope_of<K: OwnedKind>(kinds: &Kinds) -> (Vec<&'static str>, usize)
    where
        K::Scope: TypeList,
    {
        let scope: BTreeSet<TypeId> = K::Scope::type_ids().into_iter().collect();
        let lacking = kinds.names(K::KIND, |id| !scope.contains(&id));
        let listed = kinds.names(K::KIND, |id| scope.contains(&id));
        (lacking, scope.len() - listed.len())
    }

    #[test]
    fn each_owned_kinds_scope_holds_exactly_the_types_the_lists_give_it() {
        // A type a list gives a kind its scope lacks would go to every client, and one its scope
        // holds of another kind would go to the owner alone: each fails here by its name.
        let kinds = Kinds::of_every_capability();
        assert_eq!(scope_of::<PredictionKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<InventoryKind>(&kinds), (vec![], 0));
        assert_eq!(scope_of::<ProgressionKind>(&kinds), (vec![], 0));
        // The check itself: `Stray`, of the kind, which the scope lacks, fails by its name, and
        // `Points`, of the scope, which this list does not give, counts once.
        let mut stray = Kinds::default();
        stray.push::<Experience>();
        stray.push::<Stray>();
        assert_eq!(scope_of::<ProgressionKind>(&stray), (vec!["test.stray"], 1));
    }

    #[test]
    fn an_owned_kinds_filter_follows_its_entitys_owner() {
        // `Stray`, of `progression`: its filter holds the owner the entity has as it gains it,
        // the owner it changes to, and none once it loses it; a despawn changes nothing.
        let mut app = App::new();
        KindFilter::<ProgressionKind>::follow_owner(&mut app);
        KindFilter::<ProgressionKind>::observe::<Stray>(&mut app);
        let world = app.world_mut();
        let owner = |world: &World, entity| {
            world
                .get::<KindFilter<ProgressionKind>>(entity)
                .map(|filter| filter.owner)
        };
        let slot = PlayerSlot::new;
        let hero = world.spawn((Owner::new(slot(1)), Stray)).id();
        world.flush();
        assert_eq!(owner(world, hero), Some(Some(slot(1))));
        world.entity_mut(hero).insert(Owner::new(slot(2)));
        world.flush();
        assert_eq!(owner(world, hero), Some(Some(slot(2))));
        world.entity_mut(hero).remove::<Owner>();
        world.flush();
        assert_eq!(owner(world, hero), Some(None));
        // A unit no player owns, as a creep; and one that holds no type of the kind.
        let creep = world.spawn(Stray).id();
        let tower = world.spawn(Owner::new(slot(1))).id();
        world.flush();
        assert_eq!(owner(world, creep), Some(None));
        assert_eq!(owner(world, tower), None);
        world.entity_mut(hero).insert(Owner::new(slot(1)));
        world.flush();
        world.despawn(hero);
        world.flush();
        assert!(world.get_entity(hero).is_err());
    }

    #[test]
    fn only_the_owners_seated_link_holds_an_owned_kind() {
        let link = |slot| PlayerLink::new(PlayerSlot::new(slot), Team::new(0));
        let client = Entity::PLACEHOLDER;
        let owned = KindFilter::<PredictionKind>::new(Some(PlayerSlot::new(1)));
        assert!(owned.is_visible(client, Some(&link(1))));
        assert!(!owned.is_visible(client, Some(&link(0))));
        // A link with no seat, and a unit no player owns, as a creep: none holds it.
        assert!(!owned.is_visible(client, None));
        assert!(!KindFilter::<PredictionKind>::new(None).is_visible(client, Some(&link(1))));
    }
}
