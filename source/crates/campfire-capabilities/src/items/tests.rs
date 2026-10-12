use campfire_sim::internals::SimWorldInternals;
use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};

use campfire_common::{PlayerSlot, Tick};
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, SimComponent, SimWorld};
use serde::Deserialize;

use super::*;
use crate::actions::action_data::ActionData;
use crate::actions::action_target::ActionTarget;
use crate::actions::actions_world::test_world::ActionsWorldTest;
use crate::actions::effect_data::{EffectData, EffectTo, Effecting, HealFields};
use crate::actions::slot_kind::SlotKind;
use crate::actions::targeting::Targeting;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::geometry::bounds::Bounds;
use crate::items::inventory::ItemStack;
use crate::items::item_book::ItemSpec;
use crate::items::item_id::ItemId;
use crate::items::items_world::test_world::{Carrier, ItemsWorldTest};
use crate::items::shop::{Shop, ShopPlace};
use crate::orders::order::Action;
use crate::players::players_world::PlayersWorldCrate;
use crate::players::players_world::internals::PlayersWorldInternals;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::release::r#match::Match;
use crate::release::release_world::test_world::ReleaseWorldTest;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::life::Life;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_data::ModifierData;
use crate::stats::modifiers::Modifiers;
use crate::stats::stat_change::StatChange;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::stats::stats_world::internals::StatsWorldInternals;
use crate::stats::stats_world::test_world::StatsWorldTest;
use crate::units::action_id::ActionId;
use crate::units::block::Block;
use crate::units::modifier_id::ModifierId;
use crate::units::owner::Owner;
use crate::units::team::Team;
use crate::units::units_world::test_world::UnitsWorldTest;
use crate::values::declared_name::DeclaredName;
use crate::values::number::Number;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::share::Share;
use crate::values::stat::Stat;

const POTION: u32 = 0;
const FLASH: u32 = 1;
const CHARM: u32 = 2;

