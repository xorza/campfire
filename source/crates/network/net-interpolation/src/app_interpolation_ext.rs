use bevy_app::App;
use bevy_ecs::query::ArchetypeFilter;
use campfire_net_replication::HistoryComponent;

use crate::interpolation_bundle::InterpolationBundle;
use crate::interpolation_rule::InterpolationRule;
use crate::rule_building;

/// Interpolation rules beyond a component's default one: for a filter, a priority, or a bundle.
///
/// Each member takes its mode, whole values or diffs, from its own registration, resolved when the
/// app finishes its plugins. A member that replicates as diffs records its mode with
/// [`InterpolationRegistrationExt::interpolate`](crate::interpolation_registration_ext::InterpolationRegistrationExt::interpolate).
pub trait AppInterpolationExt {
    /// Registers `rule` for component `C` on the archetypes `F` matches.
    fn register_interpolation_rule<C, F>(&mut self, rule: InterpolationRule<C>) -> &mut Self
    where
        C: HistoryComponent,
        F: ArchetypeFilter + 'static;

    /// Registers `rule` for the components of bundle `B` together, on the archetypes `F`
    /// matches.
    fn register_bundle_interpolation_rule<B, F>(&mut self, rule: InterpolationRule<B>) -> &mut Self
    where
        B: InterpolationBundle,
        F: ArchetypeFilter + 'static;
}

impl AppInterpolationExt for App {
    fn register_interpolation_rule<C, F>(&mut self, rule: InterpolationRule<C>) -> &mut Self
    where
        C: HistoryComponent,
        F: ArchetypeFilter + 'static,
    {
        rule_building::register_component_rule::<C, F>(self, rule);
        self
    }

    fn register_bundle_interpolation_rule<B, F>(&mut self, rule: InterpolationRule<B>) -> &mut Self
    where
        B: InterpolationBundle,
        F: ArchetypeFilter + 'static,
    {
        B::register_rule::<F>(self, rule);
        self
    }
}
