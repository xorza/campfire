use std::sync::Arc;

use campfire_abilities::{Abilities, CastData};
use campfire_actions::{
    ActionData, ActionKeys, ActionProgress, ActionRange, ActionSlots, ActionTarget, Actions,
    ChannelData, RangeField, SlotKind, SlotTimes, Targeting, TimedSlots,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Order, Release};
use campfire_combat::Combat;
use campfire_common::{PlayerSlot, Tick};
use campfire_interaction::{UseActionData, UseSlot, UseSpec};
use campfire_math::Num;
use campfire_names::Filter;
use campfire_orders::OrdersDirective;
use campfire_players::Players;
use campfire_script::ScriptFailures;
use campfire_script::internals::FailedCall;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimComponent, SimWorld, SnapshotError, StableId};
use campfire_stats::Stats;
use campfire_units::internals::Spawn;
use campfire_units::{Owner, UnitTypeCore, Units};
use campfire_values::{
    ActionId, Block, FilterData, Number, PackageIndex, PackagePath, Ranked, Scalar, Team,
};
use campfire_values::{TypeScope, UnitType};

/// The kind of every user's one use slot.
const USES: SlotKind = SlotKind::new(0);

/// Each use's hooks: its resolve deals its object 1, each tick of its channel 10, and a cut 100.
const USE_SCRIPT: &str = r#"
fn on_resolve(ctx, user, target) {
    ctx.damage(target, "true", 1);
}
fn on_channel_tick(ctx, user, target) {
    ctx.damage(target, "true", 10);
}
fn on_interrupt(ctx, user, target) {
    ctx.damage(target, "true", 100);
}
"#;

/// The objects of a match of uses, each a type of 500 life: a shrine, whose prayer winds up
/// 100 ms, 3 ticks at 30 a second, and channels 300 ms, ticking each 100 ms, which damage cuts;
/// a well, whose draw is the same but damage does not cut it; and an altar, whose offering has no
/// channel and which only allies use. Each use reaches 2 m. The users are the types `walking
/// caster`, which walks 1 m a tick, and `caster`, which stands, each with a use slot.
#[derive(Debug, Clone, Copy)]
struct Objects {
    shrine: UnitType,
    well: UnitType,
    altar: UnitType,
    pray: ActionId,
    draw: ActionId,
}

impl Objects {
    /// A match of `stats`, `combat`, `navigation`, `abilities`, `orders` and `interaction`,
    /// with its objects and users loaded, its two players, and no unit.
    fn load() -> (Match, Self) {
        let (mut game, objects) = Self::books();
        Players::spawn(&mut game, 2, 1);
        (game, objects)
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
        for user in ["walking caster", "caster"] {
            let user = Units::type_named(&game, user).unwrap();
            Units::give_type(&mut game, user, UseSlot::new(USES));
        }
        let script = Units::compile_hooked(&mut game, USE_SCRIPT).unwrap();
        let mut load = |name: &str, channel: Option<bool>| {
            let data = use_data(channel);
            Actions::load(&mut game, PackageIndex::MODE, name, &data, Some(script), 1).unwrap()
        };
        let (pray, draw, offer) = (
            load("pray", Some(true)),
            load("draw", Some(false)),
            load("offer", None),
        );
        let mut object = |name: &str, action: ActionId, users: &str| {
            let core = UnitTypeCore::default();
            let object = Units::load_type(&mut game, TypeScope::Mode, name, &core);
            Abilities::give_combatant(&mut game, object);
            let users = Filter::of_match(&game, users);
            Units::give_type(&mut game, object, UseSpec::new(action, users));
            object
        };
        let shrine = object("shrine", pray, "all");
        let well = object("well", draw, "all");
        let altar = object("altar", offer, "allies");
        let objects = Self {
            shrine,
            well,
            altar,
            pray,
            draw,
        };
        (game, objects)
    }
}

