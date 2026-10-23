//! The rules of a tuple of components, `(C1, C2, ...)`, each of which installs its own typed
//! systems.

use crate::interpolate::{history_bracket, set_if_neq};
use crate::interpolation_fn::InterpolationFn;
use crate::interpolation_fns::{InterpolationFns, InterpolationPipeline};
use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_phase_systems::InterpolationPhaseSystems;
use crate::interpolation_registry;
use crate::interpolation_rule::InterpolationRule;
use crate::interpolation_rule_component::InterpolationRuleComponent;
use crate::interpolation_rule_id::InterpolationRuleId;
use crate::interpolation_timeline::InterpolationTimeline;
use crate::into_interpolation_fn::{IntoCxLerpFn, IntoLerpFn};
use crate::rule_building::{self, insert_rule, resolve_member};
use crate::sample_ticks;
use bevy_app::prelude::*;
use bevy_ecs::archetype::Archetype;
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_ecs::query::ArchetypeFilter;
use campfire_net_core::NetTick;
use campfire_net_frame_interpolation::InterpolationSampleCx;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::HistoryState;
use campfire_net_replication::Interpolated;
use campfire_net_timeline::NetworkTimeline;
use campfire_net_timeline::TimelineRate;
use std::borrow::Cow;

/// Tuple of components that can be interpolated by one rule.
///
/// Tuple interpolation stores each component in its own history, samples every
/// history at shared ticks around the interpolation tick. Members without an
/// update at a shared tick carry their latest present value forward, so tuple
/// interpolation does not require identical per-component history entries.
///
/// The stack implements this trait for tuples of 2 to 8 distinct
/// [`HistoryComponent`] types.
pub trait InterpolationBundle: private::Sealed + 'static {
    /// Number of components in the interpolation target.
    ///
    /// This is used as the default priority, so a default tuple rule takes
    /// priority over matching rules for smaller overlapping tuples or
    /// individual components.
    #[doc(hidden)]
    const COMPONENT_COUNT: usize;

    /// Registers an interpolation rule for this tuple target.
    #[doc(hidden)]
    fn register_rule<F>(app: &mut App, rule: InterpolationRule<Self>)
    where
        Self: Sized,
        F: ArchetypeFilter + 'static;
}

/// The marker of a bundle's fn shape, which reads each endpoint as a tuple of references to the
/// members.
#[derive(Debug)]
pub struct BundleShape;

mod private {
    #[expect(unnameable_types, reason = "sealed: only this crate implements it")]
    pub trait Sealed {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PresentHistoryBracket {
    start_tick: NetTick,
    end_tick: Option<NetTick>,
}

impl PresentHistoryBracket {
    fn intersection(self, other: Self) -> Self {
        Self {
            start_tick: self.start_tick.max(other.start_tick),
            end_tick: match (self.end_tick, other.end_tick) {
                (Some(first), Some(second)) => Some(first.min(second)),
                (Some(tick), None) | (None, Some(tick)) => Some(tick),
                (None, None) => None,
            },
        }
    }
}

fn present_history_bracket<C>(
    history: &ConfirmedHistory<C>,
    interpolation_tick: NetTick,
) -> Option<PresentHistoryBracket> {
    let bracket = history_bracket(history, interpolation_tick)?;
    bracket.start.value.value()?;
    Some(PresentHistoryBracket {
        start_tick: bracket.start.tick,
        end_tick: bracket.end.map(|end| end.tick),
    })
}

/// A present bundle endpoint value at `tick`.
///
/// If `tick` is a removal boundary, the preceding value is returned so the
/// bundle remains continuous until the presence pass removes the component.
fn present_history_value_at<C>(history: &ConfirmedHistory<C>, tick: NetTick) -> Option<&C> {
    match history.get_state_at(tick) {
        Some(HistoryState::Updated(value)) => Some(value),
        Some(HistoryState::Removed) => history.get_state_before(tick)?.value(),
        None => history.get_state_at_or_before(tick)?.value(),
    }
}

pub(crate) trait TupleInterpolationBundle: InterpolationBundle {
    /// Installs the system that applies the bundle rule `rule_id`, with `interpolation`, to the
    /// live members of each entity whose delayed apply the rule holds.
    fn install_interpolation(
        app: &mut App,
        rule_id: InterpolationRuleId,
        interpolation: InterpolationFn<Self>,
    ) where
        Self: Sized;

