use crate::frame_source::FrameSource;
use bevy_app::App;

/// What a member whose rule holds its frame history installs, once for its component: the
/// systems that record and restore it, which read the samples of a frame source where it is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameHistoryFns {
    pub(crate) install: fn(&mut App, Option<FrameSource>),
}
