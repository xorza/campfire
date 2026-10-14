#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

use campfire_declare::{ByType, Declarations, InstallCx, Module};
use campfire_sim::Capability;

mod interaction_api;
mod use_data;
mod use_slot;
mod use_spec;
mod uses_data;
mod using;

pub use crate::interaction_api::InteractionApi;
pub use crate::use_data::UseData;
pub use crate::use_slot::UseSlot;
pub use crate::use_spec::UseSpec;
pub use crate::uses_data::UsesData;
pub use crate::using::{UnitUse, Using};

/// The `interaction` capability: units that use objects, each object's use an action of the
/// `use` kind, which a user's use slot runs through the abilities' pipeline: a `use` action needs
/// `abilities` beside it, which the load checks.
#[derive(Debug)]
pub struct Interaction;

impl Module for Interaction {
    const CAPABILITY: Option<Capability> = Some(Capability::Interaction);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.book::<ByType<UseSpec>>();
        d.book::<ByType<UseSlot>>();
        d.api(InteractionApi::register);
    }

    /// Adds nothing to the schedule: the `use` order of `orders` fills a use slot, and the
    /// abilities' casts run it.
    fn install(_: &mut InstallCx<'_>) {}
}
