use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::hook::Hook;
use crate::scripts::script_api::Status;
use crate::units::hit_handle::HitHandle;

/// The script API of the deliveries that projectiles and areas make, where they run: the hit
/// their hooks record, and their `on_hit` and `on_end` hooks.
#[derive(Debug)]
pub(crate) struct DeliveriesApi;

impl DeliveriesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        HitHandle::register(api);
        api.hook(Hook::OnHit, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnEnd, Status::Runs(ApiVersion::FIRST));
    }
}
