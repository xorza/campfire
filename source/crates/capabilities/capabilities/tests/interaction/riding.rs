use std::num::NonZeroI64;
use std::sync::Arc;

use crate::halves;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::system::RunSystemOnce;
use campfire_abilities::{Abilities, CastData};
use campfire_actions::{
    ActionData, ActionKeys, ActionRange, ActionSlots, Actions, RangeBonus, RangeField, SlotKind,
    SlotTimes, Targeting, TimedSlots,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Order, Release};
use campfire_codec::Toml;
use campfire_combat::internals::TestWeapon;
use campfire_combat::{BonusStats, Combat, Rate, WeaponDamage};
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_declarations::ById;
use campfire_declarations::ByIdBuilder;
use campfire_declarations::internals::BookLoad;
use campfire_geometry::{Bounds, Grid};
use campfire_interaction::{
    EnterActionData, EnterSpec, FireSpec, Interaction, RiderCount, RiderPayload, RiderSize, Riders,
    Riding, UseSlot, Using,
};
use campfire_math::Num;
use campfire_navigation::{FlightData, Navigation, Walker};
use campfire_orders::Orders;
use campfire_orders::OrdersDirective;
use campfire_players::Players;
use campfire_script::ScriptFailures;
use campfire_script::internals::FailedCall;
use campfire_script::{Filter, Filters};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, MatchRate, Position, SimWorld, StableId};
use campfire_stats::ModifierData;
use campfire_stats::Stats;
use campfire_units::internals::Spawn;
use campfire_units::{
    BodyForm, BodyShape, Dead, Destination, FiringRider, GroundPoint, Layer, Leaving, Life,
    Lifespan, MapBook, MoveStep, Owner, RiderFate, RidersCalledOut, SeeingRider, SeenBy, StatId,
    StatusTags, UnitTypeCore, Units,
};
use campfire_values::{
    FilterData, ModifierId, Number, PackageIndex, Rank, Ranked, Scalar, Share, Team, TeamSet,
    TypeScope, UnitType,
};

/// The kind of every rider's one use slot.
const USES: SlotKind = SlotKind::new(0);

/// The containers of a match of riders, each of 500 life that stays as it dies: a `humvee`,
/// which drives a meter a tick, takes 3 of its allies' riders by their sizes, lets them out 2
/// ticks apart, and deals each half its life as it falls; a `bunker`, which stands, takes 2
/// riders of anyone, one each, lets them out at once, takes its first rider's side, deals each
/// all its life as it falls, lets them out at 35 % of its own, and lets them see from it; and a
/// `crawler`, which comes
/// with 2 walking casters. Each is entered by `board`, which reaches 1 m and winds up 100 ms, 3
/// ticks at 30 a second. The riders are the types `walking caster`, of size 1, which walks a
/// meter a tick, and `caster`, of size 2, which stands; `combatant`, with a use slot and no size,
/// rides nothing.
#[derive(Debug, Clone, Copy)]
struct Rides {
    humvee: UnitType,
    bunker: UnitType,
    crawler: UnitType,
}

impl Rides {
    /// A match of `stats`, `combat`, `navigation`, `abilities`, `orders` and `interaction`,
    /// with its containers and riders loaded, its two players, and no unit.
    fn load() -> (Match, Self) {
        let (mut game, rides) = Self::books();
        Players::spawn(&mut game, 2, 1);
        (game, rides)
    }

