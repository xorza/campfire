use std::collections::BTreeMap;
use std::mem;
use std::num::NonZeroU8;

use bevy_ecs::world::World;
use campfire_actions::{ActionProgress, ActionSlots, SlotKind, TimedSlots};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_codec::Binary;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_declarations::Role;
use campfire_declarations::internals::BookLoad;
use campfire_declarations::{ById, PerId};
use campfire_math::{Num, Rounding, Vec3};
use campfire_players::{Players, PlayersWorld, ResourceAmount};
use campfire_production::*;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimComponent, SimTick, SimWorld, StableId, UnitDues, Unpredicted};
use campfire_units::internals::Spawn;
use campfire_units::{
    BodyForm, BodyShape, Dead, Destination, Layer, MoveStep, Owner, SpawnAt, SpawnPoint, Spawner,
    StatusTags, UnitTypeCore, Units, Variation, Variations,
};
use campfire_values::{ActionId, Rank, ResourceId, Team, TypeScope, UnitType};

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
    Match::builder(declared).role(Role::Rules).build()
}

/// A shop's match: a grunt type to train, a circle of 0.5 m that walks 1 m a tick; a producer
/// type of a queue of 3, and a gate, a box of 4 by 2 m that trains as the barracks does, out of
/// its exit 3 m along its frame's x, and to its default rally point 3 m back; a depot and a forge
/// type for supply and requirements to count; and the gold its players hold. The mode spawns each
/// type.
#[derive(Debug, Clone, Copy)]
struct Shop {
    grunt: UnitType,
    barracks: UnitType,
    gate: UnitType,
    depot: UnitType,
    forge: UnitType,
    gold: ResourceId,
}

impl Shop {
    /// Loads the shop's types into `game`, a match with production, as the mode's.
    fn place(game: &mut Match) -> Self {
        let data = UnitTypeCore::default();
        let grunt = Units::load_type(game, TypeScope::Mode, "grunt", &data);
        let barracks = Units::load_type(game, TypeScope::Mode, "barracks", &data);
        let depot = Units::load_type(game, TypeScope::Mode, "depot", &data);
        let forge = Units::load_type(game, TypeScope::Mode, "forge", &data);
        let gate = Units::load_type(game, TypeScope::Mode, "gate", &data);
        let production = ProducerSpec {
            queue: NonZeroU8::new(3).unwrap(),
            exit: None,
            rally: None,
            gathers: false,
            free: Box::new([]),
            places: Box::new([]),
            doors: None,
        };
        Self::give(game, barracks, production.clone());
        let frame = |x: i64| FramePoint::new([Num::int(x), Num::ZERO]);
        let gated = ProducerSpec {
            exit: frame(3),
            rally: frame(-3),
            ..production
        };
        Self::give(game, gate, gated);
        Self::give(
            game,
            gate,
            BodyForm::new(
                BodyShape::box_sized([Num::int(4), Num::int(2)]).unwrap(),
                Layer::FIRST,
            ),
        );
        Self::give(game, grunt, MoveStep::new(Num::ONE).unwrap());
        Self::give(
            game,
            grunt,
            BodyForm::new(BodyShape::circle(Num::HALF).unwrap(), Layer::FIRST),
        );
        let gold = ResourceId::new(0);
        Self {
            grunt,
            barracks,
            gate,
            depot,
            forge,
            gold,
        }
    }

    /// Gives `unit_type` `value` in its book, as the books of a mode's type do.
    fn give<T: PerId<UnitType>>(game: &mut World, unit_type: UnitType, value: T) {
        ById::<UnitType, T>::amend(game, |_, book| book.set(unit_type, value));
    }

