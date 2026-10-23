use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError, ById};
use campfire_script::MatchNames;

use crate::walker::Walker;
use crate::walker_kinds::WalkerKinds;
use campfire_values::UnitType;

/// The kinds of walker of the match's unit types, which the map's walk check and its pathing grid
/// read: navigation derives them once, from its book of each type's walker.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct WalkerBook {
    pub kinds: WalkerKinds,
}

impl WalkerBook {
    /// The kinds of the walkers of the match's unit types.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let walkers = cx.book_if::<ById<UnitType, Walker>>()?;
        let kinds = walkers
            .into_iter()
            .flat_map(ById::values)
            .copied()
            .collect();
        Ok(Self { kinds })
    }
}

impl Book for WalkerBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