/// A use that reaches 2 m, winds up 100 ms, and, with `channel`, channels 300 ms, ticking each
/// 100 ms, which damage cuts when `channel` is true; it aims at any unit.
fn use_data(channel: Option<bool>) -> ActionData {
    let int = |value| Ranked::One(Number::Value(Scalar::Int(value)));
    ActionData {
        keys: ActionKeys {
            script: Some(PackagePath::parse("use.rhai").unwrap()),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::int(2),
            )))),
            windup_ms: Some(int(100)),
            channel: channel.map(|damage_interrupts| ChannelData {
                duration_ms: Some(int(300)),
                tick_ms: int(100),
                damage_interrupts,
                windup_spread: None,
                wind_down_ms: None,
                pays: None,
            }),
            ..ActionData::of(
                Targeting::Unit(FilterData::parse("all").unwrap()),
                CastData::default(),
            )
            .keys
        },
        section: Arc::new(UseActionData::default()),
        ..ActionData::of(
            Targeting::Unit(FilterData::parse("all").unwrap()),
            CastData::default(),
        )
    }
}

/// Player 0's user of the type `name` at `x` on team 0, with its use slot empty.
fn user(game: &mut Match, name: &str, x: i16) -> StableId {
    let slots = TimedSlots::new(ActionSlots::new([]).with_empty(USES, 1));
    let parts = (Owner::new(PlayerSlot::new(0)), slots);
    Spawn::named(game, name, (x, 0, 0)).parts(parts).place(game)
}

/// An object of `object` at `x` of `team`.
fn object(game: &mut Match, object: UnitType, x: i16, team: u8) -> StableId {
    Spawn::of(object, (x, 0, 0))
        .team(Team::new(team))
        .place(game)
}

/// Runs a tick in which player `player` orders `unit` `directive`.
fn order(game: &mut Match, player: u32, unit: StableId, directive: OrdersDirective) {
    game.step_orders(PlayerSlot::new(player), &[Order::one(unit, directive)]);
}

const fn use_of(target: StableId) -> OrdersDirective {
    OrdersDirective::Use { target }
}

/// The action the use slot of `user` holds.
fn held(game: &Match, user: StableId) -> Option<ActionId> {
    game.part::<ActionSlots>(user).slot(0).unwrap().action
}

/// Whether something is under way from the slots of `user`.
fn is_busy(game: &Match, user: StableId) -> bool {
    game.part::<ActionProgress>(user).in_progress().is_some()
}

fn failed(game: &mut Match) -> Vec<FailedCall> {
    game.resource_mut::<ScriptFailures>().take_calls()
}

