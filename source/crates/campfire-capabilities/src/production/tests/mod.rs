use std::cell::RefCell;
use std::num::NonZeroU8;
use std::rc::Rc;

use campfire_common::{Binary, PlayerSlot, Tick, Ticks};
use campfire_math::{Num, Vec3};
use campfire_sim::{Capability, IdAllocator, Position, SimComponent, StableId};
use serde::Serialize;

use crate::actions::action_book::ActionBook;
use crate::actions::action_book::internals;
use crate::actions::action_slots::ActionSlots;
use crate::actions::action_target::ActionTarget;
use crate::actions::slot_kind::SlotKind;
use crate::capability_set::test_match::TestMatch;
use crate::navigation::destination::Destination;
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::production::production_data::ProductionData;
use crate::production::supply_costs::SupplyCosts;
use crate::production::supply_data::SupplyData;
use crate::production::supply_rules::SupplyRules;
use crate::production::train_queue::{Queued, TrainQueue};
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::spawn_at::SpawnAt;
use crate::units::spawner::Spawner;
use crate::units::team::Team;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::declared_name::DeclaredName;
use crate::values::rank::Rank;

fn at(x: i64) -> Position {
    Position::new(Vec3::new(Num::from_int(x).unwrap(), Num::ZERO, Num::ZERO)).unwrap()
}

/// A spawn production asked for: what and where, and the player that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spawned {
    at: SpawnAt,
    owner: Option<PlayerSlot>,
}

/// A match of production, whose spawner records each spawn and gives it its owner and a
/// destination, with a grunt type to train, a producer type of a queue of 3, and a depot and a
/// forge type for supply and requirements to count.
#[derive(Debug)]
struct Shop {
    sim: TestMatch,
    spawned: Rc<RefCell<Vec<Spawned>>>,
    grunt: UnitType,
    barracks: UnitType,
    depot: UnitType,
    forge: UnitType,
}

impl Shop {
    /// A match of production alone.
    fn new() -> Shop {
        Shop::with(&[Capability::Production])
    }

    /// A match of production with the orders that cancel its trains and set its rally points,
    /// and the capabilities they build on.
    fn ordering() -> Shop {
        Shop::with(&[
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Orders,
            Capability::Production,
        ])
    }

    fn with(declared: &[Capability]) -> Shop {
        let mut sim = TestMatch::client(declared);
        let world = &mut sim.world;
        let spawned = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&spawned);
        world.insert_non_send(Spawner::new(move |world, at, owner| {
            log.borrow_mut().push(Spawned { at, owner });
            let parts = (at.id, at.unit_type, at.team, at.pos, Destination::default());
            let mut unit = world.spawn(parts);
            if let Some(owner) = owner {
                unit.insert(Owner::new(owner));
            }
            unit.id()
        }));
        let data = UnitTypeData::default();
        let grunt = Units::load_type(world, TypeScope::Mode, "grunt", &data);
        let barracks = Units::load_type(world, TypeScope::Mode, "barracks", &data);
        let depot = Units::load_type(world, TypeScope::Mode, "depot", &data);
        let forge = Units::load_type(world, TypeScope::Mode, "forge", &data);
        let production = ProductionData {
            queue: NonZeroU8::new(3).unwrap(),
        };
        let mut producers = world.resource_mut::<ByType<ProductionData>>();
        producers.set(barracks, production);
        Shop {
            sim,
            spawned,
            grunt,
            barracks,
            depot,
            forge,
        }
    }

    /// Counts supply, up to `max`, with each type's `supplies`, and the trains the book holds.
    fn count_supply(&mut self, max: u32, supplies: &[(UnitType, SupplyData)]) {
        let world = &mut self.sim.world;
        let mut types = ByType::default();
        for &(unit_type, supply) in supplies {
            types.set(unit_type, supply);
        }
        let costs = SupplyCosts::new(types, world.resource::<ActionBook>());
        world.insert_resource(costs);
        world.insert_resource(SupplyRules { max });
    }

    /// A unit of `unit_type` owned by `owner`, at the origin.
    fn owned(&mut self, unit_type: UnitType, owner: PlayerSlot) -> StableId {
        let parts = (unit_type, Team::new(1), Owner::new(owner));
        self.sim.spawn(at(0), parts)
    }

    /// The trains in `producer`'s queue.
    fn queued(&self, producer: StableId) -> usize {
        self.sim.get::<TrainQueue>(producer).entries().len()
    }

    /// A producer `id` of `team` at `x` meters, owned by `owner`, with `train` in its slot 0
    /// and `queue`.
    fn producer(
        &mut self,
        id: StableId,
        team: u8,
        x: i64,
        owner: Option<PlayerSlot>,
        train: ActionId,
        queue: TrainQueue,
    ) {
        let slots = ActionSlots::new([(train, SlotKind::new(0), Rank::new(1))]);
        let parts = (id, at(x), self.barracks, Team::new(team), slots, queue);
        let mut producer = self.sim.world.spawn(parts);
        if let Some(owner) = owner {
            producer.insert(Owner::new(owner));
        }
    }

    fn id(&mut self) -> StableId {
        self.sim.world.resource_mut::<IdAllocator>().allocate()
    }

    fn order(&mut self, producer: StableId) {
        let mut slots = self.sim.get_mut::<ActionSlots>(producer);
        slots.order(0, ActionTarget::None);
    }

    fn tick(&mut self) {
        self.sim.step();
    }

    /// Each spawn so far, by producer: the team and position of the producer it came from, and
    /// its owner.
    fn spawns(&self) -> Vec<(Team, Position, Option<PlayerSlot>)> {
        let spawned = self.spawned.borrow();
        assert!(spawned.iter().all(|spawn| spawn.at.unit_type == self.grunt));
        spawned
            .iter()
            .map(|spawn| (spawn.at.team, spawn.at.pos, spawn.owner))
            .collect()
    }
}

