use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};

use campfire_common::{PlayerSlot, Tick};
use campfire_math::Num;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Position, SimComponent, SimWorld, StableId};
use serde::Deserialize;

use super::*;
use crate::actions::Actions;
use crate::actions::action_data::ActionData;
use crate::actions::action_target::ActionTarget;
use crate::actions::effect_data::{DoesData, EffectData, EffectTo, HealFields};
use crate::actions::slot_kind::SlotKind;
use crate::actions::targeting::Targeting;
use crate::books::Books;
use crate::combat::on_death::OnDeath;
use crate::geometry::bounds::Bounds;
use crate::items::inventory::ItemStack;
use crate::items::item_book::ItemSpec;
use crate::items::item_id::ItemId;
use crate::items::items_action::ItemsAction;
use crate::items::shop::{Shop, ShopPlace};
use crate::mode::Mode;
use crate::orders::order::Action;
use crate::orders::order::Order;
use crate::orders::orders_action::OrdersAction;
use crate::players::Players;
use crate::players::out_of_play::OutOfPlay;
use crate::players::players_world::{PlayersWorld, PlayersWorldCrate};
use crate::players::resource_amount::ResourceAmount;
use crate::release::r#match::Match;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::Stats;
use crate::stats::life::Life;
use crate::stats::lifetime::Hold;
use crate::stats::modifier_data::ModifierData;
use crate::stats::modifiers::Modifiers;
use crate::stats::stat_change::StatChange;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::block::Block;
use crate::units::dead::Dead;
use crate::units::modifier_id::ModifierId;
use crate::units::owner::Owner;
use crate::units::resource_id::ResourceId;
use crate::units::team::Team;
use crate::units::unit_type_core::UnitTypeCore;
use crate::values::number::Number;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::share::Share;
use crate::values::stat::Stat;
use crate::visibility::OutView;

const POTION: u32 = 0;
const FLASH: u32 = 1;
const CHARM: u32 = 2;

/// A carrier of three inventory slots, of slot kind 0, with 200 of its 500 life lost, its player
/// 1000 gold, and a shop round it that sells: a potion, two to a slot, of one use, that heals 50; a
/// flash of no effect and a cooldown of 1000 ms; and a charm whose carrier holds Might, 10 attack
/// damage.
#[derive(Debug, Clone, Copy)]
struct Carrier {
    id: StableId,
    drink: ActionId,
    flash: ActionId,
    might: ModifierId,
}