    /// The match `load` gives, with no player: one a snapshot restores into.
    fn books() -> (Match, Self) {
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Abilities,
            Capability::Orders,
            Capability::Interaction,
        ];
        let mut game = Match::builder(&declared).players(2).build();
        Units::name_kinds(&mut game, &["true"], &["health", "mana", "rage"], &[]);
        Abilities::load_types(&mut game);
        Abilities::load_stats(&mut game);
        for (rider, size) in [("walking caster", 1), ("caster", 2)] {
            let rider = Units::type_named(&game, rider).unwrap();
            Units::give_type(&mut game, rider, UseSlot::new(USES));
            Units::give_type(&mut game, rider, RiderSize::new(size.try_into().unwrap()));
        }
        let combatant = Units::type_named(&game, "combatant").unwrap();
        Units::give_type(&mut game, combatant, UseSlot::new(USES));
        let data = board_data();
        let board = Actions::load(&mut game, PackageIndex::MODE, "board", &data, None, 1).unwrap();
        Actions::load_effect_lists(&mut game, board, PackageIndex::MODE, &data);
        let walking = Units::type_named(&game, "walking caster").unwrap();
        let share = |billionths| Share::billionths(billionths).unwrap();
        let (allies, all) = (
            Filters::from([Filter::of_match(&game, "allies")]),
            Filters::from([Filter::of_match(&game, "all")]),
        );
        let container = |game: &mut Match, name: &str, spec: EnterSpec| {
            let core = UnitTypeCore::default();
            let container = Units::load_type(game, TypeScope::Mode, name, &core);
            Abilities::give_combatant(game, container);
            Units::give_type(game, container, spec);
            container
        };
        let humvee = container(
            &mut game,
            "humvee",
            EnterSpec {
                action: board,
                capacity: 3,
                counts: RiderCount::Size,
                riders: allies.clone(),
                exit: Ticks::new(2),
                damage: share(500_000_000),
                full: Share::NONE,
                capture: false,
                payload: Box::new([]),
                fire: FireSpec::None,
                sight: false,
                modifier: None,
                open: None,
                assault: Some(share(500_000_000)),
            },
        );
        Units::give_type(&mut game, humvee, MoveStep::new(Num::ONE).unwrap());
        let bunker = container(
            &mut game,
            "bunker",
            EnterSpec {
                action: board,
                capacity: 2,
                counts: RiderCount::One,
                riders: all,
                exit: Ticks::new(0),
                damage: Share::ALL,
                full: share(350_000_000),
                capture: true,
                payload: Box::new([]),
                fire: FireSpec::None,
                sight: true,
                modifier: None,
                open: None,
                assault: None,
            },
        );
        let crawler = container(
            &mut game,
            "crawler",
            EnterSpec {
                action: board,
                capacity: 4,
                counts: RiderCount::Size,
                riders: allies,
                exit: Ticks::new(1),
                damage: share(100_000_000),
                full: Share::NONE,
                capture: false,
                payload: Box::new([RiderPayload {
                    unit_type: walking,
                    count: 2,
                }]),
                fire: FireSpec::None,
                sight: false,
                modifier: None,
                open: None,
                assault: None,
            },
        );
        let rides = Self {
            humvee,
            bunker,
            crawler,
        };
        (game, rides)
    }
}

/// An entry that reaches 1 m and winds up 100 ms; it aims at any unit.
fn board_data() -> ActionData {
    let unit = || Targeting::Unit(FilterData::parse("all").unwrap());
    ActionData {
        keys: ActionKeys {
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::ONE,
            )))),
            windup_ms: Some(Ranked::One(Number::Value(Scalar::Int(100)))),
            ..ActionData::of(unit(), CastData::default()).keys
        },
        section: Arc::new(EnterActionData::default()),
        ..ActionData::of(unit(), CastData::default())
    }
}

/// Player `player`'s rider of the type `name` at `x` on team `team`, with its use slot empty.
fn rider(game: &mut Match, name: &str, x: i16, player: u32, team: u8) -> StableId {
    let slots = TimedSlots::new(ActionSlots::new([]).with_empty(USES, 1));
    let parts = (Owner::new(PlayerSlot::new(player)), slots);
    Spawn::named(game, name, (x, 0, 0))
        .team(Team::new(team))
        .parts(parts)
        .place(game)
}

/// A container of `container` at `x` of team `team`, of player `player`'s or of none.
fn container(
    game: &mut Match,
    container: UnitType,
    x: i16,
    team: u8,
    player: Option<u32>,
) -> StableId {
    let spawn = Spawn::of(container, (x, 0, 0)).team(Team::new(team));
    match player {
        Some(player) => spawn.parts(Owner::new(PlayerSlot::new(player))).place(game),
        None => spawn.place(game),
    }
}

/// Runs a tick in which player `player` orders `unit` `directive`.
fn order(game: &mut Match, player: u32, unit: StableId, directive: OrdersDirective) {
    let payload = Order::payload(&[Order::one(unit, directive)]);
    game.step_with(&[(PlayerSlot::new(player), &payload)]);
}

/// The riders `container` holds, in their order.
fn riders(game: &Match, container: StableId) -> Vec<StableId> {
    let riders = game.part::<Riders>(container);
    riders.iter().map(|rider| rider.id).collect()
}

/// Where `unit` stands on the ground, in halves of a meter.
fn ground(game: &Match, unit: StableId) -> (i64, i64) {
    let at = game.part::<Position>(unit).get();
    let half = |value: Num| (value * Num::int(2)).floor();
    (half(at.x), half(at.z))
}

fn failed(game: &mut Match) -> Vec<FailedCall> {
    game.resource_mut::<ScriptFailures>().take_calls()
}

/// Whether `container` holds the `holding` tag.
fn holding(game: &Match, container: StableId) -> bool {
    game.part::<StatusTags>(container)
        .get()
        .contains(Interaction::HOLDING.tag())
}

/// Whether `unit` is out of the world: no query that does not name `Disabled` finds it.
fn is_out(game: &mut Match, unit: StableId) -> bool {
    let entity = game.unit_entity(unit);
    let mut found = game.query::<Entity>();
    found.get(game, entity).is_err() && game.entity(entity).contains::<Disabled>()
}