/// A queue of `train`, once for each of `times`, in ticks, pushed at tick 0.
fn queue(train: ActionId, times: &[u64]) -> TrainQueue {
    let mut queue = TrainQueue::default();
    for &time in times {
        let queued = Queued {
            action: train,
            rank: Rank::FIRST,
            time: Ticks::new(time),
            paid: 0,
        };
        queue.push(queued, &[], Tick::new(0));
    }
    queue
}

#[test]
fn trains_spawn_by_their_producers_stable_id_and_a_dead_producers_queue_waits() {
    let mut shop = Shop::new();
    let train = internals::train(&mut shop.sim.world, shop.grunt, Ticks::ZERO, None);
    let player = PlayerSlot::new(0);
    // The first id goes to the producer spawned second, so the world holds them out of id order.
    let (first, second) = (shop.id(), shop.id());
    shop.producer(second, 2, 20, None, train, queue(train, &[0, 0, 1]));
    shop.producer(first, 1, 10, Some(player), train, queue(train, &[0]));
    let from_first = (Team::new(1), at(10), Some(player));
    let from_second = (Team::new(2), at(20), None);
    // Tick 0 ends at time 1. The first's train spawns first. The second's two trains of no time
    // spawn one after the other: each next starts at time 1 and is done at once. Its third, of
    // 1 tick, starts at time 1 and is done at time 2, the end of tick 1.
    shop.tick();
    assert_eq!(shop.spawns(), [from_first, from_second, from_second]);
    shop.tick();
    assert_eq!(
        shop.spawns(),
        [from_first, from_second, from_second, from_second]
    );

    // A train of no time ordered in tick 2 starts at time 2 and spawns at the end of tick 2.
    shop.order(first);
    shop.tick();
    assert_eq!(shop.spawns().len(), 5);
    assert_eq!(shop.spawns()[4], from_first);

    // Dead through ticks 3 and 4, the first's queue holds a train done since time 0: it spawns in
    // tick 5, the tick the producer lives again.
    let entity = shop.sim.entity(first);
    shop.sim.insert(first, (queue(train, &[0]), Dead));
    shop.tick();
    shop.tick();
    assert_eq!(shop.spawns().len(), 5);
    shop.sim.world.entity_mut(entity).remove::<Dead>();
    shop.tick();
    assert_eq!(shop.spawns().len(), 6);
    assert_eq!(shop.spawns()[5], from_first);
}

