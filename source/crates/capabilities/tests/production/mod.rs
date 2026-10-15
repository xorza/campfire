use std::num::NonZeroU8;

use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, Actions, SlotKind, TimedSlots};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_codec::Binary;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_declare::{ByType, PerType};
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_players::{PlayerResources, Players, PlayersWorld, PlayersWorldCrate, ResourceAmount};
use campfire_production::*;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimComponent, SimWorld, StableId, Unpredicted};
use campfire_units::{
    BodyForm, Dead, Destination, MoveStep, Owner, SpawnAt, SpawnPoint, Spawner, UnitTypeCore, Units,
};
use campfire_values::Rank;
use campfire_values::{ActionId, ResourceId, Team, TypeScope, UnitType};
use serde::Serialize;

/// The point `(x, z)` in halves of a meter.
fn halves(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::HALF * x, Num::ZERO, Num::HALF * z)).unwrap()
}

/// A match of production alone, for a shop.
fn shop_match() -> Match {
    shop_match_of(&[Capability::Production])
}

/// The capabilities of an ordering match: production, the orders that cancel its trains and set
/// its rally points, and the capabilities they build on.
const ORDERING: [Capability; 5] = [
    Capability::Stats,
    Capability::Combat,
    Capability::Navigation,
    Capability::Orders,
    Capability::Production,
];

/// A shop's match of `ORDERING`.
fn ordering_match() -> Match {
    shop_match_of(&ORDERING)
}

/// A match of `declared`, with production among them.
fn shop_match_of(declared: &[Capability]) -> Match {
    Match::unscripted(declared)
}

/// A shop's match: a grunt type to train, a circle of 0.5 m that walks 1 m a tick, a producer type of a queue of 3,
/// a depot and a forge type for supply and requirements to count, and the gold its players hold;
/// the mode spawns each type.
#[derive(Debug, Clone, Copy)]
struct Shop {
    grunt: UnitType,
    barracks: UnitType,
    depot: UnitType,
    forge: UnitType,
    gold: ResourceId,
}

impl Shop {
    /// Loads the shop's types into `game`, a match with production, as the mode's.
    fn place(game: &mut Match) -> Shop {
        let data = UnitTypeCore::default();
        let grunt = Units::load_type(game, TypeScope::Mode, "grunt", &data);
        let barracks = Units::load_type(game, TypeScope::Mode, "barracks", &data);
        let depot = Units::load_type(game, TypeScope::Mode, "depot", &data);
        let forge = Units::load_type(game, TypeScope::Mode, "forge", &data);
        let production = ProductionData {
            queue: NonZeroU8::new(3).unwrap(),
        };
        Shop::give(game, barracks, production);
        Shop::give(game, grunt, MoveStep::new(Num::ONE).unwrap());
        Shop::give(game, grunt, BodyForm::circle(Num::HALF).unwrap());
        Mode::load_test_units(game);
        let gold = ResourceId::nth(0);
        Shop {
            grunt,
            barracks,
            depot,
            forge,
            gold,
        }
    }

    /// Gives `unit_type` `value` in its book, as the books of a mode's type do.
    fn give<T: PerType>(game: &mut World, unit_type: UnitType, value: T) {
        game.resource_mut::<ByType<T>>().set(unit_type, value);
    }

    /// Each unit of `unit_type` so far, by stable id: in the order production spawned them.
    fn spawned(game: &mut World, unit_type: UnitType) -> Vec<Spawned> {
        let mut units = game.query::<(&StableId, &UnitType, &Team, &SpawnPoint, Option<&Owner>)>();
        let mut spawned: Vec<Spawned> = units
            .iter(game)
            .filter(|&(_, &of, ..)| of == unit_type)
            .map(|(&id, _, &team, point, owner)| Spawned {
                id,
                team,
                pos: point.get(),
                owner: owner.map(|owner| owner.slot()),
            })
            .collect();
        spawned.sort_unstable_by_key(|spawned| spawned.id);
        spawned
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
        let slots = TimedSlots::new(ActionSlots::new([(train, SlotKind::new(0), Rank::new(1))]));
        let at = SpawnAt {
            id,
            unit_type: self.barracks,
            team,
            pos: Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::ZERO)).unwrap(),
            owner,
            angle: Num::ZERO,
        };
        let spawner = game.non_send::<Spawner>().clone();
        spawner.spawn(game, at, (slots, queue));
    }

    /// Each grunt trained so far, in order: the team and position of the producer it came from,
    /// and its owner.
    fn spawns(self, game: &mut World) -> Vec<(Team, Position, Option<PlayerSlot>)> {
        let spawned = Shop::spawned(game, self.grunt);
        spawned
            .iter()
            .map(|spawn| (spawn.team, spawn.pos, spawn.owner))
            .collect()
    }

    /// Player 0's gold.
    fn gold(self, game: &World) -> i64 {
        PlayerResources::of_player(game, PlayerSlot::new(0))
            .unwrap()
            .amount(self.gold)
    }

    /// The depots spawned so far, by stable id.
    fn depots(self, game: &mut World) -> Vec<StableId> {
        let spawned = Shop::spawned(game, self.depot);
        spawned.iter().map(|spawn| spawn.id).collect()
    }

    /// Player 0's hall at `pos`.
    fn hall(self, game: &mut World, pos: impl Into<Position>) -> StableId {
        let pos = pos.into();
        let body = BodyForm::box_sized([Num::int(4), Num::int(2)])
            .unwrap()
            .at(Num::ZERO);
        let parts = (Owner::new(PlayerSlot::new(0)), body);
        Mode::spawn(game, self.depot, pos, Team::new(0), parts)
    }
}

