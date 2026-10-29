use core::ops::{Deref, DerefMut};

use bevy_reflect::Reflect;
use campfire_net_core::TickInstant;

use crate::timeline_setup::TimelineSetup;

/// A timeline defines an independent progression of time.
///
/// A given entity can be associated with multiple timelines.
/// Each Timeline is associated with a [`TimelineSetup`] component that is used to
/// configure the timeline.
#[derive(Default, Debug, Clone, Reflect)]
pub struct Timeline<T: TimelineSetup> {
    pub state: T::State,
    pub now: TickInstant,
    #[reflect(ignore)]
    marker: core::marker::PhantomData<T>,
}

impl<T: TimelineSetup> Deref for Timeline<T> {
    type Target = T::State;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl<T: TimelineSetup> DerefMut for Timeline<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}