#[test]
fn a_producer_no_player_owns_affords_no_train_that_costs_a_resource() {
    let mut shop = Shop::new();
    let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
    let player = PlayerSlot::new(0);
    let mut resources = PlayerResources::new(1, 1);
    resources.add(player, gold, 5).unwrap();
    shop.sim.world.insert_resource(resources);
    let cost = ResourceAmount {
        resource: gold,
        amount: 2,
    };
    let train = internals::train(&mut shop.sim.world, shop.grunt, Ticks::ZERO, Some(cost));
    let (owned, unowned) = (shop.id(), shop.id());
    shop.producer(owned, 1, 10, Some(player), train, TrainQueue::default());
    shop.producer(unowned, 1, 20, None, train, TrainQueue::default());
    // The owned producer pays 2 of its player's 5 gold, and its train spawns. The other has no
    // player to pay: its order ends, and nothing is paid, queued or spawned.
    shop.order(owned);
    shop.order(unowned);
    shop.tick();
    assert_eq!(shop.spawns(), [(Team::new(1), at(10), Some(player))]);
    let held = shop
        .sim
        .world
        .resource::<PlayerResources>()
        .amount(player, gold);
    assert_eq!(held, 3);
    for id in [owned, unowned] {
        let entity = shop.sim.entity(id);
        assert_eq!(
            shop.sim
                .world
                .get::<ActionSlots>(entity)
                .unwrap()
                .in_progress(),
            None
        );
        assert_eq!(
            shop.sim.world.get::<TrainQueue>(entity),
            Some(&TrainQueue::default())
        );
    }
}

#[test]
fn a_queue_decodes_only_with_a_head_time_exactly_when_it_has_a_head_and_its_paid_runs() {
    #[derive(Debug, Serialize)]
    struct Fields {
        entries: Vec<Queued>,
        head_done: Option<Tick>,
        paid: Vec<ResourceAmount>,
    }
    let decode = |entries: Vec<Queued>, head_done: Option<u64>, paid: Vec<ResourceAmount>| {
        let head_done = head_done.map(Tick::new);
        let fields = Fields {
            entries,
            head_done,
            paid,
        };
        Binary::decode::<TrainQueue>(&Binary::encode(&fields))
    };
    let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
    let five = ResourceAmount {
        resource: gold,
        amount: 5,
    };
    let queued = Queued {
        action: ActionId::new(0),
        rank: Rank::FIRST,
        time: Ticks::new(30),
        paid: 1,
    };
    // Pushed at tick 5, a train of 30 ticks, which paid 5 gold, is done at tick 35.
    let mut queue = TrainQueue::default();
    queue.push(queued, &[five], Tick::new(5));
    assert_eq!(decode(vec![queued], Some(35), vec![five]), Ok(queue));
    assert_eq!(decode(vec![], None, vec![]), Ok(TrainQueue::default()));
    assert!(decode(vec![], Some(35), vec![]).is_err());
    assert!(decode(vec![queued], None, vec![five]).is_err());
    // A paid run longer or shorter than the entries say.
    assert!(decode(vec![queued], Some(35), vec![]).is_err());
    assert!(decode(vec![queued], Some(35), vec![five, five]).is_err());
}

#[test]
fn a_train_queue_is_state_and_restores() {
    // A queue of two trains, the first ordered at tick 0 to be done at tick 3.
    let mut shop = Shop::new();
    let train = internals::train(&mut shop.sim.world, shop.grunt, Ticks::new(3), None);
    let id = shop.id();
    shop.producer(id, 1, 10, None, train, queue(train, &[3, 3]));
    shop.tick();
    // A restore loads the match's books first: the same train.
    let mut restored = Shop::new();
    internals::train(&mut restored.sim.world, restored.grunt, Ticks::new(3), None);
    shop.sim.restore_into(&mut restored.sim);
    // A train of the longest time, pushed at tick 0, is done at the limit; pushed a tick later,
    // or a tick longer, it fails to decode.
    let entity = restored.sim.entity(id);
    let check = |pushed: u64, time: u64| {
        let mut queue = TrainQueue::default();
        let queued = Queued {
            action: train,
            rank: Rank::FIRST,
            time: Ticks::new(time),
            paid: 0,
        };
        queue.push(queued, &[], Tick::new(pushed));
        TestMatch::decodes(&queue) && queue.check(&restored.sim.world, entity)
    };
    assert!(check(0, Tick::LIMIT.get()));
    assert!(!check(1, Tick::LIMIT.get()) && !check(0, Tick::LIMIT.get() + 1));
    assert_eq!(
        restored.sim.get::<TrainQueue>(id),
        shop.sim.get::<TrainQueue>(id)
    );
}

mod construction;
mod gathering;
mod orders;
mod supply;
