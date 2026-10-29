use campfire_net_replication::ComponentRegistrator;
use campfire_net_replication::HistoryComponent;

use crate::interpolation_fns::InterpolationFns;
use crate::interpolation_mode::InterpolationMode;
use crate::interpolation_registry;
use crate::interpolation_rule::InterpolationRule;
use crate::rule_building;

/// Interpolation on a component's registration, which takes its mode from the registration's
/// replication kind.
#[expect(
    clippy::return_self_not_must_use,
    reason = "the call registers on the app; the returned registration only chains further calls"
)]
pub trait InterpolationRegistrationExt<'a, C, M>: ComponentRegistrator<'a, C, M> {
    /// Registers `fns` as the component's default interpolation rule, priority 0, the lowest,
    /// with no filter, from whole values or diffs as the component replicates, so any rule
    /// registered for it beside wins on the archetypes it matches. A bundle, a filter or another
    /// priority goes through `AppInterpolationExt` (11-net-stack/api.md, A component's
    /// `.interpolate` is its default rule).
    ///
    /// It also records the component's mode for every rule it is a member of, as
    /// [`InterpolationRegistrationExt::interpolate_in_rules`] does.
    fn interpolate(self, fns: InterpolationFns<C>) -> Self
    where
        C: HistoryComponent,
        M: InterpolationMode<C>;

    /// Records the component's mode, whole values or diffs as it replicates, for the rules it is
    /// a member of, and registers no rule of its own: a component that replicates as diffs and
    /// interpolates only within a bundle records it so.
    fn interpolate_in_rules(self) -> Self
    where
        C: HistoryComponent,
        M: InterpolationMode<C>;
}

impl<'a, C, M, R> InterpolationRegistrationExt<'a, C, M> for R
where
    R: ComponentRegistrator<'a, C, M>,
{
    fn interpolate(self, fns: InterpolationFns<C>) -> Self
    where
        C: HistoryComponent,
        M: InterpolationMode<C>,
    {
        let registration = self.into_component_registration();
        M::record(&mut interpolation_registry::open(registration.app));
        rule_building::register_component_rule::<C, ()>(
            registration.app,
            InterpolationRule::new(fns).priority(rule_building::DEFAULT_RULE_PRIORITY),
        );
        Self::from_component_registration(registration)
    }

    fn interpolate_in_rules(self) -> Self
    where
        C: HistoryComponent,
        M: InterpolationMode<C>,
    {
        let registration = self.into_component_registration();
        M::record(&mut interpolation_registry::open(registration.app));
        Self::from_component_registration(registration)
    }
}
