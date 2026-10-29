use core::any::Any;
use std::sync::Arc;

use bevy_ecs::component::ComponentId;
use bevy_ecs::world::FilteredEntityRef;
use campfire_net_core::NetTick;

/// Where frame interpolation reads a component's values at the last two fixed ticks when another
/// plugin keeps them, as prediction keeps a predicted component's history: the component that
/// holds them, and the fn that reads them at a tick.
///
/// A frame-interpolated archetype with the source component reads its samples through it, and
/// keeps no `FrameInterpolationHistory` of its own, so nothing copies the values each tick or repairs them after a rollback. The source
/// holds no frame state, so `SkipFrameInterpolation` on such an entity shows the newest value only
/// while it is present.
#[derive(Debug, Clone)]
pub struct FrameSource {
    pub(crate) component_id: ComponentId,
    /// The [`FrameSamplesFn`] of the component the source serves.
    samples: Arc<dyn Any + Send + Sync>,
}

/// Reads the values of `C` at `tick` and the tick before it from an entity whose read access
/// covers the source component; an entity without it has none.
pub type FrameSamplesFn<C> =
    for<'w, 's> fn(source: &FilteredEntityRef<'w, 's>, tick: NetTick) -> FrameSamples<'w, C>;

/// A component's values at the last two fixed ticks, as frame interpolation blends them.
#[derive(Debug)]
pub struct FrameSamples<'a, C> {
    pub previous: Option<&'a C>,
    pub current: Option<&'a C>,
}

impl<C> FrameSamples<'_, C> {
    /// No value at either tick.
    pub const NONE: Self = Self {
        previous: None,
        current: None,
    };
}

impl FrameSource {
    /// The source of `C` that the component `component_id` holds, read with `samples`.
    pub fn new<C: 'static>(component_id: ComponentId, samples: FrameSamplesFn<C>) -> Self {
        Self {
            component_id,
            samples: Arc::new(samples),
        }
    }

    /// The fn that reads the samples of `C`, which the source serves.
    pub(crate) fn samples<C: 'static>(&self) -> FrameSamplesFn<C> {
        *self
            .samples
            .downcast_ref()
            .expect("a frame source is read as the component it serves")
    }
}