/// A unit as it spawned: its stable id, team and place, and the player that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spawned {
    id: StableId,
    team: Team,
    pos: Position,
    owner: Option<PlayerSlot>,
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
    let from_first = (Team::new(1), Position::from((10, 0, 0)), Some(player));
    let from_second = (Team::new(2), Position::from((20, 0, 0)), None);
    // Tick 0 ends at time 1. The first's train spawns first. The second's two trains of no time
    // spawn one after the other: each next starts at time 1 and is done at once. Its third, of
    // 1 tick, starts at time 1 and is done at time 2, the end of tick 1.
    game.step();
    assert_eq!(
        shop.spawns(&mut game),
        [from_first, from_second, from_second]
    );
    game.step();
    assert_eq!(
        shop.spawns(&mut game),
        [from_first, from_second, from_second, from_second]
    );

    // A train of no time ordered in tick 2 starts at time 2 and spawns at the end of tick 2.
    Production::order_train(&mut game, first);
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 5);
    assert_eq!(shop.spawns(&mut game)[4], from_first);

    // Dead through ticks 3 and 4, the first's queue holds a train done since time 0: it spawns in
    // tick 5, the tick the producer lives again.
    let entity = game.unit_entity(first);
    game.insert_parts(first, (queue(train, &[0]), Dead));
    game.step();
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 5);
    game.entity_mut(entity).remove::<Dead>();
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 6);
    assert_eq!(shop.spawns(&mut game)[5], from_first);
}

#[test]
fn a_producer_no_player_owns_affords_no_train_that_costs_a_resource() {
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let gold = ResourceId::nth(0);
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
    assert_eq!(
        shop.spawns(&mut game),
        [(Team::new(1), Position::from((10, 0, 0)), Some(player))]
    );
    let held = PlayerResources::of_player(&game, player)
        .unwrap()
        .amount(gold);
    assert_eq!(held, 3);
    for id in [owned, unowned] {
        let entity = game.unit_entity(id);
        assert_eq!(game.get::<ActionSlots>(entity).unwrap().in_progress(), None);
        assert_eq!(game.get::<TrainQueue>(entity), Some(&TrainQueue::default()));
    }

    // A predicting client trains as the server does, but pays nothing: its player's resources
    // are the server's, whose word comes back. Its player keeps all 5 gold.
    let mut client = Match::predicting(&[Capability::Production]);
    let shop = Shop::place(&mut client);
    Players::spawn(&mut client, 1, 1);
    client
        .player_resources_mut(player)
        .unwrap()
        .add(gold, 5)
        .unwrap();
    let train = Actions::add_train(&mut client, shop.grunt, Ticks::ZERO, Some(cost));
    let owned = Production::allocate_id(&mut client);
    let queue = TrainQueue::default();
    shop.producer(
        &mut client,
        owned,
        10,
        Team::new(1),
        Some(player),
        train,
        queue,
    );
    Production::order_train(&mut client, owned);
    client.step();
    assert_eq!(
        shop.spawns(&mut client),
        [(Team::new(1), Position::from((10, 0, 0)), Some(player))]
    );
    let held = PlayerResources::of_player(&client, player)
        .unwrap()
        .amount(gold);
    assert_eq!(held, 5);
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
    let gold = ResourceId::nth(0);
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
mod gatherer;
mod gathering;
mod orders;
mod supply;
