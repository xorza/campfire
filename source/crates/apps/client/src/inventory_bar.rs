use std::fmt::Write as _;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::observer::On;
use bevy::ecs::query::Allow;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy::picking::events::{PointerClick, PointerDragDrop};
use bevy::picking::pointer::PointerButton;
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, FlexDirection, JustifyContent, Node, PositionType, UiRect, Val};
use campfire_actions::{ActionSlots, SlotTimes, UseGroup};
use campfire_capabilities::Order;
use campfire_common::Tick;
use campfire_items::{Inventory, ItemStack, ItemsDirective, Shopping};
use campfire_net::PendingOrders;
use campfire_sim::{MatchRate, SimTick, Unpredicted};

use crate::input_map::{InputMap, Row};
use crate::item_names::ItemTexts;
use crate::own_player::OwnPlayer;
use crate::ui_style::{self, BACK, TEXT};
use crate::view::ViewSystems;

/// The inventory bar, always shown : each of the own avatar's inventory slots,
/// with its key, its item's name, its stack or uses, and its cooldown. A right click on a slot
/// sells its item at the nearest shop unit, when it serves the avatar; a slot dragged onto
/// another swaps them.
#[derive(Debug)]
pub(crate) struct InventoryBarPlugin;

/// A slot of the bar: the inventory slot it shows, from 0.
#[derive(Component, Debug, Clone, Copy)]
struct BarSlot(u8);

/// The own avatar's inventory, its slot kind, its slots' times and its slots.
type Carriers<'w, 's> = Query<
    'w,
    's,
    (
        &'static Inventory,
        Option<&'static UseGroup>,
        Option<&'static SlotTimes>,
        Option<&'static ActionSlots>,
    ),
    Allow<Unpredicted>,
>;

impl Plugin for InventoryBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::spawn);
        app.add_systems(Update, Self::show.after(ViewSystems));
    }
}

impl InventoryBarPlugin {
    fn spawn(input: Res<'_, InputMap>, mut commands: Commands<'_, '_>) {
        let bar = Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(6.0),
            ..Node::default()
        };
        let bar = commands.spawn(bar).id();
        // A slot for each chord that casts an inventory slot's action.
        for at in (0..=u8::MAX).take(input.row(Row::Items).len()) {
            let slot = Node {
                width: Val::Px(120.0),
                height: Val::Px(64.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(4.0)),
                ..Node::default()
            };
            commands
                .spawn((
                    BarSlot(at),
                    slot,
                    BackgroundColor(BACK),
                    Text::default(),
                    ui_style::font(),
                    TextColor(TEXT),
                    ChildOf(bar),
                ))
                .observe(Self::sell)
                .observe(Self::swap);
        }
    }

    /// Writes each slot's line: its key, and the item it holds, with its stack or uses and its
    /// cooldown in whole seconds; its key alone when it is empty or the avatar has fewer slots.
    fn show(
        own: OwnPlayer<'_, '_>,
        carriers: Carriers<'_, '_>,
        (names, tick, rate): (Res<'_, ItemTexts>, Res<'_, SimTick>, Res<'_, MatchRate>),
        input: Res<'_, InputMap>,
        (slots, mut texts): (Query<'_, '_, (Entity, &BarSlot)>, Query<'_, '_, &mut Text>),
        mut line: Local<'_, String>,
    ) {
        let avatar = own.avatar();
        let carrier = avatar.and_then(|avatar| carriers.get(avatar.entity).ok());
        let first = carrier.and_then(|(_, group, _, held)| Some(held?.first_of(group?.kind())));
        let clock = BarClock {
            now: tick.start(),
            hz: rate.hz().get(),
        };
        for (entity, &BarSlot(at)) in &slots {
            line.clear();
            let chord = input.row(Row::Items)[usize::from(at)];
            write!(line, "{}", chord.label()).expect("a String takes any text");
            let carried = carrier
                .and_then(|(inventory, ..)| inventory.slots().get(usize::from(at)).copied())
                .flatten();
            if let Some(carried) = carried {
                let times = carrier.and_then(|(_, _, times, _)| times);
                let ready = first
                    .and_then(|first| times?.slot(first + at))
                    .map(|time| time.ready_at);
                let slot = ShownSlot {
                    name: names.of(carried.item),
                    carried,
                    ready,
                };
                write_slot(&mut line, slot, clock);
            }
            ui_style::set_text(&mut texts, entity, &line);
        }
    }

    /// A right click on a slot sells its item at the nearest shop unit whose buyers select the
    /// avatar, which the sim refuses unless it serves the avatar.
    fn sell(
        click: On<'_, '_, PointerClick>,
        slots: Query<'_, '_, &BarSlot>,
        own: OwnPlayer<'_, '_>,
        shopping: Shopping<'_, '_>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if click.button != PointerButton::Secondary {
            return;
        }
        let (Ok(&BarSlot(slot)), Some(avatar)) = (slots.get(click.entity), own.avatar()) else {
            return;
        };
        let Some(shop) = shopping.nearest_for(avatar.id) else {
            return;
        };
        orders.push(Order::one(avatar.id, ItemsDirective::Sell { slot, shop }));
    }

    /// A slot dropped onto another swaps the two.
    fn swap(
        drop: On<'_, '_, PointerDragDrop>,
        slots: Query<'_, '_, &BarSlot>,
        own: OwnPlayer<'_, '_>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let (Ok(&BarSlot(to)), Ok(&BarSlot(from)), Some(avatar)) = (
            slots.get(drop.entity),
            slots.get(drop.dropped),
            own.avatar(),
        ) else {
            return;
        };
        if from != to {
            orders.push(Order::one(avatar.id, ItemsDirective::Swap { from, to }));
        }
    }
}

