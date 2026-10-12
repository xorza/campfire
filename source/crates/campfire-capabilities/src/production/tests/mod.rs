use std::num::NonZeroU8;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_codec::Binary;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_math::{Num, Vec3};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimComponent, SimWorld, StableId};
use serde::Serialize;

use crate::actions::Actions;
use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_kind::SlotKind;
use crate::navigation::destination::Destination;
use crate::players::Players;
use crate::players::players_world::{PlayersWorld, PlayersWorldCrate};
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::production::Production;
use crate::production::production_data::ProductionData;
use crate::production::supply_costs::SupplyCosts;
use crate::production::supply_data::SupplyData;
use crate::production::train_queue::{Queued, TrainQueue};
use crate::release::Release;
use crate::release::r#match::Match;
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::body_form::BodyForm;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::spawn_at::SpawnAt;
use crate::units::spawner::Spawner;
use crate::units::team::Team;
use crate::units::type_scope::TypeScope;
use crate::units::unit_core::UnitCore;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::declared_name::DeclaredName;
use crate::values::rank::Rank;

fn at(x: i64) -> Position {
    Position::new(Vec3::new(Num::from_int(x).unwrap(), Num::ZERO, Num::ZERO)).unwrap()
}

/// A match of production alone, for a shop.
fn shop_match() -> Match {
    shop_match_of(&[Capability::Production])
}

/// A shop's match with the orders that cancel its trains and set its rally points, and the
/// capabilities they build on.
fn ordering_match() -> Match {
    shop_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Orders,
        Capability::Production,
    ])
}

/// A match of `declared`, with production among them.
fn shop_match_of(declared: &[Capability]) -> Match {
    Match::client(declared)
}

/// A shop's match: a grunt type to train, a producer type of a queue of 3, a depot and a forge
/// type for supply and requirements to count, and the gold its players hold; its spawner logs each
/// spawn it is asked for.
#[derive(Debug, Clone, Copy)]
struct Shop {
    grunt: UnitType,
    barracks: UnitType,
    depot: UnitType,
    forge: UnitType,
    gold: ResourceId,
}

impl Shop {
    /// Loads the shop's types into `game`, a match with production, and gives it a spawner that
    /// logs each spawn.
    fn place(game: &mut Match) -> Shop {
        game.init_resource::<SpawnLog>();
        game.insert_non_send(Spawner::new(|world, at, owner| {
            world
                .resource_mut::<SpawnLog>()
                .0
                .push(Spawned { at, owner });
            let core = at.core(owner.map(Owner::new));
            core.spawn(world, Destination::default()).id()
        }));
        let data = UnitTypeData::default();
        let grunt = Units::load_type(game, TypeScope::Mode, "grunt", &data);
        let barracks = Units::load_type(game, TypeScope::Mode, "barracks", &data);
        let depot = Units::load_type(game, TypeScope::Mode, "depot", &data);
        let forge = Units::load_type(game, TypeScope::Mode, "forge", &data);
        let production = ProductionData {
            queue: NonZeroU8::new(3).unwrap(),
        };
        game.resource_mut::<ByType<ProductionData>>()
            .set(barracks, production);
        let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
        Shop {
            grunt,
            barracks,
            depot,
            forge,
            gold,
        }
    }

    /// Each spawn production asked for so far, in order.
    fn spawned(game: &World) -> &[Spawned] {
        &game.resource::<SpawnLog>().0
    }

    /// A producer `id` at `x` meters of `team`, owned by `owner`, with `train` in its slot 0
    /// and `queue`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the shop and the match, then the producer's id, place, team, owner, train and queue"
    )]
    fn producer(
        self,
        game: &mut World,
        id: StableId,
        x: i64,
        team: Team,
        owner: Option<PlayerSlot>,
        train: ActionId,
        queue: TrainQueue,
    ) {
        let slots = ActionSlots::new([(train, SlotKind::new(0), Rank::new(1))]).timed();
        let core = UnitCore {
            id,
            pos: Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::ZERO)).unwrap(),
            unit_type: self.barracks,
            team,
            owner: owner.map(Owner::new),
        };
        Units::spawn_core(game, core, (slots, queue));
    }

    /// Each spawn so far, by producer: the team and position of the producer it came from,
    /// and its owner.
    fn spawns(self, game: &World) -> Vec<(Team, Position, Option<PlayerSlot>)> {
        let grunt = self.grunt;
        let spawned = Shop::spawned(game);
        assert!(spawned.iter().all(|spawn| spawn.at.unit_type == grunt));
        spawned
            .iter()
            .map(|spawn| (spawn.at.team, spawn.at.pos, spawn.owner))
            .collect()
    }

    /// Player 0's gold.
    fn gold(self, game: &World) -> i64 {
        game.player_resources(PlayerSlot::new(0))
            .unwrap()
            .amount(self.gold)
    }

    /// The depots spawned so far, by stable id.
    fn depots(self, game: &World) -> Vec<StableId> {
        let depot = self.depot;
        Shop::spawned(game)
            .iter()
            .filter(|spawn| spawn.at.unit_type == depot)
            .map(|spawn| spawn.at.id)
            .collect()
    }

    /// Player 0's hall at `pos`.
    fn hall(self, game: &mut World, pos: Position) -> StableId {
        let body = BodyForm::box_sized([Num::int(4), Num::int(2)])
            .unwrap()
            .at(Num::ZERO);
        let parts = (Owner::new(PlayerSlot::new(0)), body);
        let depot = self.depot;
        Units::spawn_of(game, depot, pos, Team::new(0), parts)
    }
}

