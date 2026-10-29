//! The typed systems of the frame rules: each component's frame history, and each rule's write of
//! its visual.
use bevy_app::prelude::*;
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_ecs::system::{ParamBuilder, QueryParamBuilder};
use bevy_ecs::world::FilteredEntityRef;
use bevy_time::{Fixed, Time};
use bevy_utils::prelude::DebugName;
use campfire_net_core::NetTick;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::HistoryComponent;
use campfire_net_timeline::LocalTimeline;
use tracing::trace;

use crate::frame_holders::FrameHolders;
use crate::frame_interpolate::FrameInterpolate;
use crate::frame_interpolation_history::FrameInterpolationHistory;
use crate::frame_phase_systems::FramePhaseSystems;
use crate::frame_registry::FrameRuleId;
use crate::frame_rule::FrameMap;
use crate::frame_source::{FrameSamples, FrameSamplesFn, FrameSource};
use crate::interpolation_sample_cx::InterpolationSampleCx;
use crate::skip_frame_interpolation::SkipFrameInterpolation;

/// Where a frame write reads a component's samples: its frame history, or, on an archetype that
/// holds the component's frame source, the source's fn at the app's tick.
#[derive(Debug)]
struct FrameSampler<C> {
    type_id: ComponentTypeId,
    source: Option<FrameSamplesFn<C>>,
}

impl<C> Clone for FrameSampler<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for FrameSampler<C> {}

impl<C: Component> FrameSampler<C> {
    fn new(source: Option<&FrameSource>) -> Self {
        Self {
            type_id: ComponentTypeId::of::<C>(),
            source: source.map(FrameSource::samples::<C>),
        }
    }

    /// The samples of an entity, `sourced` when its archetype holds the source: none for a sourced
    /// entity before the app has a tick to read them at, or for one with no frame history.
    fn samples<'a>(
        self,
        sourced: bool,
        history: Option<&'a FrameInterpolationHistory<C>>,
        entity: &FilteredEntityRef<'a, '_>,
        tick: Option<NetTick>,
    ) -> FrameSamples<'a, C> {
        if sourced && let Some(samples) = self.source {
            return tick.map_or(FrameSamples::NONE, |tick| samples(entity, tick));
        }
        history.map_or(FrameSamples::NONE, |history| FrameSamples {
            previous: history.previous_value.as_ref(),
            current: history.current_value.as_ref(),
        })
    }
}

/// Installs the system that records `C`'s frame history after each fixed tick, once for each
/// component a frame rule shows.
pub(crate) fn install_frame_history<C: HistoryComponent>(app: &mut App) {
    app.add_systems(
        FixedPostUpdate,
        FrameInterpolationHistory::<C>::update.in_set(FramePhaseSystems::Run),
    );
}

/// Installs the system that writes the visual `V` of rule `rule` from `C` by `map`, on each entity
/// whose archetype the rule writes: from `C`'s last two fixed ticks, its frame history's or its
/// `source`'s, on a frame-interpolated entity, else from its live value alone, so `V` is what each
/// entity shows. An entity with [`SkipFrameInterpolation`] shows its newest value, and its frame
/// history then holds it as its previous one too, so the frames after the skip do not blend
/// across it; a source holds no frame state. A missing sample takes the newer one, or the live
/// value. `C` is only read: it keeps the simulated value for every fixed tick, a rollback's replay
/// among them.
pub(crate) fn install_visual_write<C: HistoryComponent, V: HistoryComponent>(
    app: &mut App,
    rule: FrameRuleId,
    map: FrameMap<C, V>,
    source: Option<FrameSource>,
) {
    let sampler = FrameSampler::<C>::new(source.as_ref());
    let system = move |time: Res<'_, Time<Fixed>>,
                       timeline: Option<Res<'_, LocalTimeline>>,
                       holders: Res<'_, FrameHolders>,
                       mut query: Query<
        '_,
        '_,
        (
            Entity,
            EntityLocation,
            &C,
            Option<&mut V>,
            Option<&mut FrameInterpolationHistory<C>>,
            Has<FrameInterpolate>,
            Has<SkipFrameInterpolation>,
            FilteredEntityRef<'_, '_>,
        ),
    >,
                       mut deferred: Deferred<'_, DeferredEntityCommands>| {
        let cx = frame_sample(&time);
        let tick = timeline.map(|timeline| timeline.tick());
        let writes = holders.writes(rule);
        let sourced = holders.sourced(sampler.type_id);
        for (entity, location, live, visual, mut history, framed, skip, source) in &mut query {
            if !writes.contains(location.archetype_id) {
                continue;
            }
            let FrameSamples { previous, current } = if framed {
                let from_source = sourced.contains(location.archetype_id);
                sampler.samples(from_source, history.as_deref(), &source, tick)
            } else {
                FrameSamples::NONE
            };
            let current = current.unwrap_or(live);
            let previous = previous.filter(|_| !skip).unwrap_or(current);
            let value = map(previous, current, cx);
            trace!(
                target: "campfire_net_debug::frame_interpolation",
                kind = "frame_interpolation_apply",
                component = ?DebugName::type_name::<C>(),
                entity = ?entity,
                overstep = cx.t,
                skip,
                "wrote the visual value"
            );
            match visual {
                Some(mut visual) => {
                    visual.set_if_neq(value);
                }
                None => deferred.insert(entity, value),
            }
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
        ParamBuilder,
    )
        .build_state(app.world_mut())
        .build_system(system);
    app.add_systems(PostUpdate, system.in_set(FramePhaseSystems::Run));
}

/// The context a frame's blend reads: the fixed overstep and the fixed timestep.
fn frame_sample(time: &Time<Fixed>) -> InterpolationSampleCx {
    InterpolationSampleCx::new(
        time.overstep_fraction().clamp(0.0, 1.0),
        Some(time.timestep().as_secs_f32()),
    )
}
