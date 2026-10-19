use crate::local_timeline_sync::LocalTimelineSync;
use crate::timeline_sync::TimelineSync;
use bevy_ecs::change_detection::Tick as ChangeTick;
use bevy_ecs::prelude::*;
use bevy_ecs::system::{
    ReadOnlySystemParam, SystemAccess, SystemMeta, SystemParam, SystemParamValidationError,
};
use bevy_ecs::world::unsafe_world_cell::UnsafeWorldCell;
use campfire_net_timeline::LocalTimeline;
use core::marker::PhantomData;

/// Read-only access to the application-global [`LocalTimeline`] after it has synchronized
/// (13-net-stack/api.md, No `unsafe` but what an outside API requires).
///
/// This parameter combines the canonical simulation clock with its [`LocalTimelineSync`]
/// controller. Systems using it are skipped until synchronization is ready, and can read both the
/// current simulation tick and the input delay without fetching the controller separately.
#[derive(Debug)]
pub struct SyncedLocalTimeline<'w, 's> {
    timeline: Res<'w, LocalTimeline>,
    sync: Res<'w, LocalTimelineSync>,
    marker: PhantomData<&'s ()>,
}

impl core::ops::Deref for SyncedLocalTimeline<'_, '_> {
    type Target = LocalTimeline;

    fn deref(&self) -> &Self::Target {
        &self.timeline
    }
}

impl SyncedLocalTimeline<'_, '_> {
    /// The number of ticks added when assigning local inputs to the simulation timeline.
    pub fn input_delay(&self) -> u16 {
        self.sync.input_delay()
    }
}

// SAFETY: This parameter delegates all access to read-only resource parameters.
unsafe impl SystemParam for SyncedLocalTimeline<'_, '_> {
    type State = (
        <Res<'static, LocalTimeline> as SystemParam>::State,
        <Res<'static, LocalTimelineSync> as SystemParam>::State,
    );
    type Item<'world, 'state> = SyncedLocalTimeline<'world, 'state>;

    fn init_state(world: &mut World) -> Self::State {
        (
            <Res<'static, LocalTimeline> as SystemParam>::init_state(world),
            <Res<'static, LocalTimelineSync> as SystemParam>::init_state(world),
        )
    }

    fn init_access(
        state: &Self::State,
        system_meta: &mut SystemMeta,
        system_access: &mut SystemAccess,
        world: &mut World,
    ) {
        <Res<'static, LocalTimeline> as SystemParam>::init_access(
            &state.0,
            system_meta,
            system_access,
            world,
        );
        <Res<'static, LocalTimelineSync> as SystemParam>::init_access(
            &state.1,
            system_meta,
            system_access,
            world,
        );
    }

    #[inline]
    unsafe fn get_param<'world, 'state>(
        state: &'state mut Self::State,
        system_meta: &SystemMeta,
        world: UnsafeWorldCell<'world>,
        change_tick: ChangeTick,
    ) -> Result<Self::Item<'world, 'state>, SystemParamValidationError> {
        // SAFETY: `init_access` delegated to both read-only resources, and the caller guarantees
        // that this is the same World used by `init_state`.
        let timeline = unsafe {
            <Res<'static, LocalTimeline> as SystemParam>::get_param(
                &mut state.0,
                system_meta,
                world,
                change_tick,
            )
        }?;
        let sync = unsafe {
            <Res<'static, LocalTimelineSync> as SystemParam>::get_param(
                &mut state.1,
                system_meta,
                world,
                change_tick,
            )
        }?;
        if !sync.is_synced() {
            return Err(SystemParamValidationError::skipped::<Self>(
                "LocalTimeline is not synchronized",
            ));
        }
        Ok(SyncedLocalTimeline {
            timeline,
            sync,
            marker: PhantomData,
        })
    }
}

// SAFETY: `SyncedLocalTimeline` only delegates to read-only system parameters.
unsafe impl ReadOnlySystemParam for SyncedLocalTimeline<'_, '_> {}
