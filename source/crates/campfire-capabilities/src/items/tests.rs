use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};

use campfire_common::PlayerSlot;
use campfire_math::{Num, Vec3};
use campfire_sim::{Capability, Position, SimComponent, TickInput, TickInputs};
use serde::Deserialize;

use super::*;
use crate::actions::Actions;
use crate::actions::action_data::{ActionData, Targeting};
use crate::actions::action_target::ActionTarget;
use crate::actions::effect_data::{EffectData, EffectTo, Effecting};
use crate::actions::effect_lists::EffectLists;
use crate::actions::slot_kind::SlotKind;
use crate::capability_set::test_match::TestMatch;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::geometry::region::Region;
use crate::items::inventory::ItemStack;
use crate::items::item_book::ItemSpec;
use crate::items::item_id::ItemId;
use crate::items::shop::{Shop, ShopPlace};
use crate::orders::order::{Action, Order};
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::Stats;
use crate::stats::loads::load_stats;
use crate::stats::modifier_data::ModifierData;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat_change::StatChange;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::block::Block;
use crate::units::owner::Owner;
use crate::units::team::Team;
use crate::values::declared_name::DeclaredName;
use crate::values::number::Number;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::share::Share;
use crate::values::stat::Stat;

/// A match of a carrier of three inventory slots, of slot kind 0, with 200 of its 500 life
/// lost, its player 1000 gold, and a shop round it that sells: a potion, two to a slot, of one
/// use, that heals 50; a flash of no effect and a cooldown of 1000 ms; and a charm whose
/// carrier holds Might, 10 attack damage.
#[derive(Debug)]
struct Carrier {
    sim: TestMatch,
    unit: StableId,
    drink: ActionId,
    flash: ActionId,
    might: ModifierId,
}

const POTION: u32 = 0;
const FLASH: u32 = 1;
const CHARM: u32 = 2;

impl Carrier {
    fn new() -> Carrier {
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Abilities,
            Capability::Orders,
            Capability::Items,
        ];
        let mut sim = TestMatch::server(&declared, ScriptBudgets::new(ScriptLimits::ROOMY, 1));
        let gold = DeclaredName::new("gold").unwrap();
        Units::name_kinds(&sim.world, &["physical"], &["health"], &["gold"]);
        let attack_damage = Stat::named("attack_damage").unwrap();
        load_stats(
            &mut sim.world,
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
        Stats::load_modifier(&mut sim.world, 0, "might", &might, None);
        let might = Stats::modifier(&sim.world, 0, "might").unwrap();
        let heal = EffectData {
            does: Effecting::Heal {
                amount: Number::Value(Scalar::Int(50)),
            },
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
            let id = Actions::load(&mut sim.world, 0, name, data, None, 1).unwrap();
            EffectLists::load(&mut sim.world, id, 0, data);
            id
        };
        let (drink, flash) = (load("drink", &drink), load("flash", &flash));
        Carrier::stock(&mut sim, gold, [drink, flash], might);
        let kind = SlotKind::new(0);
        let mut slots = ActionSlots::new([]);
        slots.add_empty(kind, 3);
        let mut pools = Pools::life(Num::int(500));
        pools.take(PoolId::FIRST, Num::int(200));
        let at = Position::new(Vec3::new(Num::ZERO, Num::ZERO, Num::ZERO)).unwrap();
        let three = NonZeroU8::new(3).unwrap();
        let unit = sim.spawn(
            at,
            (
                (Owner::new(PlayerSlot::new(0)), Team::new(0), pools),
                (OnDeath::Stay, RecentAttackers::default()),
                (Modifiers::default(), ModifierClocks::default()),
                (slots, Inventory::new(three, kind)),
            ),
        );
        Carrier {
            sim,
            unit,
            drink,
            flash,
            might,
        }
    }

