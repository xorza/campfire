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
mod tests {
    use std::num::NonZeroU8;

    use campfire_actions::{SlotKind, SlotKindData, SlotRanks};
    use campfire_common::DeclaredName;
    use campfire_math::Num;
    use campfire_sim::IdAllocator;
    use campfire_values::{ActionId, Rank};

    use super::*;

    #[test]
    fn the_ability_keys_cast_the_slots_of_the_kinds_whose_ranks_are_learned() {
        // A weapon of ranks from the spawn, then two learned basics and an ultimate: Q, W and E
        // take slots 1 to 3, whatever the weapon stands first.
        let kind = |name, ranks: Option<u8>| SlotKindData {
            name: DeclaredName::new(name).unwrap(),
            ranks: ranks.map(|count| SlotRanks::new(NonZeroU8::new(count).unwrap(), None).unwrap()),
        };
        let kinds = [
            kind("weapon", None),
            kind("basic", Some(2)),
            kind("ultimate", Some(1)),
        ];
        let kinds = SlotKinds::new(kinds).unwrap();
        let held = |kind| (ActionId::new(0), SlotKind::new(kind), Some(Rank::FIRST));
        let slots = ActionSlots::new([held(0), held(1), held(1), held(2)]);
        let abilities: Vec<u8> = OrdersPlugin::abilities(&slots, &kinds).collect();
        assert_eq!(abilities, [1, 2, 3]);
    }

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
            OrdersPlugin::slot_directive(2, KeyUse::Cast, target),
            Directive::Orders(OrdersDirective::Slot { slot: 2, target })
        );
        assert_eq!(
            OrdersPlugin::slot_directive(2, KeyUse::Learn, target),
            Directive::Progression(ProgressionDirective::Learn { slot: 2 })
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
        let walk = Some(
            OrdersDirective::Move {
                to: GroundPoint { x, z },
            }
            .into(),
        );
        let shrine = unit(south);
        let usable = |id| id == shrine.id;
        let click_by = |selected, under, team, point| {
            let click = Click {
                under,
                team,
                point,
                selected,
            };
            OrdersPlugin::click_directive(click, usable)
        };
        let click = |under, team| click_by(Selected::Movers, under, team, point);
        assert_eq!(
            click(Some(enemy), Some(north)),
            Some(OrdersDirective::Attack { target: enemy.id }.into())
        );
        assert_eq!(click(Some(friend), Some(north)), walk);
        assert_eq!(click(None, Some(north)), walk);
        // A client of no team has no enemy.
        assert_eq!(click(Some(enemy), None), walk);
        // A unit the avatar may use is used, before an attack of an enemy or a walk: the
        // hostile shrine by either team's client.
        let used = Some(OrdersDirective::Use { target: shrine.id }.into());
        assert_eq!(click(Some(shrine), Some(north)), used);
        assert_eq!(click(Some(shrine), Some(south)), used);
        // Producers rally to the point, or to the unit under the cursor, an enemy's too.
        let rally = |target| {
            Some(
                ProductionDirective::Rally {
                    target: Some(target),
                }
                .into(),
            )
        };
        let producers = |under| click_by(Selected::Producers, under, Some(north), point);
        assert_eq!(
            producers(None),
            rally(RallyTarget::Point(GroundPoint { x, z }))
        );
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
