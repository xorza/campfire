use std::slice;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::query::{Allow, Has};
use bevy::ecs::schedule::SystemCondition;
use bevy::ecs::schedule::common_conditions::{not, resource_exists};
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::common_conditions::input_just_pressed;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::math::Vec3;
use campfire_actions::{ActionBook, ActionSlots, ActionTarget, Aim, UseGroup};
use campfire_capabilities::{Action, Order};
use campfire_interaction::Using;
use campfire_math::Vec3 as SimVec3;
use campfire_net::PendingOrders;
use campfire_orders::OrdersAction;
use campfire_production::{ProductionAction, RallyTarget, TrainQueue};
use campfire_progression::ProgressionAction;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::MoveStep;
use campfire_values::Team;

use crate::own_player::OwnPlayer;
use crate::placing::Placing;
use crate::pointer::{Pointed, Pointer};
use crate::selection::{Selection, SelectionSystems};
use crate::view::float_num;

/// Turns the player's clicks and keys into orders: a right click on a unit the own avatar may
/// use, with the avatar selected, orders the avatar alone to use it (design 13, D6); else, with
/// producers selected and no walker, it sets their rally point at the unit under the cursor or
/// the ground point (design 18, D7); else it orders the selection, as one group order, to attack
/// an enemy under the cursor, and anywhere else but the UI to walk there, and S stops it (design
/// 17, D7); while the player places a build, the right click is the preview's; Q, W, E and R cast the
/// avatar's abilities of slots 0 to 3, and with Ctrl held learn their next rank; 1 to 6 cast the
/// actions of its inventory's slots. A key aims as its slot's action takes: at the unit under
/// the cursor, at the ground point under it, or at nothing (design 13, D11).
#[derive(Debug)]
pub(crate) struct Orders;

/// The systems that turn clicks and keys into orders, which the build preview reads the clicks
/// after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct OrderSystems;

/// Whether a unit walks, and whether it trains units.
type Movers<'w, 's> = Query<'w, 's, (Has<MoveStep>, Has<TrainQueue>), Allow<Unpredicted>>;

/// A unit's slots, and the slot kind of its inventory's, if it carries one.
type Casters<'w, 's> =
    Query<'w, 's, (&'static ActionSlots, Option<&'static UseGroup>), Allow<Unpredicted>>;

/// The keys that cast, by ability slot.
const CAST_KEYS: [KeyCode; 4] = [KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR];
/// The keys that cast the actions of the inventory's slots, by inventory slot.
const ITEM_KEYS: [KeyCode; 6] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
];
/// The keys that, held, make a cast key learn.
const LEARN_KEYS: [KeyCode; 2] = [KeyCode::ControlLeft, KeyCode::ControlRight];
/// The key that stops.
const STOP_KEY: KeyCode = KeyCode::KeyS;

impl Plugin for Orders {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                Self::click.run_if(
                    input_just_pressed(MouseButton::Right)
                        .and_then(not(resource_exists::<Placing>)),
                ),
                Self::cast.run_if(Self::cast_pressed),
                Self::stop.run_if(input_just_pressed(STOP_KEY)),
            )
                .in_set(OrderSystems)
                .after(SelectionSystems),
        );
    }
}