impl Carrier {
    /// The capabilities a carrier's match declares.
    const CAPABILITIES: [Capability; 6] = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
        Capability::Items,
    ];

    /// Loads the carrier's stats, modifier, actions and shop into `game`, a match of
    /// `CAPABILITIES`, and spawns it.
    fn place(game: &mut Match) -> Carrier {
        Units::name_kinds(game, &["physical"], &["health"], &["gold"]);
        let attack_damage = Stat::named("attack_damage").unwrap();
        Stats::load_rules(
            game,
            &BTreeMap::from([(attack_damage.clone(), StatRule::default())]),
        );
        let change = StatChange {
            op: StatOp::Add,
            value: Number::Value(Scalar::Int(10)),
        };
        let might = ModifierData {
            stats: BTreeMap::from([(attack_damage, change)]),
            ..ModifierData::default()
        };
        Stats::load_modifier(game, 0, "might", &might, None);
        let might = Stats::modifier(game, 0, "might").unwrap();
        let heal = EffectData {
            does: DoesData::Heal(HealFields {
                amount: Number::Value(Scalar::Int(50)),
            }),
            to: EffectTo::Source,
        };
        let drink = ActionData {
            on_resolve: vec![heal],
            ..ActionData::cast(Targeting::None)
        };
        let flash = ActionData {
            cooldown_ms: Some(Ranked::One(Number::Value(Scalar::Int(1000)))),
            ..ActionData::cast(Targeting::None)
        };
        let mut load = |name: &str, data: &ActionData| {
            let id = Actions::load(game, 0, name, data, None, 1).unwrap();
            Actions::load_effect_lists(game, id, 0, data);
            id
        };
        let (drink, flash) = (load("drink", &drink), load("flash", &flash));
        stock(game, [drink, flash], might);
        // Its type: 500 life that stays when it dies, and an inventory of 3, whose slots it has.
        let kind = SlotKind::new(0);
        let carrier = Units::load_next_type(game, &UnitTypeCore::default());
        Units::give_type(game, carrier, Life::full(Num::int(500)));
        Units::give_type(game, carrier, OnDeath::Stay);
        Units::give_type(game, carrier, ActionSlots::new([]).with_empty(kind, 3));
        let three = NonZeroU8::new(3).unwrap();
        Units::give_type(game, carrier, Inventory::new(three));
        Units::give_type(game, carrier, UseGroup::new(kind));
        let owner = Owner::new(PlayerSlot::new(0));
        let id = Mode::spawn(game, carrier, Position::ORIGIN, Team::new(0), owner);
        game.part_mut::<Life>(id).take(Num::int(200));
        Carrier {
            id,
            drink,
            flash,
            might,
        }
    }

    /// Runs a tick in which player 0 orders the carrier each of `actions`.
    fn order<A: Into<Action> + Copy>(self, game: &mut World, actions: &[A]) {
        let unit = self.id;
        let orders: Vec<Order> = actions
            .iter()
            .map(|&action| Order::one(unit, action))
            .collect();
        let payload = Order::payload(&orders);
        game.step_with(&[(PlayerSlot::new(0), &payload)]);
    }

    /// The carrier's life.
    fn life(self, game: &World) -> Num {
        game.part::<Life>(self.id).current()
    }

    /// Each slot's item type and count, then its action and the tick it is ready.
    fn items(self, game: &World) -> Vec<Option<(usize, u32)>> {
        carried(game, self.id)
    }

    /// The action in each of the carrier's slots.
    fn actions(self, game: &World) -> Vec<Option<ActionId>> {
        let slots = game.part::<ActionSlots>(self.id);
        slots.iter().map(|slot| slot.action).collect()
    }

    /// How many instances of Might the carrier holds, each from itself as a passive.
    fn mights(self, game: &World) -> usize {
        let modifiers = game.part::<Modifiers>(self.id);
        modifiers
            .iter()
            .filter(|instance| {
                instance.id == self.might
                    && instance.source == Some(self.id)
                    && instance.lifetime.held_by(Hold::Passive)
            })
            .count()
    }
}

/// A server's match of the carrier's capabilities.
fn carrier_match() -> Match {
    Match::server(
        &Carrier::CAPABILITIES,
        ScriptBudgets::new(ScriptLimits::ROOMY, 1),
    )
}

/// Gives `game` the item book, the shop at the origin, and player 0's 1000 of `gold`.
fn stock(game: &mut Match, [drink, flash]: [ActionId; 2], might: ModifierId) {
    let resource = ResourceId::nth(0);
    let item =
        |cost: i64, stack: u32, uses: Option<u32>, action, modifiers: Vec<ModifierId>| ItemSpec {
            cost: vec![ResourceAmount {
                resource,
                amount: cost,
            }],
            components: Vec::new(),
            stack: NonZeroU32::new(stack).unwrap(),
            uses: uses.map(|uses| NonZeroU32::new(uses).unwrap()),
            modifiers,
            action,
        };
    let names = ["drink", "flash", "might"].into_iter().collect();
    game.insert_resource(ItemBook::new(
        names,
        [
            item(50, 2, Some(1), Some(drink), Vec::new()),
            item(100, 1, None, Some(flash), Vec::new()),
            item(300, 1, None, None, vec![might]),
        ],
    ));
    let share = Share::deserialize(toml::Value::String("0.5".to_owned())).unwrap();
    let places = vec![ShopPlace {
        team: Team::new(0),
        region: Bounds::new([Num::int(-5); 2], [Num::int(5); 2]).unwrap(),
    }];
    let sells = [POTION, FLASH, CHARM].map(ItemId::nth).to_vec();
    Books::load(
        game,
        ShopBook(Some(Shop::new(sells, resource, places, share))),
    );
    Players::spawn(game, 1, 1);
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(resource, 1000)
        .unwrap();
}

const fn buy(item: u32) -> ItemsAction {
    ItemsAction::Buy {
        item: ItemId::nth(item),
    }
}

const fn cast(slot: u8) -> OrdersAction {
    OrdersAction::Slot {
        slot,
        target: ActionTarget::None,
    }
}

