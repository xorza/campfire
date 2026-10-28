use std::fmt;

use bevy::camera::Camera;
use bevy::ecs::query::{Allow, Has, With, Without};
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
use campfire_units::{Dead, Destination, Owner};
use campfire_values::{Team, UnitType};

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
/// it, its drawing, its type, and whether it walks, as a unit of a destination does.
type LivingUnits<'w, 's> = Query<
    'w,
    's,
    (
        (
            &'static StableId,
            &'static Team,
            Option<&'static Owner>,
            &'static Drawing,
        ),
        (&'static UnitType, Has<Destination>),
    ),
    (Without<Dead>, Allow<Unpredicted>),
>;

/// A unit a drag or a double click may select: the unit, its type, and whether it walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shown {
    unit: Pointed,
    unit_type: UnitType,
    walks: bool,
}

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

    /// Adds to `out` each living unit of the player's own that walks whose ground point shows
    /// within `rect` of the window, in logical pixels, edges included: a drag's, which takes no
    /// building (02-engine-core.md, A drag selects no building).
    pub(crate) fn own_walkers_within(&self, rect: Rect, out: &mut Vec<StableId>) {
        out.extend(shown_within(
            Some(rect),
            self.own.player_slot(),
            self.shown(),
            |shown| shown.walks,
        ));
    }

    /// Adds to `out` each living unit of the player's own of the type of `unit` that walks, as a
    /// double click on `unit` takes them: those whose ground point the window shows, or every one
    /// with `across_map` (02-engine-core.md, Shift adds, a double click takes a kind).
    /// None for a unit that does not walk, as no mass selection takes a building.
    pub(crate) fn own_kind_of(&self, unit: StableId, across_map: bool, out: &mut Vec<StableId>) {
        let Some(kind) = self
            .units
            .iter()
            .find(|((id, ..), _)| **id == unit)
            .and_then(|(_, (&unit_type, walks))| walks.then_some(unit_type))
        else {
            return;
        };
        let window = Rect::new(0.0, 0.0, self.window.width(), self.window.height());
        out.extend(shown_within(
            (!across_map).then_some(window),
            self.own.player_slot(),
            self.shown(),
            |shown| shown.walks && shown.unit_type == kind,
        ));
    }

    /// Each living unit with a drawing, and the point of the window its drawing shows at, if it
    /// shows.
    fn shown(&self) -> impl Iterator<Item = (Shown, Option<Vec2>)> {
        let (camera, place) = *self.camera;
        self.units
            .iter()
            .filter_map(move |((&id, &team, owner, drawn), (&unit_type, walks))| {
                let transform = self.drawings.get(drawn.root()).ok()?;
                let shown = Shown {
                    unit: Pointed::new(id, team, owner),
                    unit_type,
                    walks,
                };
                Some((
                    shown,
                    camera.world_to_viewport(place, transform.translation).ok(),
                ))
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
/// stable ids of those the player of `player_slot` controls that `takes` takes, whose point lies
/// within `rect`, edges included, or anywhere, shown or not, for none.
fn shown_within(
    rect: Option<Rect>,
    player_slot: Option<PlayerSlot>,
    units: impl Iterator<Item = (Shown, Option<Vec2>)>,
    takes: impl Fn(Shown) -> bool,
) -> impl Iterator<Item = StableId> {
    units
        .filter(move |&(shown, at)| {
            let within = rect.is_none_or(|rect| at.is_some_and(|at| rect.contains(at)));
            shown.unit.owned_by(player_slot) && takes(shown) && within
        })
        .map(|(shown, _)| shown.unit.id)
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
    fn a_drag_selects_the_players_own_walkers_shown_within_its_rectangle_and_a_kind_its_type() {
        let mut ids = IdAllocator::default();
        let (own, other) = (PlayerSlot::new(0), PlayerSlot::new(1));
        let (tank, dozer) = (UnitType::new(0), UnitType::new(1));
        let mut unit = |owner: Option<PlayerSlot>, unit_type, walks| Shown {
            unit: Pointed {
                id: ids.allocate(),
                team: Team::new(0),
                owner,
            },
            unit_type,
            walks,
        };
        // A rectangle from (10, 20) to (110, 70), dragged from its bottom right corner.
        let rect = Rect::from_corners(Vec2::new(110.0, 70.0), Vec2::new(10.0, 20.0));
        let [inside, edge, outside, behind, building, others, unowned] = [
            unit(Some(own), tank, true),
            unit(Some(own), dozer, true),
            unit(Some(own), tank, true),
            unit(Some(own), tank, true),
            unit(Some(own), tank, false),
            unit(Some(other), tank, true),
            unit(None, tank, true),
        ];
        let units = [
            // Inside; on the left edge; 0.5 px past the bottom edge; behind the camera, shown
            // nowhere; a building inside.
            (inside, Some(Vec2::new(60.0, 45.0))),
            (edge, Some(Vec2::new(10.0, 50.0))),
            (outside, Some(Vec2::new(60.0, 70.5))),
            (behind, None),
            (building, Some(Vec2::new(60.0, 45.0))),
            // Inside, another player's, and no player's.
            (others, Some(Vec2::new(60.0, 45.0))),
            (unowned, Some(Vec2::new(60.0, 45.0))),
        ];
        let id = |shown: Shown| shown.unit.id;
        let within = |rect, slot, takes: &dyn Fn(Shown) -> bool| {
            shown_within(rect, slot, units.into_iter(), takes).collect::<Vec<_>>()
        };
        let walks = |shown: Shown| shown.walks;
        // A drag takes the walkers inside, not the building (02-engine-core.md, A drag
        // selects no building).
        assert_eq!(
            within(Some(rect), Some(own), &walks),
            [id(inside), id(edge)]
        );
        assert_eq!(within(Some(rect), Some(other), &walks), [id(others)]);
        // A spectator, of no slot, controls none, not even a unit no player controls.
        assert_eq!(within(Some(rect), None, &walks), []);
        // A kind of tanks within the rectangle, and on the whole map, shown or not.
        let tanks = |shown: Shown| shown.walks && shown.unit_type == tank;
        assert_eq!(within(Some(rect), Some(own), &tanks), [id(inside)]);
        assert_eq!(
            within(None, Some(own), &tanks),
            [id(inside), id(outside), id(behind)]
        );
    }
}
