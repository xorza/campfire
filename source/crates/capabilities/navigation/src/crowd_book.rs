use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError};
use campfire_names::MatchNames;
use campfire_values::Team;

use crate::mode_navigation_data::ModeNavigationData;
use crate::moves::Moves;

/// The mode's rule of who moves whom in a crowd, which steering and collision read, and the
/// mode's teams, whose crowd costs the load makes.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct CrowdBook {
    pub moves: Moves,
    pub(crate) teams: Box<[Team]>,
}

impl CrowdBook {
    /// The rule of the mode's navigation and the mode's teams; the default rule for a load of
    /// none, and no team for a match of no mode.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let data = cx.mode_section::<ModeNavigationData>();
        let teams = cx.names_if::<Team>()?;
        Ok(Self {
            moves: data.map_or_else(Moves::default, |data| data.moves),
            teams: teams.map_or_else(Box::default, |teams| {
                teams.iter().map(|(team, _)| team).collect()
            }),
        })
    }
}

impl Book for CrowdBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
