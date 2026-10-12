use crate::combat::combat_event::CombatEvents;
use crate::combat::damage::DamageWeigher;
use crate::combat::heal::HealWeigher;

/// The scripts combat's passes call, each none when no script defines it: the mode's
/// `calc_damage` and `calc_heal`, and the modifiers that hear combat's events. A non-send
/// resource, as each holds the script `ctx`; a pass clones it, which shares the scripts, while it
/// runs.
#[derive(Debug, Default, Clone)]
pub(crate) struct CombatHooks {
    pub(crate) weigher: Option<DamageWeigher>,
    pub(crate) healer: Option<HealWeigher>,
    pub(crate) events: Option<CombatEvents>,
}

impl CombatHooks {
    /// Whether any script runs in a pass.
    pub(crate) const fn scripted(&self) -> bool {
        self.weigher.is_some() || self.healer.is_some() || self.events.is_some()
    }
}