#[test]
fn a_use_runs_on_its_object_and_its_held_use_ends_whole_or_by_an_order_a_block_a_death_or_damage() {
    let (mut game, objects) = Objects::load();
    // A walking user 1 m from `object`, each of type `object_type`, ordered to use it in tick 0;
    // `cut` acts before tick 8, between the channel's first and second ticks, and gives an order
    // for that tick, if any. The object's health after each of ticks 0 to 14.
    let mut x = 0;
    let mut run =
        |game: &mut Match,
         object_type: UnitType,
         cut: &dyn Fn(&mut Match, StableId) -> Option<OrdersDirective>| {
            x += 20;
            let user = user(game, "walking caster", x);
            let used = object(game, object_type, x + 1, 0);
            order(game, 0, user, use_of(used));
            let mut seen = vec![Stats::health(game, used)];
            for tick in 1..=14 {
                match (tick == 8).then(|| cut(game, user)).flatten() {
                    Some(action) => order(game, 0, user, action),
                    None => game.step(),
                }
                seen.push(Stats::health(game, used));
            }
            assert_eq!(failed(game), []);
            seen
        };
    // Whole: ordered in tick 0, it winds up 3 ticks and resolves in tick 3 for 1; it channels
    // from tick 4, ticking each 3 ticks, in ticks 7, 10 and 13, its end, for 10 each.
    let whole = [
        500, 500, 500, 499, 499, 499, 499, 489, 489, 489, 479, 479, 479, 469, 469,
    ];
    assert_eq!(run(&mut game, objects.shrine, &|_, _| None), whole);
    // A new order, a stop, cuts it in tick 8: its `on_interrupt` runs in that tick's Hit for 100,
    // and it ticks no more; so does a tag that blocks `use`, the group of a use, and not one that
    // blocks only casting.
    let cut = [
        500, 500, 500, 499, 499, 499, 499, 489, 389, 389, 389, 389, 389, 389, 389,
    ];
    let stopped = run(&mut game, objects.shrine, &|_, _| {
        Some(OrdersDirective::Stop)
    });
    assert_eq!(stopped, cut);
    let blocked = run(&mut game, objects.shrine, &|game, user| {
        Units::set_blocks(game, user, &[Block::Use]);
        None
    });
    assert_eq!(blocked, cut);
    let cast_blocked = run(&mut game, objects.shrine, &|game, user| {
        Units::set_blocks(game, user, &[Block::Cast]);
        None
    });
    assert_eq!(cast_blocked, whole);
    // A death in tick 8's Resolve cuts the well's draw, which damage does not cut: its
    // `on_interrupt` runs in tick 9's Hit.
    let late = [
        500, 500, 500, 499, 499, 499, 499, 489, 489, 389, 389, 389, 389, 389, 389,
    ];
    let killed = run(&mut game, objects.well, &|game, user| {
        Combat::queue_damage(game, None, user, Num::int(1000), "true");
        None
    });
    assert_eq!(killed, late);
    // Damage of 1 in tick 8's Resolve cuts the shrine's prayer, which damage interrupts, as a
    // death does; not the well's draw; and neither damage of 0 nor a heal cuts the prayer.
    let damage = |amount: i64| {
        move |game: &mut Match, user: StableId| {
            Combat::queue_damage(game, None, user, Num::int(amount), "true");
            None
        }
    };
    assert_eq!(run(&mut game, objects.shrine, &damage(1)), late);
    assert_eq!(run(&mut game, objects.well, &damage(1)), whole);
    assert_eq!(run(&mut game, objects.shrine, &damage(0)), whole);
    let healed = run(&mut game, objects.shrine, &|game, user| {
        Combat::heal_unit(game, user, Num::int(50));
        None
    });
    assert_eq!(healed, whole);
}