#[test]
fn an_items_action_sits_in_its_slot_and_spends_its_uses_in_the_use_group() {
    let mut game = carrier_match();
    let carrier = Carrier::place(&mut game);
    let drink = Some(carrier.drink);
    // Two potions stack in slot 0, which holds their action; the others hold none.
    carrier.order(&mut game, &[buy(POTION), buy(POTION)]);
    assert_eq!(carrier.items(&game), [Some((0, 2)), None, None]);
    assert_eq!(carrier.actions(&game), [drink, None, None]);
    // A restored inventory holds stacks its book allows: a potion of its one use, two at most to
    // a slot, and a flash with no uses.
    let restored = |item: u32, count: u32, uses: Option<u32>| {
        let mut inventory = Inventory::new(NonZeroU8::new(1).unwrap());
        let stack = ItemStack {
            item: ItemId::nth(item),
            count: NonZeroU32::new(count).unwrap(),
            uses: uses.map(|uses| NonZeroU32::new(uses).unwrap()),
        };
        inventory.restore(0, stack);
        inventory.check(&game, game.unit_entity(carrier.id))
    };
    assert!(restored(POTION, 2, Some(1)) && restored(FLASH, 1, None));
    assert!(!restored(POTION, 3, Some(1)));
    assert!(!restored(POTION, 1, Some(2)));
    assert!(!restored(POTION, 1, None));
    assert!(!restored(FLASH, 1, Some(1)));
    assert!(!restored(9, 1, None));
    // One resolves as its order comes, and heals 50 of the 200 lost: one potion left.
    carrier.order(&mut game, &[cast(0)]);
    assert_eq!(
        (carrier.life(&game), carrier.items(&game)),
        (Num::int(350), vec![Some((0, 1)), None, None])
    );
    // A stun blocks `use`: the drink waits as its order, and resolves the tick the stun ends,
    // spending the last potion, which leaves the slot and its action empty.
    Units::set_blocks(
        &mut game,
        carrier.id,
        &[Block::Move, Block::Attack, Block::Cast, Block::Use],
    );
    carrier.order(&mut game, &[cast(0)]);
    assert_eq!(
        (carrier.life(&game), carrier.items(&game)),
        (Num::int(350), vec![Some((0, 1)), None, None])
    );
    Units::set_blocks(&mut game, carrier.id, &[]);
    game.step();
    assert_eq!(
        (carrier.life(&game), carrier.items(&game)),
        (Num::int(400), vec![None, None, None])
    );
    assert_eq!(carrier.actions(&game), [None, None, None]);
    // A silence blocks casts alone: a new potion is drunk through it.
    carrier.order(&mut game, &[buy(POTION)]);
    Units::set_blocks(&mut game, carrier.id, &[Block::Cast]);
    carrier.order(&mut game, &[cast(0)]);
    assert_eq!(
        (carrier.life(&game), carrier.items(&game)),
        (Num::int(450), vec![None, None, None])
    );
}

#[test]
fn an_active_keeps_its_cooldown_across_a_swap_and_a_carried_item_holds_its_modifier_once() {
    let mut game = carrier_match();
    let carrier = Carrier::place(&mut game);
    // One tick buys the flash and casts it: items' orders apply before those of `orders`, after
    // it in the module list, so the cast finds the flash in slot 0.
    carrier.order(
        &mut game,
        &[Action::from(buy(FLASH)), Action::from(cast(0))],
    );
    let ready = game.part::<SlotTimes>(carrier.id).slot(0).unwrap().ready_at;
    assert!(ready > game.now());
    // The flash moves to slot 2 with its cooldown; slot 0 is empty and ready.
    carrier.order(&mut game, &[ItemsAction::Swap { from: 0, to: 2 }]);
    let slots = game.part::<ActionSlots>(carrier.id);
    let times = game.part::<SlotTimes>(carrier.id);
    let held = |at: u8| {
        let time = times.slot(at).unwrap();
        (slots.slot(at).unwrap().action, time.ready_at)
    };
    assert_eq!(held(2), (Some(carrier.flash), ready));
    assert_eq!(held(0), (None, Tick::ZERO));
    // A charm holds Might from the carrier, as a passive; a second charm holds it no more often;
    // selling one keeps it, selling both lets it go.
    assert_eq!(carrier.mights(&game), 0);
    carrier.order(&mut game, &[buy(CHARM)]);
    assert_eq!(carrier.mights(&game), 1);
    carrier.order(&mut game, &[buy(CHARM)]);
    assert_eq!(
        (carrier.items(&game), carrier.mights(&game)),
        (vec![Some((2, 1)), Some((2, 1)), Some((1, 1))], 1)
    );
    carrier.order(&mut game, &[ItemsAction::Sell { slot: 0 }]);
    assert_eq!(carrier.mights(&game), 1);
    carrier.order(&mut game, &[ItemsAction::Sell { slot: 1 }]);
    assert_eq!(carrier.mights(&game), 0);
}

