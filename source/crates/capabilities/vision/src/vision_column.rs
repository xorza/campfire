use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_script::{AnyKeptRows, Checked, KeptRows, RowFill, Unit, ViewColumn};
use campfire_units::SeenBy;
use campfire_values::{Relations, Team, TeamSet};
use campfire_visibility::DataKind;

/// The parts of a unit vision reads into its row: the teams that saw it, and its team.
pub(crate) type RowReads = (Option<&'static SeenBy>, Option<&'static Team>);

/// What vision adds to the script view: the teams that see each unit, a row each.
#[derive(Debug, Default)]
pub struct VisionColumn {
    rows: KeptRows<Vec<TeamSet>>,
}

impl ViewColumn for VisionColumn {
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    fn begin(&mut self, _: &World) -> bool {
        false
    }

    /// What the last Vision stage found that `team` does not see.
    fn hides(&self, row: usize, team: Team) -> bool {
        !self.rows.now()[row].contains(team)
    }
}

impl VisionColumn {
    /// Adds the row of a unit `seen_by` sees.
    pub fn push(&mut self, seen_by: TeamSet) {
        self.rows.now_mut().push(seen_by);
    }

    /// `unit.can_see(other)`: whether `unit`'s team sees `other`; a viewer that may not ask what
    /// `unit`'s team sees fails the call, as a bot may ask only of its own vision group.
    pub(crate) fn can_see(unit: &Unit, other: &Unit) -> Checked<bool> {
        let team = unit.read(|row| row.team)?;
        let view = unit.view();
        view.admit_sight(team)?;
        let at = other.admit(DataKind::Unit)?;
        Ok(view.rows().columns().sees(at, team))
    }

    /// Fills a row of the script view with the teams that see the unit.
    pub(super) fn fill_row(parts: ROQueryItem<'_, '_, RowReads>, fill: &mut RowFill<'_, Self>) {
        let seen_by = Self::seen_by(parts, fill.relations);
        fill.column.push(seen_by);
    }

    /// The teams that see `unit`: those the last Vision stage found, or, before it ran, the
    /// unit's vision group under `relations`, as that stage would give it at the least; every
    /// team for an entity with no team, as a match without vision sees.
    pub(super) fn seen_by(parts: ROQueryItem<'_, '_, RowReads>, relations: &Relations) -> TeamSet {
        match (parts.0.and_then(|seen| seen.get()), parts.1) {
            (Some(teams), _) => teams,
            (None, Some(&team)) => relations.vision_group(team),
            (None, None) => TeamSet::ALL,
        }
    }
}
