use crate::history_component::HistoryComponent;
use crate::interpolation_fns::InterpolationFns;
use crate::interpolation_mode::InterpolationMode;
use crate::interpolation_registry;
use crate::interpolation_rule::InterpolationRule;
use crate::rule_building;
use campfire_net_replication::ComponentRegistrator;

/// Interpolation on a component's registration, which takes its mode from the registration's
/// replication kind.
#[expect(
    clippy::return_self_not_must_use,
    reason = "the call registers on the app; the returned registration only chains further calls"
)]
pub trait InterpolationRegistrationExt<'a, C, M>: ComponentRegistrator<'a, C, M> {
    /// Registers `fns` as the component's default interpolation rule, priority 1 with no
    /// filter, from whole values or diffs as the component replicates. A bundle, a filter or
    /// another priority goes through
    /// `AppInterpolationExt`.
    ///
    /// It also records the component's mode for every rule it is a member of: a component that
    /// replicates as diffs records it here, with [`InterpolationFns::disabled`] when it has no
    /// rule of its own.
    fn interpolate(self, fns: InterpolationFns<C>) -> Self
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
            InterpolationRule::new(fns),
        );
        Self::from_component_registration(registration)
    }
}