/// Enters `rider`, of player `player`, the container `container`, standing in reach: ordered in
/// the tick the match runs next, its entry winds up 3 ticks and resolves in the third after.
fn enter(game: &mut Match, player: u32, rider: StableId, container: StableId) {
    let now = game.now().get();
    order(
        game,
        player,
        rider,
        OrdersDirective::Enter { target: container },
    );
    game.run_until(now + 4);
}

#[test]
fn a_rider_in_reach_enters_out_of_the_world_and_keeps_its_parts() {
    let (mut game, rides) = Rides::load();
    let humvee = container(&mut game, rides.humvee, 0, 0, Some(0));
    let rider = rider(&mut game, "walking caster", 3, 0, 0);
    // Ordered in tick 0 from 3 m, it walks a meter a tick, in reach of 1 m by the end of tick 1,
    // and its entry winds up from tick 2 for 3 ticks, to resolve in tick 5.
    order(
        &mut game,
        0,
        rider,
        OrdersDirective::Enter { target: humvee },
    );
    game.run_until(5);
    assert_eq!(riders(&game, humvee), []);
    assert!(!holding(&game, humvee));
    game.step();
    assert_eq!(riders(&game, humvee), [rider]);
    assert_eq!(game.part::<Riding>(rider).container(), humvee);
    assert!(holding(&game, humvee));
    // Out of the world, its team alone sees it, and it keeps its id, its life and its slots.
    assert!(is_out(&mut game, rider));
    let team = TeamSet::NONE.with(Team::new(0));
    assert_eq!(*game.part::<SeenBy>(rider), SeenBy::new(team));
    assert_eq!(Stats::health(&game, rider), 500);
    assert_eq!(game.part::<ActionSlots>(rider).iter().count(), 1);
    // An order to it is dropped, as no unit takes it: it walks nowhere.
    let to = GroundPoint::new(Num::int(9), Num::ZERO);
    order(&mut game, 0, rider, OrdersDirective::Move { to });
    assert_eq!(game.part::<Destination>(rider).get(), None);
    // It stands where its humvee does, which drives 3 m in 3 ticks.
    let to = GroundPoint::new(Num::int(3), Num::ZERO);
    order(&mut game, 0, humvee, OrdersDirective::Move { to });
    game.steps(3);
    assert_eq!(ground(&game, humvee), (6, 0));
    assert_eq!(ground(&game, rider), (6, 0));
    // A save and a restore keep it inside.
    let (mut fresh, _) = Rides::books();
    Release::restore_into(&game, &mut fresh);
    assert_eq!(riders(&fresh, humvee), [rider]);
    assert!(is_out(&mut fresh, rider));
    assert_eq!(failed(&mut game), []);
}

#[test]
fn a_container_takes_no_rider_past_its_room_its_filter_its_size_its_side_or_its_parts() {
    let (mut game, rides) = Rides::load();
    let humvee = container(&mut game, rides.humvee, 0, 0, Some(0));
    // A walking caster of size 1 and a caster of size 2 fill its room of 3; one more is refused,
    // as is an enemy its filter of allies refuses, a combatant of no size, and an ally of another
    // player, whose side it is not.
    let first = rider(&mut game, "walking caster", 1, 0, 0);
    let second = rider(&mut game, "caster", -1, 0, 0);
    let third = rider(&mut game, "walking caster", 0, 0, 0);
    enter(&mut game, 0, first, humvee);
    enter(&mut game, 0, second, humvee);
    enter(&mut game, 0, third, humvee);
    assert_eq!(riders(&game, humvee), [first, second]);
    let other = container(&mut game, rides.humvee, 20, 0, Some(0));
    let enemy = rider(&mut game, "walking caster", 21, 0, 1);
    let sizeless = rider(&mut game, "combatant", 19, 0, 0);
    let ally = rider(&mut game, "walking caster", 20, 1, 0);
    for (player, unit) in [(0, enemy), (0, sizeless), (1, ally)] {
        enter(&mut game, player, unit, other);
    }
    assert_eq!(riders(&game, other), []);
    // A unit that holds its slots but not their times, as a client holds one whose times have
    // not come, finds no entry, so its order is dropped.
    let untimed = rider(&mut game, "walking caster", 21, 0, 0);
    let entity = game.unit_entity(untimed);
    game.entity_mut(entity).remove::<SlotTimes>();
    enter(&mut game, 0, untimed, other);
    assert_eq!(riders(&game, other), []);
    let own = rider(&mut game, "walking caster", 21, 0, 0);
    enter(&mut game, 0, own, other);
    assert_eq!(riders(&game, other), [own]);
    assert_eq!(failed(&mut game), []);
}

