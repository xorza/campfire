use std::slice;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::query::Allow;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::state::state::State;
use campfire_actions::{
    ActionBook, ActionSlots, ActionTarget, Aim, SlotKindBook, SlotKinds, UseGroup,
};
use campfire_capabilities::{Directive, Order};
use campfire_math::Vec3 as SimVec3;
use campfire_net::PendingOrders;
use campfire_orders::OrdersDirective;
use campfire_sim::{StableId, Unpredicted};
use campfire_values::{Relation, Team};

use crate::input_map::{Act, ChordRow, InputMap};
use crate::input_mode::InputMode;
use crate::own_player::OwnPlayer;
use crate::pointer::Pointer;
use crate::right_click::{ClickAt, ClickSystems, RightClick};
use crate::selection::{Selection, SelectionSystems};
use crate::view::float_num;

/// Turns the player's clicks and keys into orders: reads each right click on the ground, which the
/// click modules of the mode's capabilities look at first, and orders what no module took, the
/// selection, as one group order, to attack an enemy under the cursor, and anywhere else but the
/// UI to walk there; the stop chord stops it (02-engine-core.md, A selection of the player's own
/// units); while the player places a build, the right click is the preview's. The key map's cast
/// chords cast the avatar's abilities, the slots of the kinds whose ranks the player learns, in
/// order, as the item chords find the inventory's slots by its kind and cast their actions. A
/// chord aims as its slot's action takes: at the unit under the cursor, at the ground point under
/// it, or at nothing.
#[derive(Debug)]
pub(crate) struct OrdersPlugin;

/// The systems that turn clicks and keys into orders, which the build preview reads the clicks
/// after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct OrderSystems;

/// A unit's slots, and the slot kind of its inventory's, if it carries one.
type Casters<'w, 's> =
    Query<'w, 's, (&'static ActionSlots, Option<&'static UseGroup>), Allow<Unpredicted>>;

impl Plugin for OrdersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RightClick>();
        app.configure_sets(
            Update,
            (
                ClickSystems::Read,
                ClickSystems::Gather,
                ClickSystems::Use,
                ClickSystems::Rally,
                ClickSystems::Walk,
            )
                .chain()
                .in_set(OrderSystems),
        );
        app.configure_sets(Update, OrderSystems.after(SelectionSystems));
        app.add_systems(
            Update,
            (
                Self::read_click.in_set(ClickSystems::Read),
                Self::walk.in_set(ClickSystems::Walk),
                Self::cast.run_if(Self::cast_pressed).in_set(OrderSystems),
                Self::stop
                    .run_if(InputMap::on(Act::Stop))
                    .in_set(OrderSystems),
            ),
        );
    }
}

impl OrdersPlugin {
    /// Reads this frame's right click on the ground, in the select mode, off the UI: none else.
    fn read_click(
        pointer: Pointer<'_, '_>,
        selection: Res<'_, Selection>,
        (buttons, mode): (Res<'_, ButtonInput<MouseButton>>, Res<'_, State<InputMode>>),
        mut click: ResMut<'_, RightClick>,
    ) {
        let clicked = buttons.just_pressed(MouseButton::Right)
            && *mode.get() == InputMode::Select
            && !pointer.is_over_ui();
        let at = clicked
            .then(|| pointer.ground())
            .flatten()
            .map(|point| ClickAt {
                under: pointer.unit_under(selection.get()),
                point,
            });
        click.set(at);
    }