    /// Builds the self-contained component members stored by the bundle rule.
    fn rule_members(
        app: &mut App,
        pipeline: InterpolationPipeline,
    ) -> Vec<InterpolationRuleComponent>;
}

/// Builds the rule of bundle `B`, filtered by `F`, and installs its systems.
fn install_bundle_rule<B, F>(
    app: &mut App,
    fns: InterpolationFns<B>,
    priority: usize,
) -> InterpolationRuleId
where
    B: TupleInterpolationBundle,
    F: ArchetypeFilter + 'static,
{
    let members = B::rule_members(app, fns.pipeline());
    let rule_id = insert_rule::<F>(app, members, priority);
    if rule_building::runs_delayed(app) && fns.applies_interpolation_component() {
        let interpolation = fns.interpolation.expect("an applying rule interpolates");
        B::install_interpolation(app, rule_id, interpolation);
    }
    rule_id
}

/// Registers `rule` for bundle `B`, built when the app finishes its plugins.
fn register_bundle_rule<B, F>(app: &mut App, rule: InterpolationRule<B>)
where
    B: TupleInterpolationBundle,
    F: ArchetypeFilter + 'static,
{
    QueryState::<&Archetype, F>::new(app.world_mut());
    let priority = rule.priority.unwrap_or(B::COMPONENT_COUNT);
    let fns = rule.fns;
    interpolation_registry::open(app).defer(Box::new(move |app: &mut App| {
        install_bundle_rule::<B, F>(app, fns, priority);
    }));
}

macro_rules! impl_interpolation_bundle {
    (
        $N:tt,
        (
            $C0:ident,
            $history0:ident,
            $start0:ident,
            $end_value0:ident,
            $output0:ident,
            $live0:ident
        ),
        $(
            (
                $C:ident,
                $history:ident,
                $start:ident,
                $end_value:ident,
                $output:ident,
                $live:ident
            )
        ),+
        $(,)?
    ) => {
        impl<$C0, $($C),+> private::Sealed for ($C0, $($C,)+)
        where
            $C0: HistoryComponent,
            $($C: HistoryComponent),+
        {}

        impl<$C0, $($C),+> InterpolationBundle for ($C0, $($C,)+)
        where
            $C0: HistoryComponent,
            $($C: HistoryComponent),+
        {
            const COMPONENT_COUNT: usize = $N;

            fn register_rule<F>(app: &mut App, rule: InterpolationRule<Self>)
            where
                F: ArchetypeFilter + 'static,
            {
                register_bundle_rule::<Self, F>(app, rule);
            }
        }

        impl<$C0, $($C),+, F> IntoLerpFn<($C0, $($C,)+), BundleShape> for F
        where
            $C0: HistoryComponent,
            $($C: HistoryComponent,)+
            F: Fn((&$C0, $(&$C,)+), (&$C0, $(&$C,)+), f32) -> ($C0, $($C,)+)
                + Send
                + Sync
                + 'static,
        {
            fn into_interpolation_fn(self) -> InterpolationFn<($C0, $($C,)+)> {
                let interpolation: Box<
                    dyn Fn((&$C0, $(&$C,)+), (&$C0, $(&$C,)+), InterpolationSampleCx)
                            -> ($C0, $($C,)+)
                        + Send
                        + Sync,
                > = Box::new(move |start, end, cx: InterpolationSampleCx| {
                    self(start, end, cx.t)
                });
                InterpolationFn::erase(interpolation)
            }
        }

        impl<$C0, $($C),+, F> IntoCxLerpFn<($C0, $($C,)+), BundleShape> for F
        where
            $C0: HistoryComponent,
            $($C: HistoryComponent,)+
            F: Fn((&$C0, $(&$C,)+), (&$C0, $(&$C,)+), InterpolationSampleCx)
                    -> ($C0, $($C,)+)
                + Send
                + Sync
                + 'static,
        {
            fn into_interpolation_fn(self) -> InterpolationFn<($C0, $($C,)+)> {
                let interpolation: Box<
                    dyn Fn((&$C0, $(&$C,)+), (&$C0, $(&$C,)+), InterpolationSampleCx)
                            -> ($C0, $($C,)+)
                        + Send
                        + Sync,
                > = Box::new(self);
                InterpolationFn::erase(interpolation)
            }
        }

        impl<$C0, $($C),+> TupleInterpolationBundle for ($C0, $($C,)+)
        where
            $C0: HistoryComponent,
            $($C: HistoryComponent),+
        {
            fn install_interpolation(
                app: &mut App,
                rule_id: InterpolationRuleId,
                interpolation: InterpolationFn<Self>,
            ) {
                let system = move |timeline: Res<'_, InterpolationTimeline>,
                                   tick_rate: Option<Res<'_, TimelineRate>>,
                                   holders: Res<'_, InterpolationHolders>,
                                   mut query: Query<
                    '_,
                    '_,
                    (
                        EntityLocation,
                        (&mut $C0, $(&mut $C,)+),
                        (&ConfirmedHistory<$C0>, $(&ConfirmedHistory<$C>,)+),
                    ),
                    With<Interpolated>,
                >| {
                    let interpolation_tick = timeline.now().tick();
                    let interpolation_overstep = timeline.overstep().to_f32();
                    let tick_rate = tick_rate.map(|rate| rate.0);
                    let held = holders.apply(rule_id);
                    let interpolation = interpolation
                        .typed::<dyn Fn((&$C0, $(&$C,)+), (&$C0, $(&$C,)+), InterpolationSampleCx)
                                -> ($C0, $($C,)+)
                            + Send
                            + Sync>();
                    for (location, ($live0, $($live,)+), ($history0, $($history,)+)) in &mut query {
                        if !held.contains(location.archetype_id) {
                            continue;
                        }
                        let Some(mut shared_bracket) =
                            present_history_bracket($history0, interpolation_tick)
                        else {
                            continue;
                        };
                        $(
                            shared_bracket = match present_history_bracket(
                                $history,
                                interpolation_tick,
                            ) {
                                Some(bracket) => shared_bracket.intersection(bracket),
                                None => continue,
                            };
                        )+
                        // Bundle members may have different immediate brackets
                        // when replication omits unchanged components. Use the
                        // intersection around the interpolation tick: the latest
                        // member start and the earliest available member end. A
                        // member without an entry at either shared tick carries
                        // its latest present value forward.
                        let Some($start0) = present_history_value_at(
                            $history0,
                            shared_bracket.start_tick,
                        ) else {
                            continue;
                        };
                        $(
                            let Some($start) = present_history_value_at(
                                $history,
                                shared_bracket.start_tick,
                            ) else {
                                continue;
                            };
                        )+

                        let ($output0, $($output,)+) = if let Some(shared_end_tick) = shared_bracket.end_tick {
                            let Some($end_value0) = present_history_value_at(
                                $history0,
                                shared_end_tick,
                            ) else {
                                continue;
                            };
                            $(
                                let Some($end_value) = present_history_value_at(
                                    $history,
                                    shared_end_tick,
                                ) else {
                                    continue;
                                };
                            )+
                            let ($output0, $($output,)+) = interpolation(
                                ($start0, $($start,)+),
                                ($end_value0, $($end_value,)+),
                                sample_ticks::sample_of_ticks(
                                    shared_bracket.start_tick,
                                    shared_end_tick,
                                    interpolation_tick,
                                    interpolation_overstep,
                                    tick_rate,
                                ),
                            );
                            (Cow::Owned($output0), $(Cow::Owned($output),)+)
                        } else {
                            (Cow::Borrowed($start0), $(Cow::Borrowed($start),)+)
                        };

                        let _ = set_if_neq($live0, $output0);
                        $(
                            let _ = set_if_neq($live, $output);
                        )+
                    }
                };
                app.add_systems(Update, system.in_set(InterpolationPhaseSystems::Apply));
            }

            fn rule_members(
                app: &mut App,
                pipeline: InterpolationPipeline,
            ) -> Vec<InterpolationRuleComponent> {
                std::vec![
                    resolve_member::<$C0>(app, pipeline),
                    $(resolve_member::<$C>(app, pipeline)),+
                ]
            }
        }
    };
}

variadics_please::all_tuples_with_size!(
    impl_interpolation_bundle,
    2,
    8,
    C,
    history,
    start,
    end_value,
    output,
    live
);

#[cfg(test)]
mod tests;
