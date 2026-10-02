use campfire_sim::Capability;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::script_api::{DataTable, MemberSpec, Status};
use crate::units::tag_effect::TagEffect;

/// The script API and data of `vision` beside the queries the view answers: the sight range,
/// and the planned reveal and true sight.
#[derive(Debug)]
pub(crate) struct VisionApi;

impl VisionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.plan(
            MemberSpec::call(
                "reveal",
                "(pos, radius, ms)",
                "shows the source's team what is within `radius` of `pos`",
            )
            .capability(Capability::Vision),
        )
        .tag_effect(TagEffect::Hidden, Status::Runs(ApiVersion::FIRST))
        .tag_effect(TagEffect::Detects, Status::Runs(ApiVersion::FIRST))
        .data(DataTable::Vision, &["sight_range"], &[]);
    }
}
