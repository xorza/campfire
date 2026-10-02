use campfire_sim::Capability;

use crate::navigation::paths_column::PathsColumn;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::units::unit::Unit;

/// The script API of `navigation`: `unit.path`.
#[derive(Debug)]
pub(crate) struct NavigationApi;

impl NavigationApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let path = MemberSpec::field(
            ApiOwner::Unit,
            "path",
            "the name of the path it walks, `()` with none",
        )
        .capability(Capability::Navigation);
        api.bind(path, |unit: &mut Unit| PathsColumn::path(unit));
    }
}
