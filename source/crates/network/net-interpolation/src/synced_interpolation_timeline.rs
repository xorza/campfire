use crate::interpolation_timeline::InterpolationTimeline;
use bevy_ecs::change_detection::Tick as ChangeTick;
use bevy_ecs::prelude::*;
use bevy_ecs::system::{
    ReadOnlySystemParam, SystemAccess, SystemMeta, SystemParam, SystemParamValidationError,
};
use bevy_ecs::world::unsafe_world_cell::UnsafeWorldCell;
use campfire_net_sync::TimelineSync;
use core::marker::PhantomData;

/// Read-only access to the application-global [`InterpolationTimeline`] after it has synchronized.
///
/// Systems using this parameter are skipped until [`InterpolationTimeline::is_synced`] returns
/// true. The parameter dereferences directly to the presentation timeline.
#[derive(Debug)]
pub struct SyncedInterpolationTimeline<'w, 's> {
    timeline: Res<'w, InterpolationTimeline>,
    marker: PhantomData<&'s ()>,
}

impl core::ops::Deref for SyncedInterpolationTimeline<'_, '_> {
    type Target = InterpolationTimeline;

    fn deref(&self) -> &Self::Target {
        &self.timeline
    }
}

// SAFETY: This parameter delegates all access to the read-only `Res<InterpolationTimeline>`
// parameter.
unsafe impl SystemParam for SyncedInterpolationTimeline<'_, '_> {
    type State = <Res<'static, InterpolationTimeline> as SystemParam>::State;
    type Item<'world, 'state> = SyncedInterpolationTimeline<'world, 'state>;

    fn init_state(world: &mut World) -> Self::State {
        <Res<'static, InterpolationTimeline> as SystemParam>::init_state(world)
    }

    fn init_access(
        state: &Self::State,
        system_meta: &mut SystemMeta,
        system_access: &mut SystemAccess,
        world: &mut World,
    ) {
        <Res<'static, InterpolationTimeline> as SystemParam>::init_access(
            state,
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
        // SAFETY: `init_access` delegated to `Res<InterpolationTimeline>`, and the caller
        // guarantees that this is the same World used by `init_state`.
        let timeline = unsafe {
            <Res<'static, InterpolationTimeline> as SystemParam>::get_param(
                state,
                system_meta,
                world,
                change_tick,
            )
        }?;
        if !timeline.is_synced() {
            return Err(SystemParamValidationError::skipped::<Self>(
                "InterpolationTimeline is not synchronized",
            ));
        }
        Ok(SyncedInterpolationTimeline {
            timeline,
            marker: PhantomData,
        })
    }
}

// SAFETY: `SyncedInterpolationTimeline` only delegates to read-only system parameters.
unsafe impl ReadOnlySystemParam for SyncedInterpolationTimeline<'_, '_> {}
