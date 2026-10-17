use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};
use std::sync::Arc;

use bevy_ecs::system::{Query, RunSystemOnce};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionData, ActionSlots, ActionTarget, Actions, EffectData, EffectTo, SlotKind, SlotTimes,
    Targeting, UseGroup,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Action, Match, Order};
use campfire_combat::{HealEffectData, OnDeath};
use campfire_common::{PlayerSlot, Tick};
use campfire_declare::MatchFacts;
use campfire_items::*;
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_orders::OrdersAction;
use campfire_players::{OutOfPlay, Players, PlayersWorld, ResourceAmount};
use campfire_script::{Filter, ScriptLimits, ScriptPools};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimComponent, SimWorld, StableId};
use campfire_stats::{Hold, Life, ModifierData, Modifiers, StatChange, StatOp, StatRule, Stats};
use campfire_units::{Dead, Owner, UnitTypeCore, Units};
use campfire_values::{
    ActionId, Block, ModifierId, NameId, PackageIndex, ResourceId, Team, UnitType,
};
use campfire_values::{Number, Ranked, Scalar, Share, Stat};
use campfire_visibility::OutView;
use serde::Deserialize;
use serde::de::{IntoDeserializer, value};

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
    shop: StableId,
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
    fn place(game: &mut Match) -> Self {
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
        Stats::load_modifier(game, PackageIndex::MODE, "might", &might, None);
        let might = Stats::modifier(game, PackageIndex::MODE, "might").unwrap();
        let heal = EffectData {
            does: Arc::new(HealEffectData {
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
            let id = Actions::load(game, PackageIndex::MODE, name, data, None, 1).unwrap();
            Actions::load_effect_lists(game, id, PackageIndex::MODE, data);
            id
        };
        let (drink, flash) = (load("drink", &drink), load("flash", &flash));
        let shop_type = stock(game, [drink, flash], might);
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
        let shop = spawn_shop(game, shop_type);
        let id = Mode::spawn(game, carrier, Vec3::ZERO, Team::new(0), owner);
        game.part_mut::<Life>(id).take(Num::int(200));
        Self {
            id,
            shop,
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
        ScriptPools {
            limits: ScriptLimits::ROOMY,
            players: 1,
        },
    )
}

/// Gives `game` the item book and player 0's 1000 of `gold`; the shop's type.
fn stock(game: &mut Match, [drink, flash]: [ActionId; 2], might: ModifierId) -> UnitType {
    let resource = ResourceId::new(0);
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
    game.insert_resource(
        ItemBook::new(
            names,
            [
                item(50, 2, Some(1), Some(drink), Vec::new()),
                item(100, 1, None, Some(flash), Vec::new()),
                item(300, 1, None, None, vec![might]),
            ],
        )
        .unwrap(),
    );
    let sells = [POTION, FLASH, CHARM].map(ItemId::nth).to_vec();
    Players::spawn(game, 1, 1);
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(resource, 1000)
        .unwrap();
    shop_type(game, sells, "allies", share("0.5"))
}

/// The share `text` gives, as data reads it.
fn share(text: &str) -> Share {
    Share::deserialize(IntoDeserializer::<value::Error>::into_deserializer(text)).unwrap()
}

/// A new shop type that sells `sells` for resource 0 to the units `buyers` selects within 5 m,
/// and gives `share` back on a sale.
fn shop_type(game: &mut Match, sells: Vec<ItemId>, buyers: &str, share: Share) -> UnitType {
    let buyers = Filter::of_match(game, buyers);
    let shop = Shop::new(sells, ResourceId::new(0), Num::int(5), buyers, share);
    let shop_type = Units::load_next_type(game, &UnitTypeCore::default());
    Units::give_type(game, shop_type, shop);
    shop_type
}

/// A shop unit of `shop_type`, of team 0 and no player, at the origin.
fn spawn_shop(game: &mut Match, shop_type: UnitType) -> StableId {
    Mode::spawn(game, shop_type, Vec3::ZERO, Team::new(0), ())
}

const fn buy(item: u32, shop: StableId) -> ItemsAction {
    ItemsAction::Buy {
        item: ItemId::nth(item),
        shop,
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
    let shop = carrier.shop;
    carrier.order(&mut game, &[buy(POTION, shop), buy(POTION, shop)]);
    assert_eq!(carrier.items(&game), [Some((0, 2)), None, None]);
    assert_eq!(carrier.actions(&game), [drink, None, None]);
    // A restored inventory holds a slot for each of the 3 action slots of the carrier's use
    // group, and stacks its book allows: a potion of its one use, two at most to a slot, and a
    // flash with no uses.
    let sized = |slots: u8| {
        let inventory = Inventory::new(NonZeroU8::new(slots).unwrap());
        inventory.check(&game, game.unit_entity(carrier.id))
    };
    assert!(sized(3) && !sized(2) && !sized(4));
    let restored = |item: u32, count: u32, uses: Option<u32>| {
        let mut inventory = Inventory::new(NonZeroU8::new(3).unwrap());
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
    carrier.order(&mut game, &[buy(POTION, shop)]);
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
    let shop = carrier.shop;
    // One tick buys the flash and casts it: items' orders apply before those of `orders`, after
    // it in the module list, so the cast finds the flash in slot 0.
    carrier.order(
        &mut game,
        &[Action::from(buy(FLASH, shop)), Action::from(cast(0))],
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
    carrier.order(&mut game, &[buy(CHARM, shop)]);
    assert_eq!(carrier.mights(&game), 1);
    carrier.order(&mut game, &[buy(CHARM, shop)]);
    assert_eq!(
        (carrier.items(&game), carrier.mights(&game)),
        (vec![Some((2, 1)), Some((2, 1)), Some((1, 1))], 1)
    );
    carrier.order(&mut game, &[ItemsAction::Sell { slot: 0, shop }]);
    assert_eq!(carrier.mights(&game), 1);
    carrier.order(&mut game, &[ItemsAction::Sell { slot: 1, shop }]);
    assert_eq!(carrier.mights(&game), 0);
}

/// The items of `id`'s inventory, slot by slot, each by its book's index and its count.
fn carried(game: &World, id: StableId) -> Vec<Option<(usize, u32)>> {
    let inventory = game.part::<Inventory>(id);
    let carried =
        |slot: &Option<ItemStack>| slot.map(|carried| (carried.item.index(), carried.count.get()));
    inventory.slots().iter().map(carried).collect()
}

/// What the shop reader reads of an item at a shop for a unit: whether the shop serves it, and,
/// for an item the shop sells, the unit's purchase there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShopRead {
    served: bool,
    purchase: Option<Purchase>,
}

impl ShopRead {
    fn of(game: &mut Match, shop: StableId, unit: StableId, item: ItemId) -> Self {
        let entity = game.unit_entity(unit);
        let read = move |shopping: Shopping<'_, '_>, inventories: Query<'_, '_, &Inventory>| {
            let inventory = inventories.get(entity).unwrap();
            let sold = shopping.shop(shop).unwrap();
            Self {
                served: shopping.serving(shop, unit).is_some(),
                purchase: shopping.purchase(sold, inventory, item, &mut Vec::new()),
            }
        };
        game.run_system_once(read).unwrap()
    }

    /// The nearest shop unit whose buyers select `unit`.
    fn nearest(game: &mut Match, unit: StableId) -> Option<StableId> {
        let read = move |shopping: Shopping<'_, '_>| shopping.nearest_for(unit);
        game.run_system_once(read).unwrap()
    }

    /// The items the shop unit `shop` sells.
    fn items(game: &mut Match, shop: StableId) -> Vec<ItemId> {
        let read = move |shopping: Shopping<'_, '_>| shopping.shop(shop).unwrap().items().to_vec();
        game.run_system_once(read).unwrap()
    }
}

/// Player 0's gold.
fn gold(game: &World) -> i64 {
    let resources = game.player_resources(PlayerSlot::new(0));
    resources.unwrap().amount(ResourceId::new(0))
}

/// Player 0's orders of `actions` to `id`.
fn trades(id: StableId, actions: &[ItemsAction]) -> Vec<u8> {
    let orders: Vec<Order> = actions
        .iter()
        .map(|&action| Order::one(id, action))
        .collect();
    Order::payload(&orders)
}

/// A match of a blade of 300 gold, an edge of 1000 built from two blades, a potion of 50, three
/// to a slot, and a charm of 125 the shop does not sell; and team 0's shop at the origin, which
/// serves its allies within 5 m, the dead from anywhere, and gives 70% back on a sale.
#[derive(Debug)]
struct ShopMatch {
    game: Match,
    shop: StableId,
    /// A shopper's type: an inventory of three.
    shopper: UnitType,
    /// A shop type that sells the potion to its living allies alone.
    living: UnitType,
}

fn shop_match() -> ShopMatch {
    let declared = [Capability::Stats, Capability::Items];
    let mut game = Match::server(
        &declared,
        ScriptPools {
            limits: ScriptLimits::ROOMY,
            players: 2,
        },
    );
    let gold = ResourceId::new(0);
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
    )
    .unwrap();
    let sells = [0, 1, 2].map(ItemId::nth).to_vec();
    game.insert_resource(book);
    Players::spawn(&mut game, 2, 1);
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(gold, 2000)
        .unwrap();
    let three = NonZeroU8::new(3).unwrap();
    let shopper = Units::load_next_type(&mut game, &UnitTypeCore::default());
    Units::give_type(&mut game, shopper, Inventory::new(three));
    Units::give_type(&mut game, shopper, UseGroup::new(SlotKind::new(0)));
    let shop = shop_type(&mut game, sells, "allies", share("0.7"));
    let potion = vec![ItemId::nth(2)];
    let living = shop_type(&mut game, potion, "allies:!dead", share("0.7"));
    let shop = spawn_shop(&mut game, shop);
    ShopMatch {
        game,
        shop,
        shopper,
        living,
    }
}

#[test]
fn a_hero_buys_and_sells_at_its_shop_for_exactly_the_price_and_the_share() {
    // A match with no `orders`: the trades are items' own orders.
    let ShopMatch {
        mut game,
        shop,
        shopper,
        living,
    } = shop_match();
    let owner = Owner::new(PlayerSlot::new(0));
    let hero = Mode::spawn(&mut game, shopper, (0, 0, 0), Team::new(0), owner);
    let step = |game: &mut Match, actions: &[ItemsAction]| {
        game.step_with(&[(PlayerSlot::new(0), &trades(hero, actions))]);
    };
    let held = |game: &Match| (gold(game), carried(game, hero));
    // The reader: the shop sells its three in its order; the edge costs its 1000 with no blade
    // held, a potion 50, and the charm, which it does not sell, nothing.
    let read = |game: &mut Match, item: u32| ShopRead::of(game, shop, hero, ItemId::nth(item));
    let sold: Vec<usize> = ShopRead::items(&mut game, shop)
        .iter()
        .map(|item| item.index())
        .collect();
    assert_eq!(sold, [0, 1, 2]);
    let buys = |price, room| Some(Purchase { price, room });
    assert_eq!(
        read(&mut game, 1),
        ShopRead {
            served: true,
            purchase: buys(1000, true)
        }
    );
    assert_eq!(read(&mut game, 2).purchase, buys(50, true));
    assert_eq!(read(&mut game, 3).purchase, None);
    // Two blades, 300 each, from 2000: 1400.
    step(&mut game, &[buy(0, shop), buy(0, shop)]);
    assert_eq!(held(&game), (1400, vec![Some((0, 1)), Some((0, 1)), None]));
    // The edge less the two blades it is built from, as the reader reads it: 1000 − 300 − 300 =
    // 400.
    assert_eq!(read(&mut game, 1).purchase, buys(400, true));
    // The edge gives both blades up: 1000 − 300 − 300 = 400, and takes the first slot they left.
    step(&mut game, &[buy(1, shop)]);
    assert_eq!(held(&game), (1000, vec![Some((1, 1)), None, None]));
    // Three potions stack in the first empty slot, 50 each; a fourth needs the last slot.
    step(
        &mut game,
        &[buy(2, shop), buy(2, shop), buy(2, shop), buy(2, shop)],
    );
    let full = vec![Some((1, 1)), Some((2, 3)), Some((2, 1))];
    assert_eq!(held(&game), (800, full.clone()));
    // No room for a blade, and the charm is not sold: both dropped; the reader reads no room.
    assert_eq!(read(&mut game, 0).purchase, buys(300, false));
    step(&mut game, &[buy(0, shop), buy(3, shop)]);
    assert_eq!(held(&game), (800, full));
    // Three potions sell for 70% of 150, 105; a swap moves the edge; it sells for 700.
    step(
        &mut game,
        &[
            ItemsAction::Sell { slot: 1, shop },
            ItemsAction::Swap { from: 0, to: 2 },
        ],
    );
    assert_eq!(held(&game), (905, vec![Some((2, 1)), None, Some((1, 1))]));
    step(
        &mut game,
        &[
            ItemsAction::Sell { slot: 2, shop },
            ItemsAction::Sell { slot: 1, shop },
        ],
    );
    assert_eq!(held(&game), (1605, vec![Some((2, 1)), None, None]));

    // Out of its shop, it neither buys nor sells, but swaps; the other player's orders and those
    // past its gold are dropped; dead, it buys and sells anywhere.
    *game.part_mut::<Position>(hero) = Position::from((10, 0, 0));
    assert!(!read(&mut game, 2).served);
    step(
        &mut game,
        &[
            buy(2, shop),
            ItemsAction::Sell { slot: 0, shop },
            ItemsAction::Swap { from: 0, to: 1 },
        ],
    );
    assert_eq!(held(&game), (1605, vec![None, Some((2, 1)), None]));
    game.step_with(&[(PlayerSlot::new(1), &trades(hero, &[buy(2, shop)]))]);
    assert_eq!(gold(&game), 1605);
    game.insert_parts(hero, Dead);
    step(
        &mut game,
        &[
            buy(0, shop),
            buy(0, shop),
            buy(0, shop),
            buy(0, shop),
            buy(0, shop),
            buy(0, shop),
        ],
    );
    // 1605 buys a blade into slot 0 and a potion's stack in slot 1 has no room for a blade, so
    // the next blade takes slot 2; then 1005 left, and slots are full: the rest are dropped.
    assert_eq!(
        held(&game),
        (1005, vec![Some((0, 1)), Some((2, 1)), Some((0, 1))])
    );
    // An edge from the two blades costs 400 of 1005; one more edge past the gold left is dropped.
    let resource = ResourceId::new(0);
    let resources = game.player_resources_mut(PlayerSlot::new(0));
    resources.unwrap().add(resource, -1000).unwrap();
    step(&mut game, &[buy(1, shop)]);
    assert_eq!(
        held(&game),
        (5, vec![Some((0, 1)), Some((2, 1)), Some((0, 1))])
    );
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(resource, 395)
        .unwrap();
    step(&mut game, &[buy(1, shop)]);
    assert_eq!(held(&game), (0, vec![Some((1, 1)), Some((2, 1)), None]));

    // A unit of another team in the shop's region is out of a shop of its own.
    let other = Mode::spawn(&mut game, shopper, (0, 0, 0), Team::new(1), owner);
    assert!(!ShopRead::of(&mut game, shop, other, ItemId::nth(0)).served);
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(resource, 300)
        .unwrap();
    game.step_with(&[(PlayerSlot::new(0), &trades(other, &[buy(0, shop)]))]);
    assert_eq!(
        (gold(&game), carried(&game, other)),
        (300, vec![None, None, None])
    );

    // Player 0 out of play, its hero dead: a potion of 50 is bought while it sees what its team
    // sees, and none once it sees what a spectator sees, as each capability's orders refuse it.
    let out = game.player_entity(PlayerSlot::new(0)).unwrap();
    game.entity_mut(out).insert(OutOfPlay);
    game.resource_mut::<MatchFacts>().out = OutView::Spectators;
    step(&mut game, &[buy(2, shop)]);
    assert_eq!(gold(&game), 300);
    game.resource_mut::<MatchFacts>().out = OutView::Team;
    step(&mut game, &[buy(2, shop)]);
    assert_eq!(gold(&game), 250);

    // A shop that leaves the dead out, as Warcraft III's does, serves the hero only once it lives
    // again, and within its reach: from 5 m, the reach, a potion of 50; from a bit past it, none.
    // An order that names a unit that is no shop, the hero itself, is dropped.
    let living_shop = spawn_shop(&mut game, living);
    step(&mut game, &[buy(2, living_shop)]);
    assert_eq!(gold(&game), 250);
    let entity = game.unit_entity(hero);
    game.entity_mut(entity).remove::<Dead>();
    *game.part_mut::<Position>(hero) = Position::from((5, 0, 0));
    step(&mut game, &[buy(2, living_shop), buy(2, hero)]);
    assert_eq!(gold(&game), 200);
    let past = Vec3::new(Num::int(5) + Num::from_bits(1), Num::ZERO, Num::ZERO);
    *game.part_mut::<Position>(hero) = Position::from(past);
    step(&mut game, &[buy(2, living_shop)]);
    assert_eq!(gold(&game), 200);

    // The nearest shop whose buyers select the hero, in its reach or not, as a panel shows it:
    // of the two at the origin, 5 m and a bit away, the lower stable id; then one at (8, 0, 0),
    // 3 m away; and once the hero is dead, the one that serves the dead.
    let nearest = |game: &mut Match| ShopRead::nearest(game, hero);
    assert_eq!(nearest(&mut game), Some(shop));
    let near = Mode::spawn(&mut game, living, (8, 0, 0), Team::new(0), ());
    assert_eq!(nearest(&mut game), Some(near));
    game.insert_parts(hero, Dead);
    assert_eq!(nearest(&mut game), Some(shop));
}
