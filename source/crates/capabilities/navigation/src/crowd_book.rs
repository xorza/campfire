use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError};
use campfire_script::MatchNames;

use crate::mode_navigation_data::ModeNavigationData;
use crate::moves::Moves;

/// The mode's rule of who moves whom in a crowd, which steering and collision read.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct CrowdBook {
    pub moves: Moves,
}

impl CrowdBook {
    /// The rule of the mode's navigation; the default for a load of none.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder takes the build's signature"
    )]
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let data = cx.mode_section::<ModeNavigationData>();
        Ok(Self {
            moves: data.map_or_else(Moves::default, |data| data.moves),
        })
    }
}

impl Book for CrowdBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
