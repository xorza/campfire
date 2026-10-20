use std::fmt::Debug;

use bevy_ecs::query::{QueryItem, ReadOnlyQueryData};
use campfire_declarations::RoleForm;
use campfire_math::Vec3;
use campfire_units::SeenBy;
use campfire_values::Team;

use crate::fog::Fog;
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

    /// Whether `team` sees the point `position` of the ground, as `fog` holds `grid`'s cells.
    fn sees_point(fog: &Fog, grid: &VisionGrid, position: Vec3, team: Team) -> bool;
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

    fn sees_point(fog: &Fog, grid: &VisionGrid, position: Vec3, team: Team) -> bool {
        fog.sees_point(grid, position, team)
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

    /// The client holds no fog: the server, which does, decides.
    fn sees_point(_: &Fog, _: &VisionGrid, _: Vec3, _: Team) -> bool {
        true
    }
}