/// A spawn production asked for: what and where, and the player that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spawned {
    at: SpawnAt,
    owner: Option<PlayerSlot>,
}

/// Each spawn a shop's spawner was asked for, in order.
#[derive(Resource, Debug, Default)]
struct SpawnLog(Vec<Spawned>);

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
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let train = Actions::add_train(&mut game, shop.grunt, Ticks::ZERO, None);
    let player = PlayerSlot::new(0);
    // The first id goes to the producer spawned second, so the world holds them out of id order.
    let (first, second) = (
        Production::allocate_id(&mut game),
        Production::allocate_id(&mut game),
    );
    shop.producer(
        &mut game,
        second,
        20,
        Team::new(2),
        None,
        train,
        queue(train, &[0, 0, 1]),
    );
    shop.producer(
        &mut game,
        first,
        10,
        Team::new(1),
        Some(player),
        train,
        queue(train, &[0]),
    );
    let from_first = (Team::new(1), at(10), Some(player));
    let from_second = (Team::new(2), at(20), None);
    // Tick 0 ends at time 1. The first's train spawns first. The second's two trains of no time
    // spawn one after the other: each next starts at time 1 and is done at once. Its third, of
    // 1 tick, starts at time 1 and is done at time 2, the end of tick 1.
    game.step();
    assert_eq!(shop.spawns(&game), [from_first, from_second, from_second]);
    game.step();
    assert_eq!(
        shop.spawns(&game),
        [from_first, from_second, from_second, from_second]
    );

    // A train of no time ordered in tick 2 starts at time 2 and spawns at the end of tick 2.
    Production::order_train(&mut game, first);
    game.step();
    assert_eq!(shop.spawns(&game).len(), 5);
    assert_eq!(shop.spawns(&game)[4], from_first);

    // Dead through ticks 3 and 4, the first's queue holds a train done since time 0: it spawns in
    // tick 5, the tick the producer lives again.
    let entity = game.unit_entity(first);
    game.insert_parts(first, (queue(train, &[0]), Dead));
    game.step();
    game.step();
    assert_eq!(shop.spawns(&game).len(), 5);
    game.entity_mut(entity).remove::<Dead>();
    game.step();
    assert_eq!(shop.spawns(&game).len(), 6);
    assert_eq!(shop.spawns(&game)[5], from_first);
}

#[test]
fn a_producer_no_player_owns_affords_no_train_that_costs_a_resource() {
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
    let player = PlayerSlot::new(0);
    Players::spawn(&mut game, 1, 1);
    game.player_resources_mut(player)
        .unwrap()
        .add(gold, 5)
        .unwrap();
    let cost = ResourceAmount {
        resource: gold,
        amount: 2,
    };
    let train = Actions::add_train(&mut game, shop.grunt, Ticks::ZERO, Some(cost));
    let (owned, unowned) = (
        Production::allocate_id(&mut game),
        Production::allocate_id(&mut game),
    );
    shop.producer(
        &mut game,
        owned,
        10,
        Team::new(1),
        Some(player),
        train,
        TrainQueue::default(),
    );
    shop.producer(
        &mut game,
        unowned,
        20,
        Team::new(1),
        None,
        train,
        TrainQueue::default(),
    );
    // The owned producer pays 2 of its player's 5 gold, and its train spawns. The other has no
    // player to pay: its order ends, and nothing is paid, queued or spawned.
    Production::order_train(&mut game, owned);
    Production::order_train(&mut game, unowned);
    game.step();
    assert_eq!(shop.spawns(&game), [(Team::new(1), at(10), Some(player))]);
    let held = game.player_resources(player).unwrap().amount(gold);
    assert_eq!(held, 3);
    for id in [owned, unowned] {
        let entity = game.unit_entity(id);
        assert_eq!(game.get::<ActionSlots>(entity).unwrap().in_progress(), None);
        assert_eq!(game.get::<TrainQueue>(entity), Some(&TrainQueue::default()));
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
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let train = Actions::add_train(&mut game, shop.grunt, Ticks::new(3), None);
    let id = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        id,
        10,
        Team::new(1),
        None,
        train,
        queue(train, &[3, 3]),
    );
    game.step();
    // A restore loads the match's books first: the same train.
    let mut restored = shop_match();
    let restored_shop = Shop::place(&mut restored);
    Actions::add_train(&mut restored, restored_shop.grunt, Ticks::new(3), None);
    Release::restore_into(&game, &mut restored);
    // A train of the longest time, pushed at tick 0, is done at the limit; pushed a tick later,
    // or a tick longer, it fails to decode.
    let entity = restored.unit_entity(id);
    let check = |pushed: u64, time: u64| {
        let mut queue = TrainQueue::default();
        let queued = Queued {
            action: train,
            rank: Rank::FIRST,
            time: Ticks::new(time),
            paid: 0,
        };
        queue.push(queued, &[], Tick::new(pushed));
        Match::decodes(&queue) && queue.check(&restored, entity)
    };
    assert!(check(0, Tick::LIMIT.get()));
    assert!(!check(1, Tick::LIMIT.get()) && !check(0, Tick::LIMIT.get() + 1));
    assert_eq!(restored.part::<TrainQueue>(id), game.part::<TrainQueue>(id));
}

mod construction;
mod gathering;
mod orders;
mod supply;
