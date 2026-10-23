use std::fmt;

use bevy::camera::Camera;
use bevy::ecs::query::{Allow, With, Without};
use bevy::ecs::system::{Query, Res, Single, SystemParam};
use bevy::math::{Rect, Vec2, Vec3};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::shape::{InfinitePlane3d, Ray3d};
use bevy::transform::components::{GlobalTransform, Transform};
use bevy::ui::Node;
use bevy::window::{PrimaryWindow, Window};
use campfire_common::PlayerSlot;
use campfire_sim::{StableId, Unpredicted};
use campfire_units::{Dead, Owner};
use campfire_values::Team;

use crate::own_player::OwnPlayer;
use crate::pointer::unit_picking::UnderMouse;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::ground_heights::GroundHeights;

pub(crate) mod unit_picking;

/// What the cursor points at, as the player sees the match: the ground point under it, and the
/// living units the units' picking backend ranks under it.
#[derive(SystemParam)]
pub(crate) struct Pointer<'w, 's> {
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform)>,
    own: OwnPlayer<'w, 's>,
    units: LivingUnits<'w, 's>,
    drawings: Query<'w, 's, &'static Transform, With<DrawingOf>>,
    under: Res<'w, UnderMouse>,
    heights: Option<Res<'w, GroundHeights>>,
    hovers: Res<'w, HoverMap>,
    nodes: Query<'w, 's, (), With<Node>>,
}

/// The living units the client holds: each one's stable id, its team, the player who controls
/// it, and its drawing.
type LivingUnits<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static Team,
        Option<&'static Owner>,
        &'static Drawing,
    ),
    (Without<Dead>, Allow<Unpredicted>),
>;

/// A unit the player may point at: its stable id, its team, and the player who controls it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pointed {
    pub(crate) id: StableId,
    pub(crate) team: Team,
    pub(crate) owner: Option<PlayerSlot>,
}

impl Pointer<'_, '_> {
    /// Whether the cursor is over a node of the UI, the shop panel or the inventory bar, which
    /// takes its clicks from the match.
    pub(crate) fn is_over_ui(&self) -> bool {
        let hits = self.hovers.get(&PointerId::Mouse);
        hits.is_some_and(|hits| hits.keys().any(|&hit| self.nodes.contains(hit)))
    }

    /// The cursor's point of the window, in logical pixels from its top left; none when the
    /// cursor is off the window.
    pub(crate) fn cursor(&self) -> Option<Vec2> {
        self.window.cursor_position()
    }

    /// The point of the ground under the cursor, when the cursor is over the window and the ground:
    /// the drawn ground of a map with heights, else the plane at height 0.
    pub(crate) fn ground(&self) -> Option<Vec3> {
        let ray = self.ray()?;
        match self.heights.as_deref() {
            Some(heights) => heights.hit(ray.origin, *ray.direction),
            None => ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)),
        }
    }

    /// The living unit under the cursor, of those not in `except`, a list in stable id order.
    pub(crate) fn unit_under(&self, except: &[StableId]) -> Option<Pointed> {
        debug_assert!(
            except.is_sorted(),
            "a binary search needs the list in order"
        );
        (self.under.iter()).find(|unit| except.binary_search(&unit.id).is_err())
    }

    /// The living unit of the player's own under the cursor.
    pub(crate) fn own_unit_under(&self) -> Option<StableId> {
        let player_slot = self.own.player_slot();
        let unit = self.under.iter().find(|unit| unit.owned_by(player_slot))?;
        Some(unit.id)
    }

    /// The ray from the camera through the cursor, when the cursor is over the window.
    fn ray(&self) -> Option<Ray3d> {
        let (camera, place) = *self.camera;
        camera.viewport_to_world(place, self.cursor()?).ok()
    }

    /// Adds to `out` each living unit of the player's own whose ground point shows within `rect`
    /// of the window, in logical pixels, edges included.
    pub(crate) fn own_units_within(&self, rect: Rect, out: &mut Vec<StableId>) {
        let (camera, place) = *self.camera;
        let shown = self
            .drawn()
            .map(|(unit, at)| (unit, camera.world_to_viewport(place, at).ok()));
        out.extend(shown_within(rect, self.own.player_slot(), shown));
    }

    /// Each living unit with a drawing, and where the drawing stands.
    fn drawn(&self) -> impl Iterator<Item = (Pointed, Vec3)> {
        self.units.iter().filter_map(|(&id, &team, owner, drawn)| {
            let transform = self.drawings.get(drawn.root()).ok()?;
            Some((Pointed::new(id, team, owner), transform.translation))
        })
    }
}

impl Pointed {
    /// The unit of `id`, of `team`, that `owner`'s player controls.
    fn new(id: StableId, team: Team, owner: Option<&Owner>) -> Self {
        Self {
            id,
            team,
            owner: owner.map(|owner| owner.player_slot()),
        }
    }

    /// Whether the player of `player_slot` controls it; a client of no slot, a spectator's,
    /// controls no unit.
    fn owned_by(self, player_slot: Option<PlayerSlot>) -> bool {
        player_slot.is_some() && self.owner == player_slot
    }
}

/// Of `units`, each with the point of the window its ground point shows at, if it shows, the
/// stable ids of those the player of `player_slot` controls whose point lies within `rect`, edges
/// included.
fn shown_within(
    rect: Rect,
    player_slot: Option<PlayerSlot>,
    units: impl Iterator<Item = (Pointed, Option<Vec2>)>,
) -> impl Iterator<Item = StableId> {
    units
        .filter(move |&(unit, shown)| {
            unit.owned_by(player_slot) && shown.is_some_and(|shown| rect.contains(shown))
        })
        .map(|(unit, _)| unit.id)
}

/// A system param holds borrows of the world, whose queries print nothing of use.
impl fmt::Debug for Pointer<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pointer")
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn a_drag_selects_the_players_own_units_shown_within_its_rectangle() {
        let mut ids = IdAllocator::default();
        let (own, other) = (PlayerSlot::new(0), PlayerSlot::new(1));
        let mut unit = |owner: Option<PlayerSlot>| Pointed {
            id: ids.allocate(),
            team: Team::new(0),
            owner,
        };
        // A rectangle from (10, 20) to (110, 70), dragged from its bottom right corner.
        let rect = Rect::from_corners(Vec2::new(110.0, 70.0), Vec2::new(10.0, 20.0));
        let [inside, edge, outside, behind, others, unowned] = [
            unit(Some(own)),
            unit(Some(own)),
            unit(Some(own)),
            unit(Some(own)),
            unit(Some(other)),
            unit(None),
        ];
        let units = [
            // Inside; on the left edge; 0.5 px past the bottom edge; behind the camera, shown
            // nowhere.
            (inside, Some(Vec2::new(60.0, 45.0))),
            (edge, Some(Vec2::new(10.0, 50.0))),
            (outside, Some(Vec2::new(60.0, 70.5))),
            (behind, None),
            // Inside, another player's, and no player's.
            (others, Some(Vec2::new(60.0, 45.0))),
            (unowned, Some(Vec2::new(60.0, 45.0))),
        ];
        let within = |slot| shown_within(rect, slot, units.into_iter()).collect::<Vec<_>>();
        assert_eq!(within(Some(own)), [inside.id, edge.id]);
        assert_eq!(within(Some(other)), [others.id]);
        // A spectator, of no slot, controls none, not even a unit no player controls.
        assert_eq!(within(None), []);
    }
}
