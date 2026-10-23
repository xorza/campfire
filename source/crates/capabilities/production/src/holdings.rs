use bevy_ecs::change_detection::{CheckChangeTicks, DetectChanges, Ref, Tick};
use bevy_ecs::entity::{Entities, Entity};
use bevy_ecs::lifecycle::{Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut, SystemChangeTick, SystemParam};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_declarations::Cache;
use campfire_sim::Unpredicted;
use campfire_stats::PlayerModifiers;
use campfire_units::{Dead, Owner};
use campfire_values::UnitType;

use crate::requirement_book::Required;
use crate::site::Site;
use crate::supply::{SupplyParts, SupplyPartsItem, SupplySums};
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::{SupplyCostBook, UnitSupply};
use crate::train_queue::TrainQueue;

/// What the players hold: each one's supply, and the types of its living, complete units, which
/// a requirement finds. It keeps what each unit counts for, and follows only the units that
/// changed since: those whose owner, type, death, site or presence came or went, as its
/// observers mark them, and those whose queue changed, by its change tick. Derived, never state:
/// a following reader finds it as a count of the match would (04-capabilities/production.md,
/// Supply and holdings follow what changed).
#[derive(Resource, Debug, Default)]
pub(crate) struct Holdings {
    /// What each unit counts for, by its entity's index.
    units: Vec<Option<Share>>,
    /// Each player's supply, by slot.
    players: Vec<SupplySums>,
    /// The types of the living, complete units each player holds, each with its count, by owner,
    /// then type.
    held: Vec<HeldCount>,
    /// The units the observers marked since the last follow.
    marked: Vec<Entity>,
    /// The run of the last follow; none before the first, which counts every unit.
    followed: Option<Tick>,
}

/// What a unit counts for: its owner's supply and, living and complete, its type.
#[derive(Debug, Clone, Copy)]
struct Share {
    entity: Entity,
    owner: PlayerSlot,
    supply: UnitSupply,
    held: Option<UnitType>,
}

/// How many living, complete units of a type a player holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeldCount {
    owner: PlayerSlot,
    unit_type: UnitType,
    count: u32,
}

