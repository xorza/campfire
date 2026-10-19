use bevy::app::{App, Plugin, Startup, Update};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::SystemCondition;
use bevy::ecs::system::{Commands, Local, Res, ResMut, Single};
use bevy::input::common_conditions::{input_just_pressed, input_just_released};
use bevy::input::mouse::MouseButton;
use bevy::math::{Rect, Vec2};
use bevy::picking::Pickable;
use bevy::state::app::AppExtStates;
use bevy::state::condition::in_state;
use bevy::ui::{BackgroundColor, BorderColor, Node, PositionType, UiRect, Val};
use campfire_sim::{EntityIndex, StableId};

use crate::input_mode::InputMode;
use crate::own_player::OwnPlayer;
use crate::pointer::Pointer;
use crate::press;
use crate::selection::{Selection, SelectionSystems};
use crate::view::ViewSystems;

/// Selects the player's own units with the left button (02-engine-core.md, A selection of the
/// player's own units): a click on one selects it alone, and a drag selects each whose ground point
/// shows within the rectangle it draws; a click or a drag that meets none of them leaves the
/// selection as it is, and a press while the player places a build is the preview's. Each frame, a
/// unit the client no longer holds leaves the selection, and an empty selection takes the avatar.
#[derive(Debug)]
pub(crate) struct Selecting;

/// The left button, held down over the match: the point of the window it went down at, where the
/// cursor is now, and whether it moved far enough to drag.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Press {
    from: Vec2,
    to: Vec2,
    drags: bool,
}

/// The press the left button holds; none while it is up, or held from over the UI.
#[derive(Resource, Debug, Default)]
struct Held(Option<Press>);

/// The rectangle a drag draws, a node of the UI that takes no clicks.
#[derive(Component, Debug)]
struct DragBox;

/// The drawn rectangle's edge and its see-through fill.
const BOX_EDGE: Color = Color::srgb(0.3, 0.95, 0.4);
const BOX_FILL: Color = Color::srgba(0.3, 0.95, 0.4, 0.12);

impl Plugin for Selecting {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>();
        app.init_resource::<Held>();
        app.init_state::<InputMode>();
        app.add_systems(Startup, Self::add_box);
        app.add_systems(
            Update,
            (
                Self::keep,
                Self::press.run_if(
                    input_just_pressed(MouseButton::Left).and_then(in_state(InputMode::Select)),
                ),
                Self::follow,
                Self::release.run_if(input_just_released(MouseButton::Left)),
                Self::draw_box,
            )
                .chain()
                .in_set(SelectionSystems)
                .after(ViewSystems),
        );
    }
}

impl Selecting {
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

    /// Drops from the selection each unit the client no longer holds; an empty selection takes
    /// the avatar. A selection that stays as it was is not marked changed.
    fn keep(
        index: Res<'_, EntityIndex>,
        own: OwnPlayer<'_, '_>,
        mut selection: ResMut<'_, Selection>,
    ) {
        let avatar = own.avatar().map(|avatar| avatar.id);
        let held = |unit: StableId| index.get(unit).is_some();
        if selection.bypass_change_detection().keep(held, avatar) {
            selection.set_changed();
        }
    }

    /// Holds a press of the left button, unless the cursor is over the UI or off the window.
    fn press(pointer: Pointer<'_, '_>, mut held: ResMut<'_, Held>) {
        held.0 = pointer
            .cursor()
            .filter(|_| !pointer.is_over_ui())
            .map(|at| Press {
                from: at,
                to: at,
                drags: false,
            });
    }

    /// Follows the cursor with the held press.
    fn follow(pointer: Pointer<'_, '_>, mut held: ResMut<'_, Held>) {
        if let (Some(press), Some(at)) = (held.0.as_mut(), pointer.cursor()) {
            press.follow(at);
        }
    }

    /// Ends the held press: a click selects the player's own unit under the cursor alone, a drag
    /// each of the player's own units its rectangle holds; one that meets none of them leaves
    /// the selection as it is.
    fn release(
        pointer: Pointer<'_, '_>,
        mut held: ResMut<'_, Held>,
        mut selection: ResMut<'_, Selection>,
        mut picked: Local<'_, Vec<StableId>>,
    ) {
        let Some(press) = held.0.take() else {
            return;
        };
        picked.clear();
        if press.drags {
            pointer.own_units_within(press.rect(), &mut picked);
        } else {
            let unit = pointer.own_unit_under();
            picked.extend(unit);
        }
        if selection.bypass_change_detection().pick(&picked) {
            selection.set_changed();
        }
    }

    /// Shows the rectangle of a held press that drags, and hides it otherwise; the node changes,
    /// and the UI lays it out again, only when the rectangle moves.
    fn draw_box(
        held: Res<'_, Held>,
        drawn: Single<'_, '_, (&mut Node, &mut Visibility), With<DragBox>>,
    ) {
        let (mut node, mut visibility) = drawn.into_inner();
        let Some(rect) = held.0.filter(|press| press.drags).map(Press::rect) else {
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

impl Press {
    /// Moves its cursor to `at`; once the press drags, it drags until it ends, wherever the
    /// cursor goes back to.
    fn follow(&mut self, at: Vec2) {
        self.to = at;
        self.drags |= press::drags(self.from, at);
    }

    /// The rectangle from where the button went down to the cursor.
    const fn rect(self) -> Rect {
        Rect::from_corners(self.from, self.to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_drags_once_the_cursor_goes_past_four_pixels_and_drags_until_it_ends() {
        let from = Vec2::new(100.0, 50.0);
        let mut press = Press {
            from,
            to: from,
            drags: false,
        };
        // 4 px right and 4 px up is a shaking click; 4.5 px down drags.
        press.follow(Vec2::new(104.0, 46.0));
        assert!(!press.drags);
        press.follow(Vec2::new(100.0, 54.5));
        assert!(press.drags);
        // Back over where it went down, it still drags, an empty rectangle.
        press.follow(from);
        assert!(press.drags);
        assert_eq!(press.rect(), Rect::from_corners(from, from));
        // Dragged up and left, the rectangle runs from the cursor to where it went down.
        press.follow(Vec2::new(40.0, 10.0));
        assert_eq!(
            press.rect(),
            Rect {
                min: Vec2::new(40.0, 10.0),
                max: Vec2::new(100.0, 50.0),
            }
        );
    }
}
