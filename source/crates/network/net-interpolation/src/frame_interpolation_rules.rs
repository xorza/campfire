//! The typed frame-interpolation systems of the interpolation rules, which run on the entities
//! with `FrameInterpolate` that the holders give each rule and component.

use crate::frame_interpolate::FrameInterpolate;
use crate::frame_interpolation_history::FrameInterpolationHistory;
use crate::frame_interpolation_systems::FrameInterpolationSystems;
use crate::frame_source::{FrameSamples, FrameSamplesFn, FrameSource};
use crate::frame_stage::FrameStage;
use crate::history_component::HistoryComponent;
use crate::interpolate::set_if_neq;
use crate::interpolation_fn::InterpolationFn;
use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_rule_id::InterpolationRuleId;
use crate::interpolation_sample_cx::InterpolationSampleCx;
use crate::skip_frame_interpolation::SkipFrameInterpolation;
use bevy_app::prelude::*;
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_ecs::system::{ParamBuilder, QueryParamBuilder};
use bevy_ecs::world::FilteredEntityRef;
use bevy_time::{Fixed, Time};
use bevy_utils::prelude::DebugName;
use campfire_net_core::NetTick;
use campfire_net_replication::ComponentTypeId;
use campfire_net_timeline::LocalTimeline;
use std::borrow::Cow;
use tracing::trace;

/// Where a frame system reads one member's samples: its frame history, or, on an archetype that
/// holds the member's frame source, the source's fn at the app's tick.
#[derive(Debug)]
pub(crate) struct FrameSampler<C> {
    pub(crate) type_id: ComponentTypeId,
    source: Option<FrameSamplesFn<C>>,
}

impl<C> Clone for FrameSampler<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for FrameSampler<C> {}

impl<C: Component> FrameSampler<C> {
    pub(crate) fn new(source: Option<&FrameSource>) -> Self {
        Self {
            type_id: ComponentTypeId::of::<C>(),
            source: source.map(FrameSource::samples::<C>),
        }
    }

    /// The samples of an entity, `sourced` when its archetype holds the source: none for a
    /// sourced entity before the app has a tick to read them at, or for one with no frame
    /// history.
    pub(crate) fn samples<'a>(
        &self,
        sourced: bool,
        history: Option<&'a FrameInterpolationHistory<C>>,
        entity: &FilteredEntityRef<'a, '_>,
        tick: Option<NetTick>,
    ) -> Option<FrameSamples<'a, C>> {
        if sourced && let Some(samples) = self.source {
            return Some(samples(entity, tick?));
        }
        history.map(|history| FrameSamples {
            previous: history.previous_value.as_ref(),
            current: history.current_value.as_ref(),
        })
    }
}

/// Installs the systems that keep and restore `C`'s frame history, once for each component a
/// rule holds the frame history of. The restore writes live `C` from its newest frame sample, the
/// current value of its frame history or its source's at the app's tick: frame interpolation
/// writes visual values during `PostUpdate`, and the fixed loop must not read them.
pub(crate) fn install_frame_history<C: HistoryComponent>(
    app: &mut App,
    source: Option<FrameSource>,
) {
    app.add_systems(
        FixedPostUpdate,
        FrameInterpolationHistory::<C>::update
            .in_set(FrameInterpolationSystems::Update)
            .in_set(FrameStage::Run),
    );
    let sampler = FrameSampler::<C>::new(source.as_ref());
    let restore = move |timeline: Option<Res<'_, LocalTimeline>>,
                        holders: Res<'_, InterpolationHolders>,
                        mut query: Query<
        '_,
        '_,
        (
            Entity,
            EntityLocation,
            &mut C,
            Option<&FrameInterpolationHistory<C>>,
            FilteredEntityRef<'_, '_>,
        ),
        With<FrameInterpolate>,
    >| {
        let tick = timeline.map(|timeline| timeline.tick());
        let maintained = holders.frame.history(sampler.type_id);
        let sourced = holders.frame.sourced(sampler.type_id);
        for (entity, location, live, history, source) in &mut query {
            if !maintained.contains(location.archetype_id) {
                continue;
            }
            let Some(FrameSamples {
                current: Some(current_value),
                ..
            }) = sampler.samples(
                sourced.contains(location.archetype_id),
                history,
                &source,
                tick,
            )
            else {
                continue;
            };
            if !set_if_neq(live, Cow::Borrowed(current_value)) {
                continue;
            }
            trace!(
                target: "campfire_net_debug::frame_interpolation",
                kind = "frame_interpolation_restore",
                schedule = "RunFixedMainLoop",
                sample_point = "RunFixedMainLoop",
                component = ?DebugName::type_name::<C>(),
                entity = ?entity,
                "restored non-interpolated component value"
            );
        }
    };
    let restore = (
        ParamBuilder,
        ParamBuilder,
        QueryParamBuilder::new(move |builder| {
            if let Some(source) = &source {
                builder.optional(|builder| {
                    builder.ref_id(source.component_id);
                });
            }
        }),
    )
        .build_state(app.world_mut())
        .build_system(restore);
    app.add_systems(
        RunFixedMainLoop,
        restore
            .in_set(FrameInterpolationSystems::Restore)
            .in_set(FrameStage::Run),
    );
}

