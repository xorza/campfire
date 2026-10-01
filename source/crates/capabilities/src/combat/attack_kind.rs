use bevy_ecs::resource::Resource;

use crate::combat::damage_kind::DamageKind;

/// The kind of damage every attack deals, the mode's `attack_kind`; the first kind until a mode
/// sets it. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AttackKind(pub(crate) DamageKind);