    /// Gives `unit_type` `value` in its book in place of the one it has.
    fn regive<T: PerId<UnitType>>(game: &mut World, unit_type: UnitType, value: T) {
        ById::<UnitType, T>::amend(game, |_, book| {
            let mut built = mem::take(book).reopen();
            built.take(unit_type);
            built.set(unit_type, value);
            *book = built.build();
        });
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
                position: point.get(),
                owner: owner.map(|owner| owner.player_slot()),
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
            position: Vec3::new(Num::int(x), Num::ZERO, Num::ZERO),
            owner,
            angle: Num::ZERO,
            variation: None,
        };
        Spawner::place_any(game, at, (slots, queue));
    }

    /// Each grunt trained so far, in order: the team and position of the producer it came from,
    /// and its owner.
    fn spawns(self, game: &mut World) -> Vec<(Team, Vec3, Option<PlayerSlot>)> {
        let spawned = Self::spawned(game, self.grunt);
        spawned
            .iter()
            .map(|spawn| (spawn.team, spawn.position, spawn.owner))
            .collect()
    }

    /// Player 0's gold.
    fn gold(self, game: &World) -> i64 {
        game.player_resources(PlayerSlot::new(0))
            .unwrap()
            .amount(self.gold)
    }

    /// The depots spawned so far, by stable id.
    fn depots(self, game: &mut World) -> Vec<StableId> {
        let spawned = Self::spawned(game, self.depot);
        spawned.iter().map(|spawn| spawn.id).collect()
    }

    /// Player 0's hall at `position`.
    fn hall(self, game: &mut World, position: impl Into<Vec3>) -> StableId {
        let position = position.into();
        let body = BodyForm::new(
            BodyShape::box_sized([Num::int(4), Num::int(2)]).unwrap(),
            Layer::FIRST,
        )
        .at(Num::ZERO);
        let parts = (Owner::new(PlayerSlot::new(0)), body);
        Spawn::of(self.depot, position).parts(parts).place(game)
    }
}

/// A unit as it spawned: its stable id, team and place, and the player that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spawned {
    id: StableId,
    team: Team,
    position: Vec3,
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
        queue.push(queued, &[], Tick::new(0), &mut UnitDues::loose());
    }
    queue
}

#[test]
fn trains_spawn_by_their_producers_stable_id_and_a_dead_producers_queue_waits() {
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    // Two variations of the grunt's own body, so a train draws one for each unit and moves none.
    let half = BodyForm::new(BodyShape::circle(Num::HALF).unwrap(), Layer::FIRST);
    Shop::give(&mut game, shop.grunt, Variations::from([half, half]));
    let train = Production::add_train(&mut game, shop.grunt, Ticks::ZERO, None);
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
    let from_first = (Team::new(1), Vec3::from((10, 0, 0)), Some(player));
    let from_second = (Team::new(2), Vec3::from((20, 0, 0)), None);
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
    // Each trained grunt holds the variation its spawn drew on the stream of its own id.
    let trained = Shop::spawned(&mut game, shop.grunt);
    let drawn = trained
        .iter()
        .map(|spawned| game.part::<Variation>(spawned.id).get());
    assert_eq!(drawn.collect::<Vec<u8>>(), [0, 1, 1, 0, 1, 0]);
}

#[test]
fn a_producer_no_player_owns_affords_no_train_that_costs_a_resource() {
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let gold = ResourceId::new(0);
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
    let train = Production::add_train(&mut game, shop.grunt, Ticks::ZERO, Some(cost));
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
        [(Team::new(1), Vec3::from((10, 0, 0)), Some(player))]
    );
    let held = game.player_resources(player).unwrap().amount(gold);
    assert_eq!(held, 3);
    for id in [owned, unowned] {
        let entity = game.unit_entity(id);
        assert_eq!(
            game.get::<ActionProgress>(entity).unwrap().in_progress(),
            None
        );
        assert_eq!(game.get::<TrainQueue>(entity), Some(&TrainQueue::default()));
    }

    // A predicting client queues the train as the server does, but pays nothing and spawns
    // nothing: its player's resources and the unit trained are the server's, whose word comes
    // back. Its player keeps all 5 gold, and its queue the train, done.
    let mut client = Match::builder(&[Capability::Production])
        .role(Role::Client)
        .build();
    let shop = Shop::place(&mut client);
    Players::spawn(&mut client, 1, 1);
    client
        .player_resources_mut(player)
        .unwrap()
        .add(gold, 5)
        .unwrap();
    let train = Production::add_train(&mut client, shop.grunt, Ticks::ZERO, Some(cost));
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
    assert_eq!(shop.spawns(&mut client), []);
    let held = client.player_resources(player).unwrap().amount(gold);
    assert_eq!(held, 5);
    // The client's world holds no spawner, as a client's holds none: its fixtures' is their own,
    // so a system that spawned here would have failed this test.
    assert!(client.get_non_send::<Spawner>().is_none());
    let queue = client.part::<TrainQueue>(owned);
    assert_eq!(
        queue
            .entries()
            .iter()
            .map(|queued| queued.action)
            .collect::<Vec<_>>(),
        [train]
    );
}