/// A slot of the bar as it shows: the item it carries, its name, and the tick its action is ready
/// at, if it has one.
#[derive(Debug, Clone, Copy)]
struct ShownSlot<'n> {
    name: &'n str,
    carried: ItemStack,
    ready: Option<Tick>,
}

/// The match's clock as the bar reads it: the tick now, and the ticks a second.
#[derive(Debug, Clone, Copy)]
struct BarClock {
    now: Tick,
    hz: u32,
}

/// Writes after a slot's key the item `slot` carries: its name, its stack past one, its uses
/// left, and, by `clock`, the whole seconds until its action is ready, rounded up.
fn write_slot(line: &mut String, slot: ShownSlot<'_>, clock: BarClock) {
    let ShownSlot {
        name,
        carried,
        ready,
    } = slot;
    write!(line, " {name}").expect("a String takes any text");
    if carried.count.get() > 1 {
        write!(line, " x{}", carried.count).expect("a String takes any text");
    }
    if let Some(uses) = carried.uses {
        write!(line, " ({uses})").expect("a String takes any text");
    }
    let BarClock { now, hz } = clock;
    if let Some(ready) = ready
        && ready > now
    {
        let left = ready.get() - now.get();
        let seconds = left.div_ceil(u64::from(hz));
        write!(line, "\n{seconds} s").expect("a String takes any text");
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use bevy::ecs::system::{IntoSystem, System};
    use bevy::ecs::world::World;
    use campfire_values::ItemId;

    use super::*;

    #[test]
    fn the_bar_takes_no_conflicting_access() {
        // Bevy checks that a system's queries do not conflict as it initializes the system.
        let mut world = World::new();
        let mut bar = IntoSystem::into_system(InventoryBarPlugin::show);
        bar.initialize(&mut world);
    }

    #[test]
    fn a_slot_shows_its_item_its_stack_or_uses_and_its_seconds_left() {
        let line = |carried, ready, clock| {
            let mut line = String::from("2");
            let slot = ShownSlot {
                name: "Health Potion",
                carried,
                ready,
            };
            write_slot(&mut line, slot, clock);
            line
        };
        let potions = ItemStack {
            item: ItemId::nth(0),
            count: NonZeroU32::new(3).unwrap(),
            uses: Some(NonZeroU32::new(1).unwrap()),
        };
        // At 30 ticks a second, ready 31 ticks on: 2 s, rounded up; ready now: no cooldown.
        let clock = BarClock {
            now: Tick::new(100),
            hz: 30,
        };
        assert_eq!(
            line(potions, Some(Tick::new(131)), clock),
            "2 Health Potion x3 (1)\n2 s"
        );
        assert_eq!(
            line(potions, Some(Tick::new(100)), clock),
            "2 Health Potion x3 (1)"
        );
        let sword = ItemStack {
            count: NonZeroU32::MIN,
            uses: None,
            ..potions
        };
        assert_eq!(line(sword, None, clock), "2 Health Potion");
    }
}