/// The units the players own as the holdings follow them: what supply counts of each, in a mode
/// that counts it, the queues, by change tick, and whether an entity a share names still stands.
#[derive(SystemParam, Debug)]
pub(crate) struct OwnedUnits<'w, 's> {
    rules: Res<'w, SupplyBook>,
    costs: Res<'w, SupplyCostBook>,
    units: Query<'w, 's, (Entity, &'static Owner, SupplyParts)>,
    queues: Query<'w, 's, (Entity, Ref<'static, TrainQueue>)>,
    entities: &'w Entities,
    ticks: SystemChangeTick,
}

impl Holdings {
    /// Follows `owned`: counts again each unit marked or whose queue changed since the last
    /// follow, or every unit at the first, and after a supply book changed.
    pub(crate) fn follow(&mut self, owned: &OwnedUnits<'_, '_>) {
        let now = owned.ticks.this_run();
        // A change in the last follow's own run may have come after it read.
        let changed = |at: Tick, last: Tick| at == last || at.is_newer_than(last, now);
        let books = [owned.rules.last_changed(), owned.costs.last_changed()];
        let last = self
            .followed
            .filter(|&last| !books.iter().any(|&book| changed(book, last)));
        let Some(last) = last else {
            self.units.clear();
            self.players.clear();
            self.held.clear();
            self.marked.clear();
            for (entity, owner, parts) in &owned.units {
                self.add(entity, owner.player_slot(), &parts, owned);
            }
            self.followed = Some(now);
            return;
        };
        self.marked.extend(
            owned
                .queues
                .iter()
                .filter(|(_, queue)| changed(queue.last_changed(), last))
                .map(|(entity, _)| entity),
        );
        self.marked.sort_unstable();
        self.marked.dedup();
        let mut marked = std::mem::take(&mut self.marked);
        for &entity in &marked {
            self.recount(entity, owned);
        }
        marked.clear();
        self.marked = marked;
        self.followed = Some(now);
    }

    /// What `player_slot` uses.
    pub(crate) fn used(&self, player_slot: PlayerSlot) -> u64 {
        self.players
            .get(player_slot.index())
            .map_or(0, |sums| sums.used)
    }

    /// What `player_slot`'s units give, at most `max`.
    pub(crate) fn cap(&self, player_slot: PlayerSlot, max: u64) -> u64 {
        let sums = self.players.get(player_slot.index()).copied();
        sums.unwrap_or_default().cap(max)
    }

    /// Whether `player_slot` has room for `cost` more under its cap of at most `max`.
    pub(crate) fn has_room(&self, player_slot: PlayerSlot, cost: u32, max: u64) -> bool {
        self.used(player_slot) + u64::from(cost) <= self.cap(player_slot, max)
    }

    /// Counts `cost` more used by `player_slot`, as a train joins the queue of `producer`, whose
    /// share then holds it, so a count of its queue gives the same.
    pub(crate) fn reserve(&mut self, producer: Entity, player_slot: PlayerSlot, cost: u32) {
        let at = Self::index(producer);
        let share = self.units.get_mut(at).and_then(Option::as_mut);
        let share = share.filter(|share| share.entity == producer && share.owner == player_slot);
        let share = share.expect("a train joins the queue of a unit of its player's");
        share.supply.used += u64::from(cost);
        self.sums_mut(player_slot).used += u64::from(cost);
    }

    /// Whether a follow counted the units, as each tick's first does.
    pub(crate) const fn followed(&self) -> bool {
        self.followed.is_some()
    }

    /// Whether `owner`, who holds `modifiers`, holds what `required` needs, a unit of one type of
    /// each of its entries: one with no `requires` needs nothing, and a unit no player owns meets
    /// no need.
    pub(crate) fn meet(
        &self,
        owner: Option<PlayerSlot>,
        modifiers: Option<&PlayerModifiers>,
        required: Option<Required<'_>>,
    ) -> bool {
        debug_assert!(
            self.followed.is_some(),
            "the holdings follow the units before a requirement reads them"
        );
        let Some(required) = required else {
            return true;
        };
        let Some(owner) = owner else {
            return false;
        };
        let mut units = required.units();
        let units = units.all(|any| {
            any.iter()
                .any(|&unit_type| self.held_at(owner, unit_type).is_ok())
        });
        let modifiers = required
            .modifiers
            .iter()
            .all(|&modifier| modifiers.is_some_and(|held| held.holds(modifier)));
        units && modifiers
    }

    /// Marks each unit that gains a part the holdings count, or that a client starts to hide.
    pub(crate) fn inserted(
        inserted: On<'_, '_, Insert<(Owner, UnitType, Dead, Site, TrainQueue, Unpredicted)>>,
        mut holdings: ResMut<'_, Self>,
    ) {
        holdings.mark(inserted.entity);
    }

    /// Marks each unit that loses a part the holdings count, is despawned, or that a client
    /// starts to predict.
    pub(crate) fn removed(
        removed: On<'_, '_, Remove<(Owner, UnitType, Dead, Site, TrainQueue, Unpredicted)>>,
        mut holdings: ResMut<'_, Self>,
    ) {
        holdings.mark(removed.entity);
    }

    /// The observer of Bevy's check of change ticks, which clamps each tick older than the
    /// maximum change age: the last follow's clamps too, so a queue's change after it, clamped
    /// to the same tick, still reads as one since.
    pub(crate) fn aged(aged: On<'_, '_, CheckChangeTicks>, mut holdings: ResMut<'_, Self>) {
        if let Some(followed) = &mut holdings.followed {
            followed.check_tick(*aged);
        }
    }

    /// Marks `entity` for the next follow; before the first, which counts every unit, none.
    fn mark(&mut self, entity: Entity) {
        if self.followed.is_some() {
            self.marked.push(entity);
        }
    }

    /// Counts `entity` again: takes out the share its index holds, its own or that of a unit gone
    /// whose index it took, and puts in its own as it stands, if the players own it.
    fn recount(&mut self, entity: Entity, owned: &OwnedUnits<'_, '_>) {
        let at = Self::index(entity);
        if let Some(&Some(share)) = self.units.get(at)
            && (share.entity == entity || !owned.entities.contains(share.entity))
        {
            self.units[at] = None;
            self.take(share);
        }
        if let Ok((entity, owner, parts)) = owned.units.get(entity) {
            self.add(entity, owner.player_slot(), &parts, owned);
        }
    }

    /// Adds what the unit `entity` that `owner` owns counts for, as `parts` say: its supply only
    /// in a mode that counts it.
    fn add(
        &mut self,
        entity: Entity,
        owner: PlayerSlot,
        parts: &SupplyPartsItem<'_, '_>,
        owned: &OwnedUnits<'_, '_>,
    ) {
        let Some(unit) = parts.unit() else {
            return;
        };
        let supply = if owned.rules.counts() {
            owned.costs.unit(unit)
        } else {
            UnitSupply::default()
        };
        let share = Share {
            entity,
            owner,
            supply,
            held: parts.held(),
        };
        let at = Self::index(entity);
        if self.units.len() <= at {
            self.units.resize(at + 1, None);
        }
        debug_assert!(
            self.units[at].is_none(),
            "a unit's share is taken out first"
        );
        self.units[at] = Some(share);
        self.sums_mut(owner).add(share.supply);
        if let Some(unit_type) = share.held {
            match self.held_at(owner, unit_type) {
                Ok(found) => self.held[found].count += 1,
                Err(at) => self.held.insert(
                    at,
                    HeldCount {
                        owner,
                        unit_type,
                        count: 1,
                    },
                ),
            }
        }
    }

    /// Takes out what `share` counted for.
    fn take(&mut self, share: Share) {
        self.sums_mut(share.owner).take(share.supply);
        if let Some(unit_type) = share.held {
            let found = self
                .held_at(share.owner, unit_type)
                .expect("a held unit's type is counted");
            self.held[found].count -= 1;
            if self.held[found].count == 0 {
                self.held.remove(found);
            }
        }
    }

    /// Where `owner`'s count of `unit_type` stands among the held, or would.
    fn held_at(&self, owner: PlayerSlot, unit_type: UnitType) -> Result<usize, usize> {
        self.held
            .binary_search_by_key(&(owner, unit_type), |held| (held.owner, held.unit_type))
    }

    /// `player_slot`'s supply, to change, the players grown to hold it.
    fn sums_mut(&mut self, player_slot: PlayerSlot) -> &mut SupplySums {
        let at = player_slot.index();
        if self.players.len() <= at {
            self.players.resize(at + 1, SupplySums::default());
        }
        &mut self.players[at]
    }

    fn index(entity: Entity) -> usize {
        usize::try_from(entity.index_u32()).expect("an entity's index fits usize")
    }
}

impl Cache for Holdings {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::system::SystemState;
    use campfire_declarations::{ById, ByIdBuilder};

    use super::*;
    use crate::mode_supply_data::ModeSupplyData;
    use crate::supply_data::SupplyData;

    /// The holdings of `world` as they follow its units.
    fn follow(
        world: &mut World,
        state: &mut SystemState<(ResMut<'_, Holdings>, OwnedUnits<'_, '_>)>,
    ) {
        let (mut holdings, owned) = state.get_mut(world).unwrap();
        holdings.follow(&owned);
        state.apply(world);
    }

    #[test]
    fn a_unit_that_takes_a_freed_index_before_the_follow_replaces_the_share_the_index_held() {
        // Depots give 4 and are held; a mode of a most of 10.
        let depot = UnitType::new(1);
        let mut types = ByIdBuilder::default();
        types.set(
            depot,
            SupplyData {
                cost: 0,
                provides: 4,
            },
        );
        let mut world = World::new();
        world.insert_resource(SupplyBook(Some(ModeSupplyData { max: 10 })));
        world.insert_resource(SupplyCostBook::new(types.build(), &ById::default()));
        world.init_resource::<Holdings>();
        world.add_observer(Holdings::inserted);
        world.add_observer(Holdings::removed);
        let mut state = SystemState::new(&mut world);
        let player = PlayerSlot::new(0);
        let owned = (Owner::new(player), depot);
        let first = world.spawn(owned).id();
        let second = world.spawn(owned).id();
        follow(&mut world, &mut state);
        let holdings = world.resource::<Holdings>();
        assert_eq!(holdings.cap(player, 10), 8);
        assert!(holdings.held_at(player, depot).is_ok());
        // The first despawns and a new depot takes its index, of the next generation, before the
        // next follow: the follow takes out the first's share as it counts the new one,
        // 8 - 4 + 4 = 8, not 12.
        let reused = world.despawn_no_free(first).unwrap();
        let third = world.spawn_at(reused, owned).unwrap().id();
        assert_eq!((third.index(), third == first), (first.index(), false));
        follow(&mut world, &mut state);
        let holdings = world.resource::<Holdings>();
        assert_eq!(holdings.cap(player, 10), 8);
        assert_eq!(holdings.held[0].count, 2);
        // The marks go in entity order, the older generation first; counted the other way, the
        // new depot's count still takes out the share of the one gone whose index it took, and
        // the count of the one gone leaves the new one's.
        let reused = world.despawn_no_free(third).unwrap();
        let fourth = world.spawn_at(reused, owned).unwrap().id();
        let (mut holdings, owned) = state.get_mut(&mut world).unwrap();
        holdings.marked.clear();
        holdings.recount(fourth, &owned);
        holdings.recount(third, &owned);
        assert_eq!(holdings.cap(player, 10), 8);
        assert_eq!(holdings.held[0].count, 2);
        state.apply(&mut world);
        let third = fourth;
        // Both go: nothing is given, and no depot is held.
        world.despawn(second);
        world.despawn(third);
        follow(&mut world, &mut state);
        let holdings = world.resource::<Holdings>();
        assert_eq!(holdings.cap(player, 10), 0);
        assert!(holdings.held.is_empty());
    }

    #[test]
    fn a_cap_is_what_the_units_give_at_most_the_max_and_a_reserve_adds_to_what_is_used() {
        let [first, second, absent] = [0, 1, 5].map(PlayerSlot::new);
        let producer = Entity::from_raw_u32(3).unwrap();
        let mut holdings = Holdings {
            players: vec![
                SupplySums { used: 7, given: 30 },
                SupplySums { used: 2, given: 8 },
            ],
            units: vec![
                None,
                None,
                None,
                Some(Share {
                    entity: producer,
                    owner: first,
                    supply: UnitSupply { used: 7, given: 30 },
                    held: None,
                }),
            ],
            ..Holdings::default()
        };
        assert_eq!(
            [
                holdings.cap(first, 20),
                holdings.cap(second, 20),
                holdings.cap(absent, 20)
            ],
            [20, 8, 0]
        );
        // 7 + 13 = 20 fits a cap of 20; 7 + 14 does not.
        assert!(holdings.has_room(first, 13, 20) && !holdings.has_room(first, 14, 20));
        holdings.reserve(producer, first, 13);
        assert_eq!(holdings.used(first), 20);
        assert!(holdings.has_room(first, 0, 20) && !holdings.has_room(first, 1, 20));
        // The producer's share holds the reserve, so a count of its queue takes out all 20.
        let share = holdings.units[3].unwrap();
        assert_eq!(share.supply.used, 20);
        holdings.take(share);
        assert_eq!((holdings.used(first), holdings.cap(first, 20)), (0, 0));
    }
}
