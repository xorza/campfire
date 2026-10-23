//! Frame interpolation: each frame shows a fixed-tick component through a visual component, between
//! the component's last two fixed ticks at the fixed overstep, while the component keeps the
//! simulated value; and the blends and lerps both interpolation pipelines share.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod app_frame_interpolation_ext;
mod archetype_set;
mod frame_holders;
mod frame_interpolate;
mod frame_interpolation_history;
mod frame_interpolation_plugin;
mod frame_interpolation_systems;
mod frame_phase_systems;
mod frame_registry;
mod frame_registry_plugin;
mod frame_rule;
mod frame_source;
mod frame_systems;
mod interpolation_sample_cx;
mod lerps;
mod skip_frame_interpolation;

pub use crate::app_frame_interpolation_ext::AppFrameInterpolationExt;
pub use crate::archetype_set::ArchetypeSet;
pub use crate::frame_holders::FrameHolders;
pub use crate::frame_interpolate::FrameInterpolate;
pub use crate::frame_interpolation_history::FrameInterpolationHistory;
pub use crate::frame_interpolation_plugin::FrameInterpolationPlugin;
pub use crate::frame_interpolation_systems::FrameInterpolationSystems;
pub use crate::frame_registry::FrameRegistry;
pub use crate::frame_registry_plugin::FrameRegistryPlugin;
pub use crate::frame_rule::{FrameMap, FrameRule};
pub use crate::frame_source::{FrameSamples, FrameSamplesFn, FrameSource};
pub use crate::interpolation_sample_cx::InterpolationSampleCx;
pub use crate::lerps::{LerpFn, linear_lerp};
pub use crate::skip_frame_interpolation::SkipFrameInterpolation;
