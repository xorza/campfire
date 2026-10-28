//! The query filters of the kinds of entity, so a system names a kind once, the way the kind's
//! markers say it.

use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::query::{Allow, Or, With, Without};

use crate::firing_rider::FiringRider;
use crate::seeing_rider::SeeingRider;
use crate::unit_entity::UnitEntity;

/// Every unit, a delivery among them.
pub type UnitFilter = With<UnitEntity>;

/// The units that act: those in the world, and the riders that fire from their containers, which
/// the scan and the attacks name beside them (04-capabilities/combat.md, A container may let its
/// riders fire).
pub type Acting = (Allow<Disabled>, Or<(Without<Disabled>, With<FiringRider>)>);

/// The units that see: those in the world, and the riders that see from their containers, which
/// vision names beside them (04-capabilities/interaction.md, A garrison's riders see from it).
pub type Sighted = (Allow<Disabled>, Or<(Without<Disabled>, With<SeeingRider>)>);
