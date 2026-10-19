use campfire_net_core::NetTick;

#[derive(Debug, Clone, Copy)]
pub(crate) struct UpdateHistoryCx {
    pub(crate) server_complete_tick: Option<NetTick>,
    pub(crate) current_interpolate_tick: NetTick,
}
