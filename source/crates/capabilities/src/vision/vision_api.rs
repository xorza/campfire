use campfire_sim::Capability;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::{DataTable, MemberSpec};

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
        .data(DataTable::Vision, &["sight_range"], &["true_sight"]);
    }
}