#[test]
fn an_entry_opens_only_while_its_container_holds_the_tags_it_names() {
    // A humvee open while it holds no rider, as its tag `holding` says: the first rider enters,
    // and the second, ordered once the first is in, finds it closed (exits-docks-and-selection.md,
    // A bunker is an entry a tag opens).
    let (mut game, rides) = Rides::books();
    let mut spec = game
        .resource::<ById<UnitType, EnterSpec>>()
        .get(rides.humvee)
        .unwrap()
        .clone();
    spec.open = Some(Filter::of_match(&game, "all:!holding"));
    let core = UnitTypeCore::default();
    let gated = Units::load_type(&mut game, TypeScope::Mode, "gated humvee", &core);
    Abilities::give_combatant(&mut game, gated);
    Units::give_type(&mut game, gated, spec);
    Players::spawn(&mut game, 2, 1);
    let humvee = container(&mut game, gated, 0, 0, Some(0));
    let [first, second] = [1, -1].map(|x| rider(&mut game, "walking caster", x, 0, 0));
    enter(&mut game, 0, first, humvee);
    assert_eq!(riders(&game, humvee), [first]);
    enter(&mut game, 0, second, humvee);
    assert_eq!(riders(&game, humvee), [first]);
    assert_eq!(failed(&mut game), []);
}

#[test]
fn a_free_building_takes_its_first_riders_side_until_it_empties_and_none_at_its_full_share() {
    let (mut game, rides) = Rides::load();
    // A bunker of no player, on team 2, takes player 0's rider and its side, then a second of
    // player 0, and refuses player 1's.
    let bunker = container(&mut game, rides.bunker, 0, 2, None);
    let first = rider(&mut game, "walking caster", 1, 0, 0);
    let second = rider(&mut game, "walking caster", -1, 0, 0);
    let foe = rider(&mut game, "walking caster", 0, 1, 1);
    // An order checks the side as the entry does: the free bunker takes either player's rider,
    // and once player 0's first rider holds it, player 0's alone.
    let entry = |game: &mut Match, rider: StableId| {
        let entity = game.unit_entity(rider);
        let read = move |using: Using<'_, '_>| using.entry(entity, bunker).is_some();
        game.run_system_once(read).unwrap()
    };
    assert!(entry(&mut game, first) && entry(&mut game, foe));
    enter(&mut game, 0, first, bunker);
    assert!(entry(&mut game, second) && !entry(&mut game, foe));
    let side = |game: &Match| {
        let owner = game
            .try_part::<Owner>(bunker)
            .map(|owner| owner.player_slot().get());
        (owner, game.part::<Team>(bunker).get())
    };
    assert_eq!(side(&game), (Some(0), 0));
    // Its rider sees from it, as a bunker's sight says.
    assert!(game.try_part::<SeeingRider>(first).is_some());
    enter(&mut game, 1, foe, bunker);
    enter(&mut game, 0, second, bunker);
    assert_eq!(riders(&game, bunker), [first, second]);
    // Its riders leave at once, of its exit of no time, and it goes back to no player.
    order(&mut game, 0, bunker, OrdersDirective::Evacuate);
    assert_eq!(riders(&game, bunker), []);
    assert_eq!(side(&game), (None, 2));
    assert!(!holding(&game, bunker));
    assert!(game.try_part::<SeeingRider>(first).is_none());
    // At its `full` share, 35 % of 500, it takes none.
    Combat::queue_damage(&mut game, None, bunker, Num::int(325), "true");
    game.step();
    assert_eq!(Stats::health(&game, bunker), 175);
    enter(&mut game, 1, foe, bunker);
    assert_eq!(riders(&game, bunker), []);
    assert_eq!(failed(&mut game), []);
}

/// Gives `game` a pathing grid of 1 m cells from (−12, −12) to (12, 12), which the riders walk
/// as circles of a half meter, and the bunker, which stands, a box 2 m wide.
fn grid(game: &mut Match, rides: Rides) {
    let bounds = Bounds::new([Num::int(-12); 2], [Num::int(12); 2]).unwrap();
    let walker = Walker {
        layer: Layer::FIRST,
        radius: Num::HALF,
    };
    MapBook::amend(game, |_, map| map.bounds = bounds);
    Navigation::load_pathing(game, Grid::new(Num::ONE, bounds).unwrap(), [walker]);
    let mut walkers = ByIdBuilder::default();
    for name in ["walking caster", "caster"] {
        walkers.set(Units::type_named(game, name).unwrap(), walker);
    }
    walkers.build().load_into(game);
    let box_body = BodyForm::new(
        BodyShape::box_sized([Num::int(2), Num::int(2)]).unwrap(),
        Layer::FIRST,
    );
    Units::give_type(game, rides.bunker, box_body);
}

