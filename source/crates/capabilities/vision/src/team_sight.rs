use std::fmt::Debug;

use bevy_ecs::query::{Allow, QueryItem, ReadOnlyQueryData, With};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_declarations::RoleForm;
use campfire_math::Vec3;
use campfire_players::Player;
use campfire_sim::Unpredicted;
use campfire_units::SeenBy;
use campfire_values::Team;

use crate::fog::Fog;
use crate::seen_cells::SeenCells;
use crate::vision_grid::VisionGrid;

/// How a match tells whether a team sees a unit, for a check of what a player's order may aim at:
/// by the server's vision, or, on a predicting client, by what the client holds, as the server
/// sends it each unit its team sees and no other (10-replication-audiences.md, Prediction reads
/// only what its client holds).
pub trait TeamSight: RoleForm {
    /// What a query reads of a unit to tell.
    type Seen: ReadOnlyQueryData + Debug + 'static;

    /// Whether `team` sees the unit of `seen`; `None` when no vision stage has spoken of it, which
    /// the caller decides.
    fn sees(seen: QueryItem<'_, '_, Self::Seen>, team: Team) -> Option<bool>;

    /// Whether `team` sees the point `position` of `grid`'s ground, as `points` tell.
    fn sees_point(
        points: &PointSight<'_, '_>,
        grid: &VisionGrid,
        position: Vec3,
        team: Team,
    ) -> bool;
}

/// What a system reads to tell whether a team sees a point of the ground: the server's fog, and
/// the cells of each team the server sent the players a client holds, as it sent them.
#[derive(SystemParam, Debug)]
pub struct PointSight<'w, 's> {
    fog: Option<Res<'w, Fog>>,
    players: Query<'w, 's, (&'static Team, &'static SeenCells), (With<Player>, Allow<Unpredicted>)>,
}

/// The server's sight: the teams the last Vision stage found see a unit, `SeenBy`.
#[derive(Debug)]
pub struct ByVision;

impl TeamSight for ByVision {
    type Seen = Option<&'static SeenBy>;

    fn sees(seen: Option<&SeenBy>, team: Team) -> Option<bool> {
        let teams = seen.and_then(|seen| seen.get())?;
        Some(teams.contains(team))
    }

    /// By the fog; a match of none tests no point.
    fn sees_point(
        points: &PointSight<'_, '_>,
        grid: &VisionGrid,
        position: Vec3,
        team: Team,
    ) -> bool {
        points
            .fog
            .as_deref()
            .is_none_or(|fog| fog.sees_point(grid, position, team))
    }
}

/// A predicting client's sight: its team sees each unit it holds, as the server sends it only
/// those, and `SeenBy` stays the server's.
#[derive(Debug)]
pub struct ByHolding;

/// The server's sight is of the server's form of a system.
impl RoleForm for ByVision {
    const PREDICTS: bool = false;
}

/// A predicting client's sight is of its form of a system.
impl RoleForm for ByHolding {
    const PREDICTS: bool = true;
}

impl TeamSight for ByHolding {
    type Seen = ();

    fn sees((): (), _: Team) -> Option<bool> {
        Some(true)
    }

    /// By the cells of `team` the server sent its player, as the server's fog holds them
    /// (04-capabilities/vision.md, The server sends a team's cells to its player); a team of none
    /// the client holds, as a spectator's, the server decides.
    fn sees_point(
        points: &PointSight<'_, '_>,
        grid: &VisionGrid,
        position: Vec3,
        team: Team,
    ) -> bool {
        let held = points
            .players
            .iter()
            .find(|&(&of, seen)| of == team && !seen.cells().is_empty());
        let Some((_, seen)) = held else {
            return true;
        };
        grid.grid
            .cell_of(position)
            .is_some_and(|cell| seen.cells().holds(cell))
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::entity_disabling::DefaultQueryFilters;
    use bevy_ecs::system::SystemState;
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_geometry::{Bounds, Grid};
    use campfire_math::Num;

    use super::*;
    use crate::brush_map::BrushMap;
    use crate::cell_bits::CellBits;

    #[test]
    fn a_client_tells_a_point_by_its_players_cells_and_the_server_by_its_fog() {
        // A grid of 1 m cells over ±2 m, 16 cells; team 0's player sees the cell of (0.5, 0.5).
        let bounds = Bounds::new([Num::int(-2); 2], [Num::int(2); 2]).unwrap();
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        let vision = VisionGrid {
            brush: BrushMap::new(&grid, &[]),
            grid,
            teams: 2,
            shroud: false,
            neutral_in_fog: false,
        };
        let point = |x: i64| Vec3::new(Num::int(x) + Num::HALF, Num::ZERO, Num::HALF);
        let cell = vision.grid.cell_of(point(0)).unwrap();
        let mut cells = CellBits::empty(16);
        cells.set(&[1 << cell]);
        // A client holds its player's entity as the server sent it, which queries of no
        // `Allow<Unpredicted>` skip.
        let mut world = World::new();
        world.init_resource::<DefaultQueryFilters>();
        Unpredicted::register(&mut world);
        let player = (Player::new(PlayerSlot::new(0)), Team::new(0));
        world.spawn((player, SeenCells(cells), Unpredicted));
        let mut state = SystemState::<PointSight<'_, '_>>::new(&mut world);
        let points = state.get(&world).unwrap();
        let held =
            |team: u8, x: i64| ByHolding::sees_point(&points, &vision, point(x), Team::new(team));
        // Its own team's cell and no other; a team of no player the client holds, the server's to
        // tell.
        assert_eq!([held(0, 0), held(0, 1), held(1, 1)], [true, false, true]);
        // A match of no fog tests no point on the server.
        assert!(ByVision::sees_point(
            &points,
            &vision,
            point(1),
            Team::new(0)
        ));
    }
}