impl Orders {
    fn click(
        pointer: Pointer<'_, '_>,
        own: OwnPlayer<'_, '_>,
        (selection, using): (Res<'_, Selection>, Using<'_, '_>),
        (index, movers): (Res<'_, EntityIndex>, Movers<'_, '_>),
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if pointer.over_ui() {
            return;
        }
        let Some(point) = pointer.ground() else {
            return;
        };
        let under = pointer.unit_at(point, selection.get());
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
        let order = match Self::click_action(click, uses) {
            Some(action @ Action::Orders(OrdersAction::Use { .. })) => {
                avatar.map(|avatar| Order::one(avatar.id, action))
            }
            action => action.and_then(|action| selection.order(action)),
        };
        if let Some(order) = order {
            orders.push(order);
        }
    }

    fn cast(
        keys: Res<'_, ButtonInput<KeyCode>>,
        pointer: Pointer<'_, '_>,
        own: OwnPlayer<'_, '_>,
        (book, avatars): (Res<'_, ActionBook>, Casters<'_, '_>),
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(avatar) = own.avatar() else {
            return;
        };
        let Ok((slots, group)) = avatars.get(avatar.entity) else {
            return;
        };
        let ground = pointer.ground();
        let under = ground
            .and_then(|point| pointer.unit_at(point, slice::from_ref(&avatar.id)))
            .map(|unit| unit.id);
        let point = ground.and_then(position);
        let ability_use = if keys.any_pressed(LEARN_KEYS) {
            KeyUse::Learn
        } else {
            KeyUse::Cast
        };
        let abilities = (0..)
            .zip(&CAST_KEYS)
            .map(|(slot, key)| (slot, key, ability_use));
        // An item key past the inventory's slots casts nothing.
        let inventory = group.map(|group| {
            let kind = group.kind();
            let count = slots.iter().filter(|held| held.kind == kind).count();
            (slots.first_of(kind), count.min(ITEM_KEYS.len()))
        });
        let items = inventory.into_iter().flat_map(|(first, count)| {
            (first..)
                .zip(&ITEM_KEYS[..count])
                .map(|(slot, key)| (slot, key, KeyUse::Cast))
        });
        for (slot, &key, key_use) in abilities.chain(items) {
            if !keys.just_pressed(key) {
                continue;
            }
            let action = slots.slot(slot).and_then(|held| held.action);
            let aim = action
                .and_then(|action| book.get(action))
                .map(|action| &action.aim);
            let target = Takes::of(aim).target(under, point);
            orders.push(Order::one(
                avatar.id,
                Self::slot_action(slot, key_use, target),
            ));
        }
    }

    fn stop(selection: Res<'_, Selection>, mut orders: ResMut<'_, PendingOrders>) {
        if let Some(order) = selection.order(OrdersAction::Stop) {
            orders.push(order);
        }
    }

    /// What `click` orders: a use of the unit under the cursor that `uses` says the avatar may
    /// use; for a selection of producers, a rally point at the unit or the point; an attack on
    /// the unit when it is of another team than the player's; else a move to the point. None far
    /// off any map.
    fn click_action(click: Click, uses: impl Fn(StableId) -> bool) -> Option<Action> {
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
        let action = match under {
            Some(unit) if uses(unit.id) => OrdersAction::Use { target: unit.id }.into(),
            _ if selected == Selected::Producers => {
                let target = if let Some(unit) = under {
                    RallyTarget::Unit(unit.id)
                } else {
                    let [x, z] = ground()?;
                    RallyTarget::Point { x, z }
                };
                let target = Some(target);
                ProductionAction::Rally { target }.into()
            }
            Some(unit) if team.is_some_and(|team| unit.team != team) => {
                OrdersAction::Attack { target: unit.id }.into()
            }
            _ => {
                let [x, z] = ground()?;
                OrdersAction::Move { x, z }.into()
            }
        };
        Some(action)
    }

    /// Whether a cast key or an item key went down this frame.
    fn cast_pressed(keys: Res<'_, ButtonInput<KeyCode>>) -> bool {
        keys.any_just_pressed(CAST_KEYS) || keys.any_just_pressed(ITEM_KEYS)
    }

    /// What a key of `slot` orders as `key_use` says: a learn, or a cast at `target`.
    const fn slot_action(slot: u8, key_use: KeyUse, target: ActionTarget) -> Action {
        match key_use {
            KeyUse::Learn => Action::Progression(ProgressionAction::Learn { slot }),
            KeyUse::Cast => Action::Orders(OrdersAction::Slot { slot, target }),
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
    Position::from(point).within_bound().then_some(point)
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn a_key_aims_as_its_action_takes() {
        let unit = IdAllocator::default().allocate();
        let point = SimVec3::from((3, 0, -2));
        // An action of a point, or of a direction, takes the point; of none, or no action,
        // nothing.
        let aims = [
            (Aim::Point { clamp: false }, Takes::Point),
            (Aim::Direction, Takes::Point),
            (Aim::None, Takes::Nothing),
        ];
        for (aim, takes) in aims {
            assert_eq!(Takes::of(Some(&aim)), takes, "{aim:?}");
        }
        assert_eq!(Takes::of(None), Takes::Nothing);
        // With a unit and a point under the cursor: the unit, the point, or nothing; with
        // neither under it, nothing.
        for (takes, both) in [
            (Takes::Unit, ActionTarget::Unit(unit)),
            (Takes::Point, ActionTarget::Point(point)),
            (Takes::Nothing, ActionTarget::None),
        ] {
            assert_eq!(takes.target(Some(unit), Some(point)), both, "{takes:?}");
            assert_eq!(takes.target(None, None), ActionTarget::None, "{takes:?}");
        }
        // A ground point, exactly, its height among its axes: 0.1 in `f32` is 13,421,773 · 2⁻²⁷,
        // which rounds to the step 1,677,722 of 2⁻²⁴, not a millimeter's multiple.
        assert_eq!(
            position(Vec3::new(3.0, 0.0, -2.0)),
            Some(SimVec3::from((3, 0, -2)))
        );
        let tenth = position(Vec3::new(0.1, 0.0, 0.0)).unwrap();
        assert_eq!(tenth.x.to_bits(), 1_677_722);
        assert_eq!(position(Vec3::new(1e15, 0.0, 0.0)), None);
    }

    #[test]
    fn a_cast_key_casts_at_its_target_and_with_ctrl_learns() {
        let target = ActionTarget::None;
        assert_eq!(
            Orders::slot_action(2, KeyUse::Cast, target),
            Action::Orders(OrdersAction::Slot { slot: 2, target })
        );
        assert_eq!(
            Orders::slot_action(2, KeyUse::Learn, target),
            Action::Progression(ProgressionAction::Learn { slot: 2 })
        );
    }

    #[test]
    fn a_right_click_uses_what_the_avatar_may_use_rallies_producers_attacks_an_enemy_and_else_walks()
     {
        let mut ids = IdAllocator::default();
        let (north, south) = (Team::new(0), Team::new(1));
        let mut unit = |team| Pointed {
            id: ids.allocate(),
            team,
            owner: None,
        };
        let (enemy, friend) = (unit(south), unit(north));
        let point = Vec3::new(3.0, 1.0, -2.5);
        // To (3, −2.5): x = 3, z = −5 / 2, the height not sent.
        let (x, z) = (Num::int(3), Num::int(-5) / Num::int(2));
        let walk = Some(OrdersAction::Move { x, z }.into());
        let shrine = unit(south);
        let usable = |id| id == shrine.id;
        let click_by = |selected, under, team, point| {
            let click = Click {
                under,
                team,
                point,
                selected,
            };
            Orders::click_action(click, usable)
        };
        let click = |under, team| click_by(Selected::Movers, under, team, point);
        assert_eq!(
            click(Some(enemy), Some(north)),
            Some(OrdersAction::Attack { target: enemy.id }.into())
        );
        assert_eq!(click(Some(friend), Some(north)), walk);
        assert_eq!(click(None, Some(north)), walk);
        // A client of no team has no enemy.
        assert_eq!(click(Some(enemy), None), walk);
        // A unit the avatar may use is used, before an attack of an enemy or a walk: the
        // hostile shrine by either team's client.
        let used = Some(OrdersAction::Use { target: shrine.id }.into());
        assert_eq!(click(Some(shrine), Some(north)), used);
        assert_eq!(click(Some(shrine), Some(south)), used);
        // Producers rally to the point, or to the unit under the cursor, an enemy's too.
        let rally = |target| {
            Some(
                ProductionAction::Rally {
                    target: Some(target),
                }
                .into(),
            )
        };
        let producers = |under| click_by(Selected::Producers, under, Some(north), point);
        assert_eq!(producers(None), rally(RallyTarget::Point { x, z }));
        assert_eq!(producers(Some(friend)), rally(RallyTarget::Unit(friend.id)));
        assert_eq!(producers(Some(enemy)), rally(RallyTarget::Unit(enemy.id)));
        // A ground point far off any map orders nothing, a walk or a rally.
        let far = Vec3::new(1e15, 0.0, 0.0);
        for selected in [Selected::Movers, Selected::Producers] {
            assert_eq!(click_by(selected, None, Some(north), far), None);
        }
    }

    #[test]
    fn a_selection_of_producers_and_no_walker_rallies() {
        let (walker, producer, tower) = (
            Mover {
                walks: true,
                trains: false,
            },
            Mover {
                walks: false,
                trains: true,
            },
            Mover {
                walks: false,
                trains: false,
            },
        );
        // A barracks alone, or with a tower; a barracks with a ranger, a ranger alone, a tower
        // alone, and nothing.
        for (units, selected) in [
            (&[producer][..], Selected::Producers),
            (&[producer, tower], Selected::Producers),
            (&[producer, walker], Selected::Movers),
            (&[walker], Selected::Movers),
            (&[tower], Selected::Movers),
            (&[], Selected::Movers),
        ] {
            assert_eq!(Selected::of(units.iter().copied()), selected, "{units:?}");
        }
    }
}