    /// Gives `sim` the item book, the shop at the origin, and player 0's 1000 of `gold`.
    fn stock(
        sim: &mut TestMatch,
        gold: DeclaredName,
        [drink, flash]: [ActionId; 2],
        might: ModifierId,
    ) {
        let resource = ResourceId::named(&[gold], "gold").unwrap();
        let item =
            |cost: i64, stack: u32, uses: Option<u32>, action, modifiers: Vec<ModifierId>| {
                ItemSpec {
                    cost: vec![ResourceAmount {
                        resource,
                        amount: cost,
                    }],
                    components: Vec::new(),
                    stack: NonZeroU32::new(stack).unwrap(),
                    uses: uses.map(|uses| NonZeroU32::new(uses).unwrap()),
                    modifiers,
                    action,
                }
            };
        sim.world.insert_resource(ItemBook::new(vec![
            item(50, 2, Some(1), Some(drink), Vec::new()),
            item(100, 1, None, Some(flash), Vec::new()),
            item(300, 1, None, None, vec![might]),
        ]));
        let share = Share::deserialize(toml::Value::String("0.5".to_owned())).unwrap();
        let places = vec![ShopPlace {
            team: Team::new(0),
            region: Region::new([Num::int(-5); 2], [Num::int(5); 2]),
        }];
        let sells = [POTION, FLASH, CHARM].map(ItemId::nth).to_vec();
        sim.world
            .insert_resource(Shop::new(sells, resource, places, share));
        let mut resources = PlayerResources::new(1, 1);
        resources.add(PlayerSlot::new(0), resource, 1000).unwrap();
        sim.world.insert_resource(resources);
    }

    /// Runs a tick in which player 0 orders the carrier each of `actions`.
    fn order(&mut self, actions: &[Action]) {
        let orders: Vec<Order> = actions
            .iter()
            .map(|&action| Order::one(self.unit, action))
            .collect();
        let payload = Order::payload(&orders);
        self.sim.world.resource_mut::<TickInputs>().push(TickInput {
            slot: PlayerSlot::new(0),
            payload: &payload,
        });
        self.sim.step();
    }

    fn life(&self) -> Num {
        self.sim
            .get::<Pools>(self.unit)
            .current(PoolId::FIRST)
            .unwrap()
    }

    /// Each slot's item type and count, then its action and the tick it is ready.
    fn carried(&self) -> Vec<Option<(usize, u32)>> {
        let inventory = self.sim.get::<Inventory>(self.unit);
        let carried = |slot: &Option<ItemStack>| {
            slot.map(|carried| (carried.item.index(), carried.count.get()))
        };
        inventory.slots().iter().map(carried).collect()
    }

    fn actions(&self) -> Vec<Option<ActionId>> {
        let slots = self.sim.get::<ActionSlots>(self.unit);
        slots.iter().map(|slot| slot.action).collect()
    }

    /// How many instances of Might the carrier holds, each from itself as a passive.
    fn mights(&self) -> usize {
        let modifiers = self.sim.get::<Modifiers>(self.unit);
        modifiers
            .iter()
            .filter(|instance| {
                instance.id == self.might
                    && instance.source == Some(self.unit)
                    && instance.lifetime.held_by(Hold::Passive)
            })
            .count()
    }
}

const fn buy(item: u32) -> Action {
    Action::Buy {
        item: ItemId::nth(item),
    }
}

const fn cast(slot: u8) -> Action {
    Action::Slot {
        slot,
        target: ActionTarget::None,
    }
}