/// The items of `id`'s inventory, slot by slot, each by its book's index and its count.
fn carried(game: &World, id: StableId) -> Vec<Option<(usize, u32)>> {
    let inventory = game.part::<Inventory>(id);
    let carried =
        |slot: &Option<ItemStack>| slot.map(|carried| (carried.item.index(), carried.count.get()));
    inventory.slots().iter().map(carried).collect()
}

/// Player 0's gold.
fn gold(game: &World) -> i64 {
    let resources = game.player_resources(PlayerSlot::new(0));
    resources.unwrap().amount(ResourceId::nth(0))
}

/// Player 0's orders of `actions` to `id`.
fn trades(id: StableId, actions: &[ItemsAction]) -> Vec<u8> {
    let orders: Vec<Order> = actions
        .iter()
        .map(|&action| Order::one(id, action))
        .collect();
    Order::payload(&orders)
}

/// A blade of 300 gold, an edge of 1000 built from two blades, a potion of 50, three to a slot,
/// and a charm of 125 the shop does not sell; and team 0's shop from (−5, −5) to (5, 5), which
/// gives 70% back on a sale.
fn shop_match() -> Match {
    let declared = [Capability::Stats, Capability::Items];
    let mut game = Match::server(&declared, ScriptBudgets::new(ScriptLimits::ROOMY, 2));
    let gold = ResourceId::nth(0);
    let item = |cost: i64, components: &[u32], stack: u32| ItemSpec {
        cost: vec![ResourceAmount {
            resource: gold,
            amount: cost,
        }],
        components: components.iter().map(|&at| ItemId::nth(at)).collect(),
        stack: NonZeroU32::new(stack).unwrap(),
        uses: None,
        modifiers: Vec::new(),
        action: None,
    };
    let names = ["boots", "blade", "potion", "ward"].into_iter().collect();
    let book = ItemBook::new(
        names,
        [
            item(300, &[], 1),
            item(1000, &[0, 0], 1),
            item(50, &[], 3),
            item(125, &[], 1),
        ],
    );
    let share = Share::deserialize(toml::Value::String("0.7".to_owned())).unwrap();
    let region = Bounds::new([Num::int(-5); 2], [Num::int(5); 2]).unwrap();
    let places = vec![ShopPlace {
        team: Team::new(0),
        region,
    }];
    let sells = [0, 1, 2].map(ItemId::nth).to_vec();
    game.insert_resource(book);
    Books::load(
        &mut game,
        ShopBook(Some(Shop::new(sells, gold, places, share))),
    );
    Players::spawn(&mut game, 2, 1);
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(gold, 2000)
        .unwrap();
    game
}

