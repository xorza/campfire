use bevy::app::{App, Plugin, Startup, Update};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::{Allow, Or, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::SystemCondition;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::input::ButtonInput;
use bevy::input::common_conditions::{input_just_pressed, input_just_released};
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::math::{Rect, Vec2};
use bevy::picking::Pickable;
use bevy::state::app::AppExtStates;
use bevy::state::condition::in_state;
use bevy::time::{Real, Time};
use bevy::ui::{BackgroundColor, BorderColor, Node, PositionType, UiRect, Val};
use campfire_client_files::Press;
use campfire_mode::PlayerControl;
use campfire_production::Sold;
use campfire_sim::{EntityIndex, StableId, Unpredicted};
use campfire_units::{Leader, SpawnOf};

use crate::input_map::InputMap;
use crate::input_mode::InputMode;
use crate::own_player::OwnPlayer;
use crate::pointer::Pointer;
use crate::selection::{Selection, SelectionSystems, SelectionVoice};
use crate::view::ViewSystems;

/// Keeps the selection of the player's own units (02-engine-core.md, A selection of the player's
/// own units): each frame, a unit the client no longer holds or that its player sold leaves it, and
/// an empty selection takes the avatar. In a mode of `PlayerControl::Units` (02-engine-core.md, The
/// mode says what a player selects), the left button selects: a click on one of them selects it
/// alone, and a drag selects each whose ground point shows within the rectangle it draws; a click
/// or a drag that meets none of them leaves the selection as it is, a sold one selected by neither,
/// as Zero Hour makes a sold building unselectable; and a press while the player places a build or
/// aims a command is theirs.
#[derive(Debug)]
pub(crate) struct SelectingPlugin {
    pub(crate) control: PlayerControl,
}

/// The left button, held down over the match: the point of the window it went down at, where the
/// cursor is now, and whether it moved far enough to drag.
#[derive(Debug, Clone, Copy, PartialEq)]
struct HeldPress {
    from: Vec2,
    to: Vec2,
    drags: bool,
}

/// The press the left button holds; none while it is up, or held from over the UI.
#[derive(Resource, Debug, Default)]
struct Held(Option<HeldPress>);

/// The last click that met a unit, which a click on the same unit soon after makes a double
/// click: the unit, and the app's seconds at the click; none once a double click took it.
#[derive(Resource, Debug, Default)]
struct LastClick(Option<(StableId, f64)>);

/// The units their players sold, and those out of the world, as a rider is, which no selection
/// holds, as Zero Hour's rider leaves its group as it enters.
type Unselectable<'w, 's> = Query<
    'w,
    's,
    (),
    (
        Or<(With<Sold>, With<Disabled>)>,
        Allow<Unpredicted>,
        Allow<Disabled>,
    ),
>;

/// The rectangle a drag draws, a node of the UI that takes no clicks.
#[derive(Component, Debug)]
struct DragBox;

/// The drawn rectangle's edge and its see-through fill.
const BOX_EDGE: Color = Color::srgb(0.3, 0.95, 0.4);
const BOX_FILL: Color = Color::srgba(0.3, 0.95, 0.4, 0.12);

impl Plugin for SelectingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>();
        app.add_message::<SelectionVoice>();
        app.init_state::<InputMode>();
        app.add_systems(
            Update,
            Self::keep.in_set(SelectionSystems).after(ViewSystems),
        );
        if self.control == PlayerControl::Avatar {
            return;
        }
        app.init_resource::<Held>();
        app.init_resource::<LastClick>();
        app.add_systems(Startup, Self::add_box);
        app.add_systems(
            Update,
            (
                Self::press.run_if(
                    input_just_pressed(MouseButton::Left).and_then(in_state(InputMode::Select)),
                ),
                Self::follow,
                Self::release.run_if(input_just_released(MouseButton::Left)),
                Self::draw_box,
            )
                .chain()
                .in_set(SelectionSystems)
                .after(Self::keep),
        );
    }
}

impl SelectingPlugin {
    fn add_box(mut commands: Commands<'_, '_>) {
        commands.spawn((
            DragBox,
            Node {
                position_type: PositionType::Absolute,
                border: UiRect::all(Val::Px(1.0)),
                ..Node::default()
            },
            BorderColor::all(BOX_EDGE),
            BackgroundColor(BOX_FILL),
            Visibility::Hidden,
            Pickable::IGNORE,
        ));
    }