/// Where each of `units` stands out of its container, in halves of a meter; none inside.
fn outside(game: &Match, units: &[StableId]) -> Vec<Option<(i64, i64)>> {
    let rides = |unit| game.try_part::<Riding>(unit).is_some();
    let out = |&unit: &StableId| (!rides(unit)).then(|| ground(game, unit));
    units.iter().map(out).collect()
}

#[test]
fn riders_leave_each_exit_time_apart_at_the_free_cell_nearest_their_container() {
    let (mut game, rides) = Rides::load();
    grid(&mut game, rides);
    let humvee = container(&mut game, rides.humvee, 0, 0, Some(0));
    let units: Vec<StableId> = [1, -1, 0]
        .into_iter()
        .map(|x| rider(&mut game, "walking caster", x, 0, 0))
        .collect();
    for &unit in &units {
        enter(&mut game, 0, unit, humvee);
    }
    assert_eq!(riders(&game, humvee), units);
    // The second leaves in the tick of its exit, 12, at the center of the free cell nearest the
    // humvee, (−0.5, −0.5), of the four as near the lower first; the evacuation of tick 13 lets
    // the first out in tick 14, the exit time of 2 ticks after the second, and the third in 16.
    assert_eq!(game.now().get(), 12);
    order(
        &mut game,
        0,
        humvee,
        OrdersDirective::Exit { rider: units[1] },
    );
    let cell = Some((-1, -1));
    assert_eq!(outside(&game, &units), [None, cell, None]);
    order(&mut game, 0, humvee, OrdersDirective::Evacuate);
    assert_eq!(outside(&game, &units), [None, cell, None]);
    game.step();
    assert_eq!(outside(&game, &units), [cell, cell, None]);
    game.step();
    assert_eq!(outside(&game, &units), [cell, cell, None]);
    game.step();
    assert_eq!(outside(&game, &units), [cell, cell, cell]);
    assert!(!holding(&game, humvee));
    // Out, each is in the world again, and the last, out this tick, leaves through its team's
    // units.
    for &unit in &units {
        assert!(!is_out(&mut game, unit));
    }
    assert!(game.part::<Leaving>(units[2]).get());
    // An exit of a unit that does not ride in it does nothing.
    order(
        &mut game,
        0,
        humvee,
        OrdersDirective::Exit { rider: units[0] },
    );
    assert_eq!(failed(&mut game), []);
}

#[test]
fn an_assault_transports_attack_lets_out_each_rider_above_its_share_of_life() {
    // A humvee, an assault transport of half a life, holds 2 riders; the first hurt to half its
    // life, 250 of 500, stays, and the second, whole, leaves as the humvee is ordered to
    // attack-move (exits-docks-and-selection.md, A transport may assault).
    let (mut game, rides) = Rides::load();
    let humvee = container(&mut game, rides.humvee, 0, 0, Some(0));
    let [hurt, whole] = [1, -1].map(|x| rider(&mut game, "walking caster", x, 0, 0));
    enter(&mut game, 0, hurt, humvee);
    enter(&mut game, 0, whole, humvee);
    assert_eq!(riders(&game, humvee), [hurt, whole]);
    let entity = game.unit_entity(hurt);
    game.get_mut::<Life>(entity).unwrap().take(Num::int(250));
    let to = GroundPoint::new(Num::int(9), Num::ZERO);
    order(&mut game, 0, humvee, OrdersDirective::AttackMove { to });
    game.step();
    assert_eq!(riders(&game, humvee), [hurt]);
    assert!(!is_out(&mut game, whole));
    assert_eq!(failed(&mut game), []);
}

#[test]
fn riders_called_out_leave_at_their_exit_times_or_go_with_their_container() {
    // A crawler's 2 riders, which it came with, called out to leave: they leave a tick apart, by
    // its exit time; another's, called to go, despawn with it (exits-docks-and-selection.md, A
    // sold container lets its riders out).
    let (mut game, rides) = Rides::load();
    grid(&mut game, rides);
    let crawler = container(&mut game, rides.crawler, 0, 0, Some(0));
    let carried = riders(&game, crawler);
    let entity = game.unit_entity(crawler);
    game.trigger(RidersCalledOut {
        entity,
        fate: RiderFate::Leave,
    });
    game.step();
    assert_eq!(riders(&game, crawler), [carried[1]]);
    game.step();
    assert_eq!(riders(&game, crawler), []);
    let other = container(&mut game, rides.crawler, 6, 0, Some(0));
    let gone: Vec<Entity> = riders(&game, other)
        .into_iter()
        .map(|rider| game.unit_entity(rider))
        .collect();
    assert_eq!(gone.len(), 2);
    let entity = game.unit_entity(other);
    game.trigger(RidersCalledOut {
        entity,
        fate: RiderFate::Go,
    });
    game.entity_mut(entity).despawn();
    assert!(gone.iter().all(|&rider| game.get_entity(rider).is_err()));
    assert_eq!(failed(&mut game), []);
}