#[test]
fn a_hero_buys_and_sells_at_its_shop_for_exactly_the_price_and_the_share() {
    // A match with no `orders`: the trades are items' own orders.
    let mut game = shop_match();
    let three = NonZeroU8::new(3).unwrap();
    let shopper = Units::load_next_type(&mut game, &UnitTypeCore::default());
    Units::give_type(&mut game, shopper, Inventory::new(three));
    Units::give_type(&mut game, shopper, UseGroup::new(SlotKind::new(0)));
    let owner = Owner::new(PlayerSlot::new(0));
    let hero = Mode::spawn(&mut game, shopper, (0, 0, 0), Team::new(0), owner);
    let step = |game: &mut Match, actions: &[ItemsAction]| {
        game.step_with(&[(PlayerSlot::new(0), &trades(hero, actions))]);
    };
    let held = |game: &Match| (gold(game), carried(game, hero));
    // Two blades, 300 each, from 2000: 1400.
    step(&mut game, &[buy(0), buy(0)]);
    assert_eq!(held(&game), (1400, vec![Some((0, 1)), Some((0, 1)), None]));
    // The edge gives both blades up: 1000 − 300 − 300 = 400, and takes the first slot they left.
    step(&mut game, &[buy(1)]);
    assert_eq!(held(&game), (1000, vec![Some((1, 1)), None, None]));
    // Three potions stack in the first empty slot, 50 each; a fourth needs the last slot.
    step(&mut game, &[buy(2), buy(2), buy(2), buy(2)]);
    let full = vec![Some((1, 1)), Some((2, 3)), Some((2, 1))];
    assert_eq!(held(&game), (800, full.clone()));
    // No room for a blade, and the charm is not sold: both dropped.
    step(&mut game, &[buy(0), buy(3)]);
    assert_eq!(held(&game), (800, full));
    // Three potions sell for 70% of 150, 105; a swap moves the edge; it sells for 700.
    step(
        &mut game,
        &[
            ItemsAction::Sell { slot: 1 },
            ItemsAction::Swap { from: 0, to: 2 },
        ],
    );
    assert_eq!(held(&game), (905, vec![Some((2, 1)), None, Some((1, 1))]));
    step(
        &mut game,
        &[ItemsAction::Sell { slot: 2 }, ItemsAction::Sell { slot: 1 }],
    );
    assert_eq!(held(&game), (1605, vec![Some((2, 1)), None, None]));

    // Out of its shop, it neither buys nor sells, but swaps; the other player's orders and those
    // past its gold are dropped; dead, it buys and sells anywhere.
    *game.part_mut::<Position>(hero) = Position::from((10, 0, 0));
    step(
        &mut game,
        &[
            buy(2),
            ItemsAction::Sell { slot: 0 },
            ItemsAction::Swap { from: 0, to: 1 },
        ],
    );
    assert_eq!(held(&game), (1605, vec![None, Some((2, 1)), None]));
    game.step_with(&[(PlayerSlot::new(1), &trades(hero, &[buy(2)]))]);
    assert_eq!(gold(&game), 1605);
    game.insert_parts(hero, Dead);
    step(&mut game, &[buy(0), buy(0), buy(0), buy(0), buy(0), buy(0)]);
    // 1605 buys a blade into slot 0 and a potion's stack in slot 1 has no room for a blade, so
    // the next blade takes slot 2; then 1005 left, and slots are full: the rest are dropped.
    assert_eq!(
        held(&game),
        (1005, vec![Some((0, 1)), Some((2, 1)), Some((0, 1))])
    );
    // An edge from the two blades costs 400 of 1005; one more edge past the gold left is dropped.
    let resource = ResourceId::nth(0);
    let resources = game.player_resources_mut(PlayerSlot::new(0));
    resources.unwrap().add(resource, -1000).unwrap();
    step(&mut game, &[buy(1)]);
    assert_eq!(
        held(&game),
        (5, vec![Some((0, 1)), Some((2, 1)), Some((0, 1))])
    );
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(resource, 395)
        .unwrap();
    step(&mut game, &[buy(1)]);
    assert_eq!(held(&game), (0, vec![Some((1, 1)), Some((2, 1)), None]));

    // A unit of another team in the shop's region is out of a shop of its own.
    let other = Mode::spawn(&mut game, shopper, (0, 0, 0), Team::new(1), owner);
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(resource, 300)
        .unwrap();
    game.step_with(&[(PlayerSlot::new(0), &trades(other, &[buy(0)]))]);
    assert_eq!(
        (gold(&game), carried(&game, other)),
        (300, vec![None, None, None])
    );

    // Player 0 out of play, its hero dead: a potion of 50 is bought while it sees what its team
    // sees, and none once it sees what a spectator sees, as each capability's orders refuse it.
    let out = game.player_entity(PlayerSlot::new(0)).unwrap();
    game.entity_mut(out).insert(OutOfPlay);
    game.insert_resource(OutView::Spectators);
    step(&mut game, &[buy(2)]);
    assert_eq!(gold(&game), 300);
    game.insert_resource(OutView::Team);
    step(&mut game, &[buy(2)]);
    assert_eq!(gold(&game), 250);
}