    /// Drops from the selection each unit the client no longer holds, that its player sold, or
    /// that left the world; an empty selection takes the avatar. A selection that stays as it was
    /// is not marked changed.
    fn keep(
        (index, sold): (Res<'_, EntityIndex>, Unselectable<'_, '_>),
        own: OwnPlayer<'_, '_>,
        mut selection: ResMut<'_, Selection>,
    ) {
        let avatar = own.avatar().map(|avatar| avatar.id);
        let held = |unit: StableId| index.get(unit).is_some_and(|unit| !sold.contains(unit));
        if selection.bypass_change_detection().keep(held, avatar) {
            selection.set_changed();
        }
    }

    /// Holds a press of the left button, unless the cursor is over the UI or off the window.
    fn press(pointer: Pointer<'_, '_>, mut held: ResMut<'_, Held>) {
        held.0 = pointer
            .cursor()
            .filter(|_| !pointer.is_over_ui())
            .map(|at| HeldPress {
                from: at,
                to: at,
                drags: false,
            });
    }

    /// Follows the cursor with the held press, which drags by the mode's press.
    fn follow(pointer: Pointer<'_, '_>, input: Res<'_, InputMap>, mut held: ResMut<'_, Held>) {
        if let (Some(press), Some(at)) = (held.0.as_mut(), pointer.cursor()) {
            press.follow(at, input.press());
        }
    }

    /// Ends the held press: a click selects the player's own unit under the cursor alone, a
    /// double click on a unit that walks every unit of its type the screen shows, or the map
    /// holds with the map key, and a drag each of the player's own units that walk its rectangle
    /// holds, a leader's spawn as its leader; with the add key they join the selection, and a
    /// click's units all selected leave it (02-engine-core.md, A drag selects no
    /// building; 02-engine-core.md, Shift adds, a double click takes a kind). One that
    /// meets none of them leaves the selection as it is.
    fn release(
        pointer: Pointer<'_, '_>,
        (input, keys, time): (
            Res<'_, InputMap>,
            Res<'_, ButtonInput<KeyCode>>,
            Res<'_, Time<Real>>,
        ),
        (index, spawns, leaders): (
            Res<'_, EntityIndex>,
            Query<'_, '_, &SpawnOf>,
            Query<'_, '_, (), With<Leader>>,
        ),
        sold: Unselectable<'_, '_>,
        (mut held, mut last): (ResMut<'_, Held>, ResMut<'_, LastClick>),
        (mut selection, mut voices): (ResMut<'_, Selection>, MessageWriter<'_, SelectionVoice>),
        mut picked: Local<'_, Vec<StableId>>,
    ) {
        let Some(press) = held.0.take() else {
            return;
        };
        picked.clear();
        let mut clicked = None;
        if press.drags {
            last.0 = None;
            pointer.own_walkers_within(press.rect(), &mut picked);
        } else if let Some(unit) = pointer.own_unit_under() {
            let now = time.elapsed_secs_f64();
            if last.double(unit, now, input.press()) {
                pointer.own_kind_of(unit, input.across_map(&keys), &mut picked);
            }
            if picked.is_empty() {
                picked.push(unit);
                clicked = Some(unit);
            }
        } else {
            // A drag, or a click on no unit of the player's, comes between two clicks, as a
            // double click follows its first click alone.
            last.0 = None;
        }
        picked.retain(|&unit| index.get(unit).is_none_or(|unit| !sold.contains(unit)));
        let led_by = |unit: StableId| {
            let spawner = spawns.get(index.get(unit)?).ok()?.spawner();
            leaders.contains(index.get(spawner)?).then_some(spawner)
        };
        let chosen = selection.bypass_change_detection();
        let changed = if !input.adds(&keys) {
            chosen.pick(&picked, led_by)
        } else if clicked.is_some() && chosen.remove(&picked, led_by) {
            true
        } else {
            chosen.add(&picked, led_by)
        };
        if changed {
            selection.set_changed();
            voices.write(SelectionVoice::Select);
        }
    }

    /// Shows the rectangle of a held press that drags, and hides it otherwise; the node changes,
    /// and the UI lays it out again, only when the rectangle moves.
    fn draw_box(
        held: Res<'_, Held>,
        drawn: Single<'_, '_, (&mut Node, &mut Visibility), With<DragBox>>,
    ) {
        let (mut node, mut visibility) = drawn.into_inner();
        let Some(rect) = held.0.filter(|press| press.drags).map(HeldPress::rect) else {
            visibility.set_if_neq(Visibility::Hidden);
            return;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let place = [rect.min.x, rect.min.y, rect.width(), rect.height()].map(Val::Px);
        if [node.left, node.top, node.width, node.height] != place {
            [node.left, node.top, node.width, node.height] = place;
        }
    }
}

impl LastClick {
    /// Notes a click on `unit` at `now`, the app's seconds; whether it doubles the last, a click
    /// on the same unit within `rule`'s double click time, which then no third click doubles.
    fn double(&mut self, unit: StableId, now: f64, rule: Press) -> bool {
        let doubles = self
            .0
            .is_some_and(|(last, at)| last == unit && rule.doubles(now - at));
        self.0 = (!doubles).then_some((unit, now));
        doubles
    }
}

impl HeldPress {
    /// Moves its cursor to `at`; once the press drags by `rule`, it drags until it ends, wherever
    /// the cursor goes back to.
    fn follow(&mut self, at: Vec2, rule: Press) {
        self.to = at;
        self.drags |= rule.drags(self.from, at);
    }

    /// The rectangle from where the button went down to the cursor.
    const fn rect(self) -> Rect {
        Rect::from_corners(self.from, self.to)
    }
}

#[cfg(test)]
mod tests {
    use bevy::state::app::StatesPlugin;

    use super::*;

    #[test]
    fn the_left_button_selects_only_in_a_mode_of_units() {
        // Each mode keeps a selection; only one of units holds a press of the left button.
        let held = |control| {
            let mut app = App::new();
            app.add_plugins((StatesPlugin, SelectingPlugin { control }));
            assert!(app.world().contains_resource::<Selection>());
            app.world().contains_resource::<Held>()
        };
        let controls = [PlayerControl::Units, PlayerControl::Avatar];
        assert_eq!(controls.map(held), [true, false]);
    }

    #[test]
    fn a_second_click_on_its_unit_within_the_double_time_doubles_and_a_third_does_not() {
        let mut ids = campfire_sim::IdAllocator::default();
        let [tank, dozer] = [ids.allocate(), ids.allocate()];
        let rule = Press::default();
        let mut last = LastClick::default();
        // A click, then one on the same unit 0.5 s after, Windows' time: a double click; a third
        // 0.1 s after starts again.
        assert!(!last.double(tank, 1.0, rule));
        assert!(last.double(tank, 1.5, rule));
        assert!(!last.double(tank, 1.6, rule));
        // 0.6 s after, or on another unit, it does not double.
        assert!(!last.double(tank, 2.2, rule));
        assert!(!last.double(dozer, 2.3, rule));
        assert!(last.double(dozer, 2.4, rule));
    }

    #[test]
    fn a_press_drags_once_the_cursor_goes_past_the_modes_pixels_and_drags_until_it_ends() {
        let from = Vec2::new(100.0, 50.0);
        let held = || HeldPress {
            from,
            to: from,
            drags: false,
        };
        // By the engine's press of 4 px, 4 px right and 4 px up is a shaking click; 4.5 px down
        // drags.
        let engine = Press::default();
        let mut press = held();
        press.follow(Vec2::new(104.0, 46.0), engine);
        assert!(!press.drags);
        press.follow(Vec2::new(100.0, 54.5), engine);
        assert!(press.drags);
        // Back over where it went down, it still drags, an empty rectangle.
        press.follow(from, engine);
        assert!(press.drags);
        assert_eq!(press.rect(), Rect::from_corners(from, from));
        // Dragged up and left, the rectangle runs from the cursor to where it went down.
        press.follow(Vec2::new(40.0, 10.0), engine);
        assert_eq!(
            press.rect(),
            Rect {
                min: Vec2::new(40.0, 10.0),
                max: Vec2::new(100.0, 50.0),
            }
        );
        // A mode's press of 10 px takes the 4.5 px as a click, and drags past 10 px.
        let wide = Press {
            drag: 10.0,
            ..engine
        };
        let mut press = held();
        press.follow(Vec2::new(100.0, 54.5), wide);
        assert!(!press.drags);
        press.follow(Vec2::new(110.5, 50.0), wide);
        assert!(press.drags);
    }
}
