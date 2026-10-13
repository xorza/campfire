use campfire_core::{ApiBuilder, ApiVersion, HitHandle, Hook, Status};

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
