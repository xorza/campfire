use std::slice;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::query::{Allow, Has};
use bevy::ecs::schedule::SystemCondition;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::common_conditions::input_just_pressed;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::math::Vec3;
use bevy::state::condition::in_state;
use campfire_actions::{
    ActionBook, ActionSlots, ActionTarget, Aim, SlotKindBook, SlotKinds, UseGroup,
};
use campfire_capabilities::{Directive, Order};
use campfire_interaction::Using;
use campfire_math::Vec3 as SimVec3;
use campfire_net::PendingOrders;
use campfire_orders::OrdersDirective;
use campfire_production::{ProductionDirective, RallyTarget, TrainQueue};
use campfire_progression::ProgressionDirective;
use campfire_sim::{EntityIndex, StableId, Unpredicted};
use campfire_units::{GroundPoint, MoveStep};
use campfire_values::Team;

use crate::dock_click::Docks;
use crate::input_map::{Act, ChordRow, InputMap};
use crate::input_mode::InputMode;
use crate::own_player::OwnPlayer;
use crate::pointer::{Pointed, Pointer};
use crate::selection::{Selection, SelectionSystems};
use crate::view::float_num;

/// Turns the player's clicks and keys into orders: a right click on a node or a drop-off orders
/// each selected unit that gathers or returns there by its own gather (02-engine-core.md, A
/// right-click on a node gathers); else a right click on a unit the own avatar may use,
/// with the avatar selected, orders the avatar alone to use it; else, with producers selected and
/// no walker, it sets their rally point at the unit under the cursor or the ground point
/// (02-engine-core.md, A command panel from the actions); else it orders the selection, as one
/// group order, to attack an enemy under the cursor, and anywhere else but the UI to walk there,
/// and the stop chord stops it (02-engine-core.md, A selection of the player's own units); while
/// the player places a build, the right click is the preview's. The key map's cast chords cast the
/// avatar's abilities, the slots of the kinds whose ranks the player learns, in order, and its
/// learn chords learn their next rank, as the item chords find the inventory's slots by its kind
/// and cast their actions. A chord aims as its slot's action takes: at the unit under the cursor,
/// at the ground point under it, or at nothing.
#[derive(Debug)]
pub(crate) struct OrdersPlugin;

/// The systems that turn clicks and keys into orders, which the build preview reads the clicks
/// after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct OrderSystems;

/// Whether a unit walks, and whether it trains units.
type Movers<'w, 's> = Query<'w, 's, (Has<MoveStep>, Has<TrainQueue>), Allow<Unpredicted>>;

/// A unit's slots, and the slot kind of its inventory's, if it carries one.
type Casters<'w, 's> =
    Query<'w, 's, (&'static ActionSlots, Option<&'static UseGroup>), Allow<Unpredicted>>;

impl Plugin for OrdersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                Self::click.run_if(
                    input_just_pressed(MouseButton::Right).and_then(in_state(InputMode::Select)),
                ),
                Self::cast.run_if(Self::cast_pressed),
                Self::stop.run_if(InputMap::on(Act::Stop)),
            )
                .in_set(OrderSystems)
                .after(SelectionSystems),
        );
    }
}

impl OrdersPlugin {
    fn click(
        pointer: Pointer<'_, '_>,
        own: OwnPlayer<'_, '_>,
        (selection, using): (Res<'_, Selection>, Using<'_, '_>),
        (index, movers, docks): (Res<'_, EntityIndex>, Movers<'_, '_>, Docks<'_, '_>),
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if pointer.is_over_ui() {
            return;
        }
        let Some(point) = pointer.ground() else {
            return;
        };
        let under = pointer.unit_under(selection.get());
        let gathers = under.map(|unit| docks.orders(selection.get(), unit.id));
        if let Some(gathers) = gathers.filter(|gathers| !gathers.is_empty()) {
            for order in gathers {
                orders.push(order);
            }
            return;
        }
        let avatar = own
            .avatar()
            .filter(|avatar| selection.get().contains(&avatar.id));
        let uses = |object| avatar.is_some_and(|avatar| using.of(avatar.entity, object).is_some());
        let held = selection.get().iter().filter_map(|&unit| {
            let entity = index.get(unit)?;
            let (walks, trains) = movers.get(entity).ok()?;
            Some(Mover { walks, trains })
        });
        let click = Click {
            under,
            team: own.team(),
            point,
            selected: Selected::of(held),
        };
        let order = match Self::click_directive(click, uses) {
            Some(directive @ Directive::Orders(OrdersDirective::Use { .. })) => {
                avatar.map(|avatar| Order::one(avatar.id, directive))
            }
            directive => directive.and_then(|directive| selection.order(directive)),
        };
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
        let point = ground.and_then(position);
        let row = |row, key_use| {
            Self::abilities(slots, &kinds.0)
                .zip(input.row(row))
                .map(move |(slot, chord)| (slot, chord, key_use))
        };
        let abilities =
            row(ChordRow::Cast, KeyUse::Cast).chain(row(ChordRow::Learn, KeyUse::Learn));
        // An item chord past the inventory's slots casts nothing.
        let inventory = group.map(|group| {
            let kind = group.kind();
            let count = slots.iter().filter(|held| held.kind == kind).count();
            (slots.first_of(kind), count)
        });
        let items = inventory.into_iter().flat_map(|(first, count)| {
            let chords = input.row(ChordRow::Items);
            (first..)
                .zip(&chords[..count.min(chords.len())])
                .map(|(slot, chord)| (slot, chord, KeyUse::Cast))
        });
        for (slot, &chord, key_use) in abilities.chain(items) {
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
                Self::slot_directive(slot, key_use, target),
            ));
        }
    }

