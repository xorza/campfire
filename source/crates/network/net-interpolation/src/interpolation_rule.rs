use crate::interpolation_fns::InterpolationFns;

/// An interpolation rule a caller registers: its functions, and the priority it takes among the
/// rules that match an archetype.
///
/// Rules are evaluated per interpolated archetype and component type. If multiple rules match
/// the same archetype, priority is resolved independently for history and apply behavior.
/// Histories can represent temporarily absent members, while apply requires every live member.
/// A bundle may therefore maintain `(A, B)` histories while a standalone rule applies `A` until
/// `B` becomes live again.
#[derive(Debug, Clone)]
pub struct InterpolationRule<S> {
    pub(crate) fns: InterpolationFns<S>,
    pub(crate) priority: Option<usize>,
}

impl<S> InterpolationRule<S> {
    pub const fn new(fns: InterpolationFns<S>) -> Self {
        Self {
            fns,
            priority: None,
        }
    }

    /// The priority used to choose between matching interpolation rules: higher values are
    /// selected first, and matching rules with the same priority use registration order, with
    /// earlier registrations selected first.
    ///
    /// The default is the number of components the rule targets, so a default
    /// `(Position, Rotation)` rule wins over default single-component rules on the same
    /// archetype.
    #[must_use]
    pub const fn priority(mut self, priority: usize) -> Self {
        self.priority = Some(priority);
        self
    }
}