#[test]
fn a_use_is_dropped_by_a_unit_with_no_use_slot_one_its_users_refuse_or_one_beyond_range_that_stands()
 {
    let (mut game, objects) = Objects::load();
    let shrine = object(&mut game, objects.shrine, 1, 0);
    // A combatant of player 0 has no use slot: it starts nothing, and the shrine keeps its
    // health past the tick a use would resolve in.
    let owner = Owner::new(PlayerSlot::new(0));
    let unslotted = Spawn::named(&game, "combatant", (0, 0, 0))
        .team(Team::new(0))
        .parts(owner)
        .place(&mut game);
    order(&mut game, 0, unslotted, use_of(shrine));
    assert_eq!(game.part::<ActionProgress>(unslotted).in_progress(), None);
    // An altar of team 1 admits its allies alone, and the user is of team 0: its slot stays
    // empty. An altar of the user's team admits it: ordered in tick 2, its offering resolves in
    // tick 5.
    let user = user(&mut game, "walking caster", 0);
    let foreign = object(&mut game, objects.altar, 1, 1);
    order(&mut game, 0, user, use_of(foreign));
    assert_eq!((held(&game, user), is_busy(&game, user)), (None, false));
    let own = object(&mut game, objects.altar, -1, 0);
    order(&mut game, 0, user, use_of(own));
    assert!(is_busy(&game, user));
    game.run_until(6);
    assert_eq!(
        [shrine, foreign, own].map(|id| Stats::health(&game, id)),
        [500, 500, 499]
    );
    // A unit that is no object, the combatant, is no use's target.
    order(&mut game, 0, user, use_of(unslotted));
    assert!(!is_busy(&game, user));
    // An order of the use slot by its index starts nothing: only a use runs a use.
    let target = ActionTarget::Unit(shrine);
    order(
        &mut game,
        0,
        user,
        OrdersDirective::Slot { slot: 0, target },
    );
    assert!(!is_busy(&game, user));
    // A user that stands 5 m from the shrine, beyond the prayer's 2 m, fills its slot but drops
    // the use as it would start; one that walks walks in range, 1 m a tick, and uses it.
    let standing = self::user(&mut game, "caster", 6);
    let walking = self::user(&mut game, "walking caster", -4);
    order(&mut game, 0, standing, use_of(shrine));
    assert_eq!(
        (held(&game, standing), is_busy(&game, standing)),
        (Some(objects.pray), false)
    );
    let now = game.now().get();
    order(&mut game, 0, walking, use_of(shrine));
    assert_eq!(
        (held(&game, walking), is_busy(&game, walking)),
        (Some(objects.pray), true)
    );
    // From 5 m, it walks 3 m in its order's tick and the two after, starts in the third after,
    // in range, and resolves 3 ticks later, its windup.
    game.run_until(now + 6);
    assert_eq!(Stats::health(&game, shrine), 500);
    game.step();
    assert_eq!(Stats::health(&game, shrine), 499);
    assert_eq!(failed(&mut game), []);

    // The match restores; one whose user of a type with a use slot holds none, or two, is
    // refused, where its use would fill a slot of no use.
    Release::restore_into(&game, &mut Objects::books().0);
    for slots in [
        ActionSlots::new([]),
        ActionSlots::new([]).with_empty(USES, 2),
    ] {
        let (mut game, _) = Objects::load();
        let parts = (Owner::new(PlayerSlot::new(0)), TimedSlots::new(slots));
        let id = Spawn::named(&game, "caster", (0, 0, 0))
            .parts(parts)
            .place(&mut game);
        let refused = SnapshotError::Invalid {
            ty: ActionSlots::NAME,
            id: Some(id),
        };
        let restored = Release::refusal_into(&game, &mut Objects::books().0);
        assert_eq!(restored, Some(refused));
    }
}

#[test]
fn a_second_use_of_one_object_keeps_its_slot_and_a_use_of_another_fills_it_afresh() {
    let (mut game, objects) = Objects::load();
    let user = user(&mut game, "walking caster", 0);
    let shrine = object(&mut game, objects.shrine, 1, 0);
    let well = object(&mut game, objects.well, -1, 0);
    let ready = |game: &Match| game.part::<SlotTimes>(user).slot(0).unwrap().ready_at;
    // The prayer resolves in tick 3, which spends its slot: no cooldown, so it is ready from
    // tick 3.
    order(&mut game, 0, user, use_of(shrine));
    game.run_until(14);
    assert_eq!(
        (held(&game, user), is_busy(&game, user), ready(&game)),
        (Some(objects.pray), false, Tick::new(3))
    );
    // A second prayer, in tick 14, keeps the slot as it is; a draw at the well, in tick 15,
    // fills it afresh, ready at once, and replaces the prayer before it resolves.
    order(&mut game, 0, user, use_of(shrine));
    assert_eq!(
        (held(&game, user), is_busy(&game, user), ready(&game)),
        (Some(objects.pray), true, Tick::new(3))
    );
    order(&mut game, 0, user, use_of(well));
    assert_eq!(
        (held(&game, user), is_busy(&game, user), ready(&game)),
        (Some(objects.draw), true, Tick::ZERO)
    );
    game.run_until(19);
    // The shrine took the first prayer whole; the well the draw's resolve, in tick 18.
    assert_eq!(
        [shrine, well].map(|id| Stats::health(&game, id)),
        [469, 499]
    );
    assert_eq!(failed(&mut game), []);
}

mod riding;