    fn stop(selection: Res<'_, Selection>, mut orders: ResMut<'_, PendingOrders>) {
        if let Some(order) = selection.order(OrdersDirective::Stop) {
            orders.push(order);
        }
    }

    /// What `click` orders: a use of the unit under the cursor that `uses` says the avatar may
    /// use; for a selection of producers, a rally point at the unit or the point; an attack on
    /// the unit when it is of another team than the player's; else a move to the point. None far
    /// off any map.
    fn click_directive(click: Click, uses: impl Fn(StableId) -> bool) -> Option<Directive> {
        let Click {
            under,
            team,
            point,
            selected,
        } = click;
        let ground = || {
            let x = float_num::to_num(point.x)?;
            let z = float_num::to_num(point.z)?;
            Some([x, z])
        };
        let directive = match under {
            Some(unit) if uses(unit.id) => OrdersDirective::Use { target: unit.id }.into(),
            _ if selected == Selected::Producers => {
                let target = if let Some(unit) = under {
                    RallyTarget::Unit(unit.id)
                } else {
                    let [x, z] = ground()?;
                    RallyTarget::Point(GroundPoint { x, z })
                };
                let target = Some(target);
                ProductionDirective::Rally { target }.into()
            }
            Some(unit) if team.is_some_and(|team| unit.team != team) => {
                OrdersDirective::Attack { target: unit.id }.into()
            }
            _ => {
                let [x, z] = ground()?;
                OrdersDirective::Move {
                    to: GroundPoint { x, z },
                }
                .into()
            }
        };
        Some(directive)
    }

    /// The slots of `slots` that hold abilities, of the kinds of `kinds` whose ranks the player
    /// learns, in their order.
    fn abilities<'a>(
        slots: &'a ActionSlots,
        kinds: &'a SlotKinds,
    ) -> impl Iterator<Item = u8> + 'a {
        let learned = slots
            .indexed()
            .filter(|(_, held)| kinds.first_rank(held.kind).is_none());
        learned.map(|(slot, _)| slot)
    }

    /// Whether a chord of a cast, a learn or an item went down this frame.
    fn cast_pressed(keys: Res<'_, ButtonInput<KeyCode>>, input: Res<'_, InputMap>) -> bool {
        [ChordRow::Cast, ChordRow::Learn, ChordRow::Items]
            .into_iter()
            .flat_map(|row| input.row(row))
            .any(|&chord| InputMap::chord_pressed(chord, &keys))
    }

    /// What a key of `slot` orders as `key_use` says: a learn, or a cast at `target`.
    const fn slot_directive(slot: u8, key_use: KeyUse, target: ActionTarget) -> Directive {
        match key_use {
            KeyUse::Learn => Directive::Progression(ProgressionDirective::Learn { slot }),
            KeyUse::Cast => Directive::Orders(OrdersDirective::Slot { slot, target }),
        }
    }
}

/// A right click: the unit under the cursor, the player's team, the ground point under the
/// cursor, and what the selection holds.
#[derive(Debug, Clone, Copy)]
struct Click {
    under: Option<Pointed>,
    team: Option<Team>,
    point: Vec3,
    selected: Selected,
}

/// What a selection holds, as a right click reads it: producers and no walker, whose rally point
/// a click sets, or any other units, which a click moves or sends to attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selected {
    Producers,
    Movers,
}

/// A selected unit, as a right click reads it: whether it walks, and whether it trains units.
#[derive(Debug, Clone, Copy)]
struct Mover {
    walks: bool,
    trains: bool,
}

impl Selected {
    /// What a selection of `units` holds: producers when one trains and none walks.
    fn of(units: impl IntoIterator<Item = Mover>) -> Self {
        let (mut walks, mut trains) = (false, false);
        for unit in units {
            walks |= unit.walks;
            trains |= unit.trains;
        }
        if trains && !walks {
            Self::Producers
        } else {
            Self::Movers
        }
    }
}

/// What a slot's key does: with Ctrl held, an ability key learns its ability; else a key casts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyUse {
    Learn,
    Cast,
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

/// A point of the ground as a sim position, each axis as `float_num::to_num` takes it;
/// `None` past the bound.
fn position(point: Vec3) -> Option<SimVec3> {
    let axes = [point.x, point.y, point.z].map(float_num::to_num);
    let [Some(x), Some(y), Some(z)] = axes else {
        return None;
    };
    let point = SimVec3::new(x, y, z);
    point.within_bound().then_some(point)
}

#[cfg(test)]
mod tests;