/// Installs the system that applies the component rule `rule_id` of `C` to the frame, with
/// `interpolation`, reading `C`'s samples from its frame history or from `source`, on each entity
/// whose frame apply the rule holds. An entity with [`SkipFrameInterpolation`] takes its newest
/// value, and its frame history then holds it as its previous one too, so the frames after the
/// skip do not blend across it; a source holds no frame state.
pub(crate) fn install_component_frame_interpolation<C: HistoryComponent>(
    app: &mut App,
    rule_id: InterpolationRuleId,
    interpolation: InterpolationFn<C>,
    source: Option<FrameSource>,
) {
    let sampler = FrameSampler::<C>::new(source.as_ref());
    let system = move |time: Res<'_, Time<Fixed>>,
                       timeline: Option<Res<'_, LocalTimeline>>,
                       holders: Res<'_, InterpolationHolders>,
                       mut query: Query<
        '_,
        '_,
        (
            Entity,
            EntityLocation,
            &mut C,
            Option<&mut FrameInterpolationHistory<C>>,
            Has<SkipFrameInterpolation>,
            FilteredEntityRef<'_, '_>,
        ),
        With<FrameInterpolate>,
    >| {
        let sample = frame_sample(&time);
        let tick = timeline.map(|timeline| timeline.tick());
        let held = holders.frame.apply(rule_id);
        let sourced = holders.frame.sourced(sampler.type_id);
        let interpolation = interpolation.component();
        for (entity, location, live, mut history, skip, source) in &mut query {
            if !held.contains(location.archetype_id) {
                continue;
            }
            let Some(FrameSamples {
                previous,
                current: Some(current_value),
            }) = sampler.samples(
                sourced.contains(location.archetype_id),
                history.as_deref(),
                &source,
                tick,
            )
            else {
                continue;
            };
            let interpolated = if skip {
                trace!(
                    target: "campfire_net_debug::frame_interpolation",
                    kind = "frame_interpolation_skipped",
                    schedule = "PostUpdate",
                    sample_point = "PostUpdate",
                    component = ?DebugName::type_name::<C>(),
                    entity = ?entity,
                    current_value_present = true,
                    "skipped frame interpolation"
                );
                Cow::Borrowed(current_value)
            } else if let Some(previous_value) = previous {
                Cow::Owned(interpolation(previous_value, current_value, sample))
            } else {
                trace!(
                    component = ?DebugName::type_name::<C>(),
                    entity = ?entity,
                    "No previous value, skipping visual interpolation"
                );
                Cow::Borrowed(current_value)
            };
            trace!(
                target: "campfire_net_debug::frame_interpolation",
                kind = "frame_interpolation_apply",
                schedule = "PostUpdate",
                sample_point = "PostUpdate",
                component = ?DebugName::type_name::<C>(),
                entity = ?entity,
                overstep = sample.t,
                "applied frame interpolation"
            );
            let _ = set_if_neq(live, interpolated);
            if skip && let Some(history) = &mut history {
                let history = history.bypass_change_detection();
                history.previous_value.clone_from(&history.current_value);
            }
        }
    };
    let system = (
        ParamBuilder,
        ParamBuilder,
        ParamBuilder,
        QueryParamBuilder::new(move |builder| {
            if let Some(source) = &source {
                builder.optional(|builder| {
                    builder.ref_id(source.component_id);
                });
            }
        }),
    )
        .build_state(app.world_mut())
        .build_system(system);
    app.add_systems(
        PostUpdate,
        system
            .in_set(FrameInterpolationSystems::Interpolate)
            .in_set(FrameStage::Run),
    );
}

/// The context a frame's blend reads: the fixed overstep and the fixed timestep.
pub(crate) fn frame_sample(time: &Time<Fixed>) -> InterpolationSampleCx {
    InterpolationSampleCx::new(
        time.overstep_fraction().clamp(0.0, 1.0),
        Some(time.timestep().as_secs_f32()),
    )
}
