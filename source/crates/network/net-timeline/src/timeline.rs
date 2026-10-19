use crate::timeline_config::TimelineConfig;
use bevy_reflect::Reflect;
use campfire_net_core::TickInstant;
use core::ops::{Deref, DerefMut};

/// A timeline defines an independent progression of time.
///
/// A given entity can be associated with multiple timelines.
/// Each Timeline is associated with a [`TimelineConfig`] component that is used to
/// configure the timeline.
#[derive(Default, Debug, Clone, Reflect)]
pub struct Timeline<T: TimelineConfig> {
    pub state: T::State,
    pub now: TickInstant,
    #[reflect(ignore)]
    pub marker: core::marker::PhantomData<T>,
}

impl<T: TimelineConfig> Deref for Timeline<T> {
    type Target = T::State;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl<T: TimelineConfig> DerefMut for Timeline<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}