#[test]
fn an_items_action_sits_in_its_slot_and_spends_its_uses_in_the_use_group() {
    let mut carrier = Carrier::new();
    let drink = Some(carrier.drink);
    // Two potions stack in slot 0, which holds their action; the others hold none.
    carrier.order(&[buy(POTION), buy(POTION)]);
    assert_eq!(carrier.carried(), [Some((0, 2)), None, None]);
    assert_eq!(carrier.actions(), [drink, None, None]);
    // A restored inventory holds stacks its book allows: a potion of its one use, two at most to
    // a slot, and a flash with no uses.
    let restored = |item: u32, count: u32, uses: Option<u32>| {
        let mut inventory = Inventory::new(NonZeroU8::new(1).unwrap(), SlotKind::new(0));
        let stack = ItemStack {
            item: ItemId::nth(item),
            count: NonZeroU32::new(count).unwrap(),
            uses: uses.map(|uses| NonZeroU32::new(uses).unwrap()),
        };
        inventory.restore(0, stack);
        let world = &carrier.sim.world;
        inventory.check(world, carrier.sim.entity(carrier.unit))
    };
    assert!(restored(POTION, 2, Some(1)) && restored(FLASH, 1, None));
    assert!(!restored(POTION, 3, Some(1)));
    assert!(!restored(POTION, 1, Some(2)));
    assert!(!restored(POTION, 1, None));
    assert!(!restored(FLASH, 1, Some(1)));
    assert!(!restored(9, 1, None));
    // One resolves as its order comes, and heals 50 of the 200 lost: one potion left.
    carrier.order(&[cast(0)]);
    assert_eq!(
        (carrier.life(), carrier.carried()),
        (Num::int(350), vec![Some((0, 1)), None, None])
    );
    // A stun blocks `use`: the drink waits as its order, and resolves the tick the stun ends,
    // spending the last potion, which leaves the slot and its action empty.
    carrier.sim.set_blocks(
        carrier.unit,
        &[Block::Move, Block::Attack, Block::Cast, Block::Use],
    );
    carrier.order(&[cast(0)]);
    assert_eq!(
        (carrier.life(), carrier.carried()),
        (Num::int(350), vec![Some((0, 1)), None, None])
    );
    carrier.sim.set_blocks(carrier.unit, &[]);
    carrier.sim.step();
    assert_eq!(
        (carrier.life(), carrier.carried()),
        (Num::int(400), vec![None, None, None])
    );
    assert_eq!(carrier.actions(), [None, None, None]);
    // A silence blocks casts alone: a new potion is drunk through it.
    carrier.order(&[buy(POTION)]);
    carrier.sim.set_blocks(carrier.unit, &[Block::Cast]);
    carrier.order(&[cast(0)]);
    assert_eq!(
        (carrier.life(), carrier.carried()),
        (Num::int(450), vec![None, None, None])
    );
}

#[test]
fn an_active_keeps_its_cooldown_across_a_swap_and_a_carried_item_holds_its_modifier_once() {
    let mut carrier = Carrier::new();
    carrier.order(&[buy(FLASH)]);
    carrier.order(&[cast(0)]);
    let ready = carrier
        .sim
        .get::<ActionSlots>(carrier.unit)
        .slot(0)
        .unwrap()
        .ready_at;
    assert!(ready > carrier.sim.now());
    // The flash moves to slot 2 with its cooldown; slot 0 is empty and ready.
    carrier.order(&[Action::Swap { from: 0, to: 2 }]);
    let slots = carrier.sim.get::<ActionSlots>(carrier.unit);
    let held = |at: u8| {
        slots
            .slot(at)
            .map(|slot| (slot.action, slot.ready_at))
            .unwrap()
    };
    assert_eq!(held(2), (Some(carrier.flash), ready));
    assert_eq!(held(0).0, None);
    // A charm holds Might from the carrier, as a passive; a second charm holds it no more often;
    // selling one keeps it, selling both lets it go.
    assert_eq!(carrier.mights(), 0);
    carrier.order(&[buy(CHARM)]);
    assert_eq!(carrier.mights(), 1);
    carrier.order(&[buy(CHARM)]);
    assert_eq!(
        (carrier.carried(), carrier.mights()),
        (vec![Some((2, 1)), Some((2, 1)), Some((1, 1))], 1)
    );
    carrier.order(&[Action::Sell { slot: 0 }]);
    assert_eq!(carrier.mights(), 1);
    carrier.order(&[Action::Sell { slot: 1 }]);
    assert_eq!(carrier.mights(), 0);
}
