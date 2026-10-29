use campfire_script::{ApiBuilder, ApiHolder, MemberDecl, Unit};

use crate::use_data::UseData;
use crate::uses_data::UsesData;

/// The data of `interaction`: a unit type's `use` and `uses`. It gives scripts no name it runs: the
/// `use` and `enter` orders start a use and an entry, whose actions run their hooks; a unit's
/// `inside` and `passengers` wait for the script view to hold riders
/// (04-capabilities/interaction.md, The casts' pipeline runs a use and an entry).
#[derive(Debug)]
pub struct InteractionApi;

impl InteractionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.plan(MemberDecl::field(
            ApiHolder::of::<Unit>(),
            "inside",
            "the host it rides or garrisons in, `()` with none",
        ))
        .plan(MemberDecl::field(
            ApiHolder::of::<Unit>(),
            "passengers",
            "the units inside it, by stable id",
        ))
        .data::<UseData>(&[])
        .data::<UsesData>(&[]);
    }
}