/// A match of a sim of three inventory slots, of slot kind 0, with 200 of its 500 life
/// lost, its player 1000 gold, and a shop round it that sells: a potion, two to a slot, of one
/// use, that heals 50; a flash of no effect and a cooldown of 1000 ms; and a charm whose
/// sim holds Might, 10 attack damage.
fn carrier_match() -> Match {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
        Capability::Items,
    ];
    let mut game = Match::server(&declared, ScriptBudgets::new(ScriptLimits::ROOMY, 1));
    let gold = DeclaredName::new("gold").unwrap();
    game.name_kinds(&["physical"], &["health"], &["gold"]);
    let attack_damage = Stat::named("attack_damage").unwrap();
    game.load_stat_rules(&BTreeMap::from([(
        attack_damage.clone(),
        StatRule::default(),
    )]));
    let change = StatChange {
        op: StatOp::Add,
        value: Number::Value(Scalar::Int(10)),
    };
    let might = ModifierData {
        stats: BTreeMap::from([(attack_damage, change)]),
        ..ModifierData::default()
    };
    game.load_modifier(0, "might", &might, None);
    let might = game.modifier_named(0, "might").unwrap();
    let heal = EffectData {
        does: Effecting::Heal(HealFields {
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
        let id = game.load_action(0, name, data, None, 1).unwrap();
        game.load_effect_lists(id, 0, data);
        id
    };
    let (drink, flash) = (load("drink", &drink), load("flash", &flash));
    stock(&mut game, gold, [drink, flash], might);
    let kind = SlotKind::new(0);
    let slots = ActionSlots::new([]).with_empty(kind, 3).timed();
    let mut life = Life::full(Num::int(500));
    life.take(Num::int(200));
    let pos = Position::new(Vec3::new(Num::ZERO, Num::ZERO, Num::ZERO)).unwrap();
    let three = NonZeroU8::new(3).unwrap();
    let id = game.spawn_unit(
        pos,
        Team::new(0),
        (
            (Owner::new(PlayerSlot::new(0)), life),
            (OnDeath::Stay, RecentAttackers::default()),
            (Modifiers::default(), ModifierClocks::default()),
            (slots, Inventory::new(three, kind)),
        ),
    );
    game.insert_resource(Carrier {
        id,
        drink,
        flash,
        might,
    });
    game
}

/// Gives `game` the item book, the shop at the origin, and player 0's 1000 of `gold`.
fn stock(game: &mut Match, gold: DeclaredName, [drink, flash]: [ActionId; 2], might: ModifierId) {
    let resource = ResourceId::named(&[gold], "gold").unwrap();
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
    game.insert_resource(ItemBook::new(vec![
        item(50, 2, Some(1), Some(drink), Vec::new()),
        item(100, 1, None, Some(flash), Vec::new()),
        item(300, 1, None, None, vec![might]),
    ]));
    let share = Share::deserialize(toml::Value::String("0.5".to_owned())).unwrap();
    let places = vec![ShopPlace {
        team: Team::new(0),
        region: Bounds::new([Num::int(-5); 2], [Num::int(5); 2]).unwrap(),
    }];
    let sells = [POTION, FLASH, CHARM].map(ItemId::nth).to_vec();
    game.insert_resource(Shop::new(sells, resource, places, share));
    game.spawn_players(1, 1);
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(resource, 1000)
        .unwrap();
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
    let mut game = carrier_match();
    let carrier = game.carrier();
    let drink = Some(carrier.drink);
    // Two potions stack in slot 0, which holds their action; the others hold none.
    game.order_carrier(&[buy(POTION), buy(POTION)]);
    assert_eq!(game.carried_items(), [Some((0, 2)), None, None]);
    assert_eq!(game.carried_actions(), [drink, None, None]);
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
        inventory.check(&game, game.unit_entity(carrier.id))
    };
    assert!(restored(POTION, 2, Some(1)) && restored(FLASH, 1, None));
    assert!(!restored(POTION, 3, Some(1)));
    assert!(!restored(POTION, 1, Some(2)));
    assert!(!restored(POTION, 1, None));
    assert!(!restored(FLASH, 1, Some(1)));
    assert!(!restored(9, 1, None));
    // One resolves as its order comes, and heals 50 of the 200 lost: one potion left.
    game.order_carrier(&[cast(0)]);
    assert_eq!(
        (game.carrier_life(), game.carried_items()),
        (Num::int(350), vec![Some((0, 1)), None, None])
    );
    // A stun blocks `use`: the drink waits as its order, and resolves the tick the stun ends,
    // spending the last potion, which leaves the slot and its action empty.
    game.set_blocks(
        carrier.id,
        &[Block::Move, Block::Attack, Block::Cast, Block::Use],
    );
    game.order_carrier(&[cast(0)]);
    assert_eq!(
        (game.carrier_life(), game.carried_items()),
        (Num::int(350), vec![Some((0, 1)), None, None])
    );
    game.set_blocks(carrier.id, &[]);
    game.step();
    assert_eq!(
        (game.carrier_life(), game.carried_items()),
        (Num::int(400), vec![None, None, None])
    );
    assert_eq!(game.carried_actions(), [None, None, None]);
    // A silence blocks casts alone: a new potion is drunk through it.
    game.order_carrier(&[buy(POTION)]);
    game.set_blocks(carrier.id, &[Block::Cast]);
    game.order_carrier(&[cast(0)]);
    assert_eq!(
        (game.carrier_life(), game.carried_items()),
        (Num::int(450), vec![None, None, None])
    );
}

#[test]
fn an_active_keeps_its_cooldown_across_a_swap_and_a_carried_item_holds_its_modifier_once() {
    let mut game = carrier_match();
    let carrier = game.carrier();
    game.order_carrier(&[buy(FLASH)]);
    game.order_carrier(&[cast(0)]);
    let ready = game.part::<SlotTimes>(carrier.id).slot(0).unwrap().ready_at;
    assert!(ready > game.now());
    // The flash moves to slot 2 with its cooldown; slot 0 is empty and ready.
    game.order_carrier(&[Action::Swap { from: 0, to: 2 }]);
    let slots = game.part::<ActionSlots>(carrier.id);
    let times = game.part::<SlotTimes>(carrier.id);
    let held = |at: u8| {
        let time = times.slot(at).unwrap();
        (slots.slot(at).unwrap().action, time.ready_at)
    };
    assert_eq!(held(2), (Some(carrier.flash), ready));
    assert_eq!(held(0), (None, Tick::ZERO));
    // A charm holds Might from the sim, as a passive; a second charm holds it no more often;
    // selling one keeps it, selling both lets it go.
    assert_eq!(game.mights(), 0);
    game.order_carrier(&[buy(CHARM)]);
    assert_eq!(game.mights(), 1);
    game.order_carrier(&[buy(CHARM)]);
    assert_eq!(
        (game.carried_items(), game.mights()),
        (vec![Some((2, 1)), Some((2, 1)), Some((1, 1))], 1)
    );
    game.order_carrier(&[Action::Sell { slot: 0 }]);
    assert_eq!(game.mights(), 1);
    game.order_carrier(&[Action::Sell { slot: 1 }]);
    assert_eq!(game.mights(), 0);
}
