use crate::areas::area_effect::AreaEffect;
use crate::combat::combat_effect::CombatEffect;
use crate::mode::mode_effect::ModeEffect;
use crate::orders::ai_order::AiOrder;
use crate::progression::progression_effect::ProgressionEffect;
use crate::projectiles::projectile_effect::ProjectileEffect;
use crate::stats::modifier_effect::ModifierEffect;

/// An effect a call queued, which applies when the call returns, in the order queued: each
/// capability applies its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Effect {
    Combat(CombatEffect),
    Modifier(ModifierEffect),
    Order(AiOrder),
    Mode(ModeEffect),
    Progression(ProgressionEffect),
    Projectile(ProjectileEffect),
    Area(AreaEffect),
}