#[test]
fn a_fall_lets_every_rider_out_dealt_its_containers_share_and_its_full_share_starts_their_exits() {
    let (mut game, rides) = Rides::load();
    grid(&mut game, rides);
    // A humvee that dies deals each rider half its life, 250; a bunker all of it, which kills
    // them; a crawler's 2 riders, which it came with, a tenth, 50.
    let humvee = container(&mut game, rides.humvee, -6, 0, Some(0));
    let bunker = container(&mut game, rides.bunker, 6, 0, Some(0));
    let crawler = container(&mut game, rides.crawler, 0, 0, Some(0));
    let carried = riders(&game, crawler);
    assert_eq!(carried.len(), 2);
    let in_humvee = rider(&mut game, "walking caster", -5, 0, 0);
    let in_bunker = rider(&mut game, "walking caster", 4, 0, 0);
    enter(&mut game, 0, in_humvee, humvee);
    enter(&mut game, 0, in_bunker, bunker);
    for container in [humvee, bunker, crawler] {
        Combat::queue_damage(&mut game, None, container, Num::int(500), "true");
    }
    game.step();
    let fallen = [in_humvee, in_bunker, carried[0], carried[1]];
    assert_eq!(
        outside(&game, &fallen),
        [
            Some((-13, -1)),
            Some((11, -3)),
            Some((-1, -1)),
            Some((-1, -1))
        ]
    );
    let lives = fallen.map(|unit| Stats::health(&game, unit));
    assert_eq!(lives, [250, 0, 450, 450]);
    game.step();
    assert!(game.try_part::<Dead>(in_bunker).is_some());
    // A living bunker whose life falls to its `full` share, 175, lets its riders out, at once,
    // its exit of no time.
    let standing = container(&mut game, rides.bunker, -10, 0, Some(0));
    let held = rider(&mut game, "walking caster", -8, 0, 0);
    enter(&mut game, 0, held, standing);
    assert_eq!(riders(&game, standing), [held]);
    Combat::queue_damage(&mut game, None, standing, Num::int(324), "true");
    game.step();
    assert_eq!(riders(&game, standing), [held]);
    Combat::queue_damage(&mut game, None, standing, Num::int(1), "true");
    game.step();
    assert_eq!(Stats::health(&game, standing), 175);
    assert_eq!(riders(&game, standing), []);
    assert_eq!(failed(&mut game), []);
}

#[test]
fn a_riders_timers_wait_inside_and_act_on_its_first_tick_out() {
    let (mut game, rides) = Rides::load();
    // A rider whose life ends with tick 10 enters in tick 3: its end comes inside, where nothing
    // visits it, and acts on its first tick out, tick 14, which its exit's Resolve starts.
    let humvee = container(&mut game, rides.humvee, 0, 0, Some(0));
    let rider = rider(&mut game, "walking caster", 1, 0, 0);
    game.insert_parts(rider, Lifespan::until(Tick::new(10)));
    enter(&mut game, 0, rider, humvee);
    game.run_until(14);
    assert_eq!(riders(&game, humvee), [rider]);
    order(&mut game, 0, humvee, OrdersDirective::Exit { rider });
    assert_eq!(game.entity_of(rider), None);
    assert_eq!(failed(&mut game), []);
}

#[test]
fn a_container_of_a_payload_comes_with_its_riders_inside() {
    let (mut game, rides) = Rides::load();
    let crawler = container(&mut game, rides.crawler, 0, 0, Some(0));
    let carried = riders(&game, crawler);
    assert_eq!(carried.len(), 2);
    for &rider in &carried {
        assert!(is_out(&mut game, rider));
        assert_eq!(game.part::<Riding>(rider).container(), crawler);
        assert_eq!(game.part::<Owner>(rider).player_slot().get(), 0);
        assert!(game.try_part::<SeeingRider>(rider).is_none());
    }
    assert!(holding(&game, crawler));
    // Its first tick runs with them inside.
    game.step();
    assert_eq!(riders(&game, crawler), carried);
    assert_eq!(failed(&mut game), []);
}