    /// Orders the selection by the click no module took: to attack an enemy under the cursor, or
    /// to walk to the ground point.
    fn walk(
        own: OwnPlayer<'_, '_>,
        selection: Res<'_, Selection>,
        mut click: ResMut<'_, RightClick>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(at) = click.get() else {
            return;
        };
        click.take();
        let relation = |team| own.relation_to(team);
        let order =
            Self::click_directive(at, relation).and_then(|directive| selection.order(directive));
        if let Some(order) = order {
            orders.push(order);
        }
    }

    fn cast(
        (keys, input): (Res<'_, ButtonInput<KeyCode>>, Res<'_, InputMap>),
        pointer: Pointer<'_, '_>,
        own: OwnPlayer<'_, '_>,
        (book, kinds, avatars): (Res<'_, ActionBook>, Res<'_, SlotKindBook>, Casters<'_, '_>),
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(avatar) = own.avatar() else {
            return;
        };
        let Ok((slots, group)) = avatars.get(avatar.entity) else {
            return;
        };
        let ground = pointer.ground();
        let under = pointer
            .unit_under(slice::from_ref(&avatar.id))
            .map(|unit| unit.id);
        let point = ground.and_then(float_num::position);
        let row = |row| Self::abilities(slots, &kinds.0).zip(input.row(row));
        let abilities = row(ChordRow::Cast);
        // An item chord past the inventory's slots casts nothing.
        let inventory = group.map(|group| {
            let kind = group.kind();
            let count = slots.iter().filter(|held| held.kind == kind).count();
            (slots.first_of(kind), count)
        });
        let items = inventory.into_iter().flat_map(|(first, count)| {
            let chords = input.row(ChordRow::Items);
            (first..).zip(&chords[..count.min(chords.len())])
        });
        for (slot, &chord) in abilities.chain(items) {
            if !InputMap::chord_pressed(chord, &keys) {
                continue;
            }
            let action = slots.slot(slot).and_then(|held| held.action);
            let aim = action
                .and_then(|action| book.get(action))
                .map(|action| &action.aim);
            let target = Takes::of(aim).target(under, point);
            orders.push(Order::one(
                avatar.id,
                OrdersDirective::Slot { slot, target },
            ));
        }
    }

    fn stop(selection: Res<'_, Selection>, mut orders: ResMut<'_, PendingOrders>) {
        if let Some(order) = selection.order(OrdersDirective::Stop) {
            orders.push(order);
        }
    }

    /// What a click at `at` orders a selection of a player whose side regards a team as
    /// `relation` says: an attack on the unit under the cursor when its team is hostile, else a
    /// walk to the point, as a click on a neutral unit walks to it; none far off any map.
    fn click_directive(
        at: ClickAt,
        relation: impl Fn(Team) -> Option<Relation>,
    ) -> Option<Directive> {
        let directive = match at.under {
            Some(unit) if relation(unit.team) == Some(Relation::Hostile) => {
                OrdersDirective::Attack { target: unit.id }.into()
            }
            _ => OrdersDirective::Move {
                to: float_num::ground_point(at.point)?,
            }
            .into(),
        };
        Some(directive)
    }

    /// The slots of `slots` that hold abilities, of the kinds of `kinds` whose ranks the player
    /// learns, in their order.
    pub(crate) fn abilities<'a>(
        slots: &'a ActionSlots,
        kinds: &'a SlotKinds,
    ) -> impl Iterator<Item = u8> + 'a {
        let learned = slots
            .indexed()
            .filter(|(_, held)| kinds.first_rank(held.kind).is_none());
        learned.map(|(slot, _)| slot)
    }

    /// Whether a chord of a cast or an item went down this frame.
    fn cast_pressed(keys: Res<'_, ButtonInput<KeyCode>>, input: Res<'_, InputMap>) -> bool {
        [ChordRow::Cast, ChordRow::Items]
            .into_iter()
            .flat_map(|row| input.row(row))
            .any(|&chord| InputMap::chord_pressed(chord, &keys))
    }
}

/// What a key's action takes as its target: a unit, a point, a direction's among them, or
/// nothing, as no action does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Takes {
    Unit,
    Point,
    Nothing,
}

impl Takes {
    /// What an action that aims as `aim` says takes; nothing for no action.
    const fn of(aim: Option<&Aim>) -> Self {
        match aim {
            Some(Aim::Unit(_)) => Self::Unit,
            Some(Aim::Point { .. } | Aim::Direction) => Self::Point,
            Some(Aim::None) | None => Self::Nothing,
        }
    }

    /// The target it takes, `under` the unit under the cursor and `point` the ground point under
    /// it: none when the cursor shows none, which the sim refuses of an action that needs one.
    fn target(self, under: Option<StableId>, point: Option<SimVec3>) -> ActionTarget {
        let target = match self {
            Self::Unit => under.map(ActionTarget::Unit),
            Self::Point => point.map(ActionTarget::Point),
            Self::Nothing => None,
        };
        target.unwrap_or(ActionTarget::None)
    }
}

#[cfg(test)]
mod tests;
