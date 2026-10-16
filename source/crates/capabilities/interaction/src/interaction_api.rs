use campfire_script::{ApiBuilder, ApiOwner, DataTable, MemberSpec};

use crate::use_data::UseData;
use crate::uses_data::UsesData;

/// The data of `interaction`: a unit type's `use` and `uses`. Its first cut gives scripts no
/// name it runs: the `use` order starts a use, and the use's action runs its hooks; a unit's
/// `inside` and `passengers` wait for enter and exit.
#[derive(Debug)]
pub struct InteractionApi;

impl InteractionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.plan(MemberSpec::field(
            ApiOwner::Unit,
            "inside",
            "the host it rides or garrisons in, `()` with none",
        ))
        .plan(MemberSpec::field(
            ApiOwner::Unit,
            "passengers",
            "the units inside it, by stable id",
        ))
        .shared_data(DataTable::UnitType, &["use", "uses"], &[])
        .data::<UseData>(&["action", "users"], &[])
        .data::<UsesData>(&["kind"], &[]);
    }
}