/// Arms the walking caster as a rider that fires: a weapon of 10 damage, an attack each 3 ticks,
/// that reaches 2 m and winds up none, its damage and its range each stretched by the stat
/// `attack_damage`, the third of the match's, and a scan each 100 ms of its enemies; and loads the
/// modifier `garrisoned`, which adds 0.5 of that stat. The weapon's slot, beside its use slot.
fn arm(game: &mut Match) -> (ActionSlots, ModifierId) {
    let bonus = StatId::new(2);
    let weapon = TestWeapon {
        rate: Rate::Period(Box::new([Ticks::new(3)])),
        damage: WeaponDamage::Amount(Box::new([Num::int(10)])),
        damage_bonus: Some(BonusStats::from([bonus])),
        range_bonus: Some(RangeBonus::from([bonus])),
        ..TestWeapon::new(
            Filter::of_match(game, "enemies"),
            ActionRange::meters(Num::int(2)),
            Ticks::ZERO,
        )
    };
    let weapon = Combat::add_weapon(game, weapon);
    let walking = Units::type_named(game, "walking caster").unwrap();
    Stats::give_type_stats(game, walking, &[Num::ZERO; 4]);
    Orders::load_acquire(game, walking, 100, &["enemies"]);
    let data = Toml::parse::<ModifierData>("stats = { attack_damage = { add = \"0.5\" } }");
    Stats::load_modifier(game, PackageIndex::MODE, "garrisoned", &data.unwrap(), None);
    let garrisoned = Stats::modifier(game, PackageIndex::MODE, "garrisoned").unwrap();
    let slots = ActionSlots::new([(weapon, SlotKind::new(1), Some(Rank::FIRST))]);
    (slots.with_empty(USES, 1), garrisoned)
}

/// Player 0's armed walking caster at `x` on team 0.
fn gunner(game: &mut Match, slots: &ActionSlots, x: i16) -> StableId {
    let parts = (
        Owner::new(PlayerSlot::new(0)),
        TimedSlots::new(slots.clone()),
    );
    Spawn::named(game, "walking caster", (x, 0, 0))
        .parts(parts)
        .place(game)
}

/// A combatant of team 1, of 500 life, at the point `(x, z)` in halves of a meter.
fn foe(game: &mut Match, x: i64, z: i64) -> StableId {
    Spawn::named(game, "combatant", halves(x, z))
        .team(Team::new(1))
        .place(game)
}

#[test]
fn a_rider_fires_from_its_container_as_its_fire_lets_it_with_the_containers_modifier() {
    let (mut game, rides) = Rides::load();
    let (slots, garrisoned) = arm(&mut game);
    // Humvees that let their riders fire from their place, the second with `garrisoned`; and a
    // third that lets none fire.
    let fires = |modifier| EnterSpec {
        fire: FireSpec::Center(Filter::of_match(&game, "all")),
        modifier,
        ..game
            .resource::<ById<UnitType, EnterSpec>>()
            .get(rides.humvee)
            .unwrap()
            .clone()
    };
    let [plain, held, closed] = [
        fires(None),
        fires(Some(garrisoned)),
        EnterSpec {
            fire: FireSpec::None,
            ..fires(None)
        },
    ];
    let types: Vec<UnitType> = [
        ("open humvee", plain),
        ("garrison humvee", held),
        ("closed humvee", closed),
    ]
    .into_iter()
    .map(|(name, spec)| {
        let core = UnitTypeCore::default();
        let humvee = Units::load_type(&mut game, TypeScope::Mode, name, &core);
        Abilities::give_combatant(&mut game, humvee);
        Units::give_type(&mut game, humvee, spec);
        Units::give_type(&mut game, humvee, MoveStep::new(Num::ONE).unwrap());
        humvee
    })
    .collect();
    // Each 20 m apart on x, each rider beside its humvee, and a foe 2.5 m from the open and the
    // garrison humvee on z, past the weapon's 2 m and within its 3 m stretched by `garrisoned`, and
    // 1.5 m from the closed one, within its reach.
    let mut placed: Vec<(StableId, StableId, StableId)> = Vec::new();
    for (at, (&humvee_type, z)) in types.iter().zip([5, 5, 3]).enumerate() {
        let x = i16::try_from(at).unwrap() * 20;
        let humvee = container(&mut game, humvee_type, x, 0, Some(0));
        let rider = gunner(&mut game, &slots, x + 1);
        enter(&mut game, 0, rider, humvee);
        let target = foe(&mut game, i64::from(x) * 2, z);
        placed.push((humvee, rider, target));
    }
    let lives = |game: &Match| -> Vec<i64> {
        placed
            .iter()
            .map(|&(.., target)| Stats::health(game, target))
            .collect()
    };
    // The garrison humvee's rider struck 5 times by now, each 10 × 1.5 = 15, from 2.5 m; the
    // open one's foe stands past its reach, and the closed one's rider fires at nothing.
    game.steps(12);
    assert_eq!(lives(&game), [500, 425, 500]);
    assert!(game.try_part::<FiringRider>(placed[2].1).is_none());
    // The open humvee drives a meter toward its foe, to 1.5 m off, its rider with it: the rider
    // strikes twice for 10 as it comes.
    let to = GroundPoint::new(Num::ZERO, Num::ONE);
    order(&mut game, 0, placed[0].0, OrdersDirective::Move { to });
    game.steps(6);
    assert_eq!(ground(&game, placed[0].1), (0, 2));
    assert_eq!(lives(&game), [480, 395, 500]);
    assert_eq!(failed(&mut game), []);
}