#[test]
fn a_producer_whose_tags_block_production_neither_pays_nor_queues() {
    let mut game = shop_match_of(&[Capability::Stats, Capability::Production]);
    let shop = Shop::place(&mut game);
    // The engine tags' properties come with the sealed types, as a match's load gives them.
    Units::seal_types(&mut game, &BTreeMap::new());
    let gold = ResourceId::new(0);
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
    let train = Production::add_train(&mut game, shop.grunt, Ticks::ZERO, Some(cost));
    let (site, built) = (
        Production::allocate_id(&mut game),
        Production::allocate_id(&mut game),
    );
    for (id, x) in [(site, 10), (built, 20)] {
        let queue = TrainQueue::default();
        shop.producer(&mut game, id, x, Team::new(1), Some(player), train, queue);
    }
    // A site under construction holds the `constructing` tag, which blocks production: its order
    // ends, and it pays and queues nothing. The built producer pays 2 of the 5 gold and trains.
    game.insert_parts(site, StatusTags::from([Production::CONSTRUCTING]));
    game.step();
    Production::order_train(&mut game, site);
    Production::order_train(&mut game, built);
    game.step();
    assert_eq!(
        shop.spawns(&mut game),
        [(Team::new(1), Vec3::from((20, 0, 0)), Some(player))]
    );
    assert_eq!(game.player_resources(player).unwrap().amount(gold), 3);
    let entity = game.unit_entity(site);
    assert_eq!(
        game.get::<ActionProgress>(entity).unwrap().in_progress(),
        None
    );
    assert_eq!(game.get::<TrainQueue>(entity), Some(&TrainQueue::default()));
}

#[test]
fn a_train_queue_is_state_and_restores() {
    // A queue of two trains, the first ordered at tick 0 to be done at tick 3.
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let train = Production::add_train(&mut game, shop.grunt, Ticks::new(3), None);
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
    Production::add_train(&mut restored, restored_shop.grunt, Ticks::new(3), None);
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
        queue.push(queued, &[], Tick::new(pushed), &mut UnitDues::loose());
        Binary::decodes_again(&queue) && queue.check(&restored, entity)
    };
    assert!(check(0, Tick::LIMIT.get()));
    assert!(!check(1, Tick::LIMIT.get()) && !check(0, Tick::LIMIT.get() + 1));
    // A head started before the tick the match runs next ends at most its time after it: one of
    // 3 ticks pushed in that tick holds; pushed a tick later, it would wait past what any train
    // waits, and fails.
    let now = restored.resource::<SimTick>().start().get();
    assert!(check(now, 3) && !check(now + 1, 3));
    assert_eq!(restored.part::<TrainQueue>(id), game.part::<TrainQueue>(id));
}

#[test]
fn a_producers_door_opens_before_its_unit_leaves_and_closes_after_its_open_time() {
    // The barracks of one door that opens in 2 ticks, stays open 3 past its last unit and closes in
    // 2, with a train of no time and one of a tick (defenses-and-airfields.md, A producer's door
    // opens before its unit leaves).
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let times = DoorTimes {
        count: 1,
        opening: Ticks::new(2),
        open: Ticks::new(3),
        closing: Ticks::new(2),
    };
    let doored = |game: &mut Match| {
        let mut spec = game
            .resource::<ById<UnitType, ProducerSpec>>()
            .get(shop.barracks)
            .unwrap()
            .clone();
        spec.doors = Some(times);
        Shop::regive(game, shop.barracks, spec);
    };
    doored(&mut game);
    let train = Production::add_train(&mut game, shop.grunt, Ticks::ZERO, None);
    let id = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        id,
        10,
        Team::new(1),
        None,
        train,
        queue(train, &[0, 1]),
    );
    let door = |game: &Match| game.part::<Doors>(id).get(0);
    assert_eq!(door(&game), None);
    // Done at time 1, the end of tick 0, the first finds its door closed: it begins to open, and
    // the unit waits until it opened, at time 3, then leaves, and the door stays open from then.
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 0);
    let opening = OpenedDoor {
        open_at: Tick::new(3),
        left: Tick::new(3),
        held: false,
    };
    assert_eq!(door(&game), Some(opening));
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 0);
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 1);
    // The second, which started at time 3, is done at time 4 and leaves through the open door,
    // which stays open 3 ticks from then: open through time 6, closing at 7 and 8, closed from 9.
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 2);
    let passed = OpenedDoor {
        open_at: Tick::new(3),
        left: Tick::new(4),
        held: false,
    };
    assert_eq!(door(&game), Some(passed));
    assert_eq!(
        [1, 2, 3, 6, 7, 8, 9].map(|at| passed.phase(Tick::new(at), times)),
        [
            DoorPhase::Opening,
            DoorPhase::Opening,
            DoorPhase::Open,
            DoorPhase::Open,
            DoorPhase::Closing,
            DoorPhase::Closing,
            DoorPhase::Closed,
        ]
    );
    // Its phase holds from time 9, as it closed, and from its opening's end while held open.
    assert_eq!(
        [6, 8, 9].map(|at| passed.settled(Tick::new(at), times)),
        [false, false, true]
    );
    let held = OpenedDoor {
        held: true,
        ..passed
    };
    assert_eq!(
        [2, 3, 9].map(|at| (
            held.phase(Tick::new(at), times),
            held.settled(Tick::new(at), times)
        )),
        [
            (DoorPhase::Opening, false),
            (DoorPhase::Open, true),
            (DoorPhase::Open, true)
        ]
    );
    // The door is state: a restore of the same books holds it, and a door count other than its
    // type's, or a unit that left before its door opened, fails the check.
    let mut restored = shop_match();
    let restored_shop = Shop::place(&mut restored);
    doored(&mut restored);
    Production::add_train(&mut restored, restored_shop.grunt, Ticks::ZERO, None);
    Release::restore_into(&game, &mut restored);
    assert_eq!(restored.part::<Doors>(id), game.part::<Doors>(id));
    let entity = restored.unit_entity(id);
    assert!(!Doors::of([None, None]).check(&restored, entity));
    assert!(Doors::of([None]).check(&restored, entity));
    let early = OpenedDoor {
        open_at: Tick::new(3),
        left: Tick::new(2),
        held: false,
    };
    assert!(!Doors::of([Some(early)]).check(&restored, entity));
    // Closed from time 9, the door begins to open again for a third train ordered in tick 9, done
    // at its end, time 10: its unit leaves at time 12.
    for _ in 0..5 {
        game.step();
    }
    assert_eq!(game.resource::<SimTick>().start(), Tick::new(9));
    Production::order_train(&mut game, id);
    game.step();
    assert_eq!(
        door(&game),
        Some(OpenedDoor {
            open_at: Tick::new(12),
            left: Tick::new(12),
            held: false,
        })
    );
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 2);
    game.step();
    assert_eq!(shop.spawns(&mut game).len(), 3);
}

mod construction;
mod gathering;
mod orders;
mod places;
mod research;
mod supply;