#[test]
fn a_garrisons_rider_fires_from_the_free_point_nearest_its_target() {
    let (mut game, rides) = Rides::load();
    let (slots, _) = arm(&mut game);
    // A bunker of two fire points, 1 m each side of it on x, which lets every rider fire.
    let spec = EnterSpec {
        fire: FireSpec::Points(Box::new([[Num::int(-1), Num::ZERO], [Num::ONE, Num::ZERO]])),
        ..game
            .resource::<ById<UnitType, EnterSpec>>()
            .get(rides.bunker)
            .unwrap()
            .clone()
    };
    let core = UnitTypeCore::default();
    let garrison = Units::load_type(&mut game, TypeScope::Mode, "garrison", &core);
    Abilities::give_combatant(&mut game, garrison);
    Units::give_type(&mut game, garrison, spec);
    let bunker = container(&mut game, garrison, 0, 0, Some(0));
    let rider = gunner(&mut game, &slots, 1);
    enter(&mut game, 0, rider, bunker);
    // It takes the first point, at (−1, 0).
    assert_eq!(ground(&game, rider), (-2, 0));
    // A foe at (0.5, 0), 1.5 m from it, in reach: its scan takes it, and it moves to the free
    // point nearest it, at (1, 0), from which it strikes.
    let target = foe(&mut game, 1, 0);
    game.steps(6);
    assert_eq!(ground(&game, rider), (2, 0));
    assert!(Stats::health(&game, target) < 500);
    assert_eq!(failed(&mut game), []);
}

#[test]
fn a_flier_lands_for_its_riders_and_climbs_once_none_waits() {
    let (mut game, rides) = Rides::load();
    // A chinook of the humvee's room, which flies at 8 m, climbing a meter a tick, and lets its
    // riders out a tick apart.
    let spec = EnterSpec {
        exit: Ticks::new(1),
        ..game
            .resource::<ById<UnitType, EnterSpec>>()
            .get(rides.humvee)
            .unwrap()
            .clone()
    };
    let core = UnitTypeCore::default();
    let chinook = Units::load_type(&mut game, TypeScope::Mode, "chinook", &core);
    Abilities::give_combatant(&mut game, chinook);
    Units::give_type(&mut game, chinook, spec);
    let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
    let flight = FlightData::new(Num::int(8), Num::int(hz), Num::int(3)).unwrap();
    Navigation::load_flight(&mut game, chinook, flight);
    let flier = container(&mut game, chinook, 0, 0, Some(0));
    // Its height and how many ride in it after each of `ticks` ticks from the next.
    let trace = |game: &mut Match, ticks: usize| -> Vec<(i64, usize)> {
        (0..ticks)
            .map(|_| {
                game.step();
                let height = game.part::<Position>(flier).get().y.to_int().unwrap();
                (height, riders(game, flier).len())
            })
            .collect()
    };
    game.steps(10);
    assert_eq!(game.part::<Position>(flier).get().y, Num::int(8));
    // A rider a meter off is ordered in: the chinook lands for it, a meter a tick, on the ground
    // after the order's tick and 7 more. The rider's entry, which winds up 3 ticks, found it in
    // the air, so it waits; once the chinook stands on the ground it enters again, from the next
    // tick's Act, and in 3 ticks. Then the chinook climbs back to 8 m, as none waits.
    let rider = rider(&mut game, "walking caster", 1, 0, 0);
    let payload = Order::payload(&[Order::one(rider, OrdersDirective::Enter { target: flier })]);
    game.push_input(PlayerSlot::new(0), &payload);
    let down = [
        (7, 0),
        (6, 0),
        (5, 0),
        (4, 0),
        (3, 0),
        (2, 0),
        (1, 0),
        (0, 0),
    ];
    let board = [(0, 0), (0, 0), (0, 0), (0, 0), (0, 1)];
    let up = [
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (8, 1),
    ];
    assert_eq!(trace(&mut game, 22), [&down[..], &board, &up].concat());
    // Its rider's exit, ordered in the air, lands it, and the rider leaves on the ground; then
    // it climbs again.
    let payload = Order::payload(&[Order::one(flier, OrdersDirective::Exit { rider })]);
    game.push_input(PlayerSlot::new(0), &payload);
    let down = [
        (7, 1),
        (6, 1),
        (5, 1),
        (4, 1),
        (3, 1),
        (2, 1),
        (1, 1),
        (0, 0),
    ];
    let up = [
        (1, 0),
        (2, 0),
        (3, 0),
        (4, 0),
        (5, 0),
        (6, 0),
        (7, 0),
        (8, 0),
        (8, 0),
    ];
    assert_eq!(trace(&mut game, 17), [&down[..], &up].concat());
    assert!(!is_out(&mut game, rider));
    assert_eq!(failed(&mut game), []);
}
