use bevy_ecs::resource::Resource;
use campfire_declarations::{CallbackTable, Callbacks};
use campfire_math::Num;
use campfire_script::{CallError, ScriptBatch};
use campfire_sim::Capability;

use crate::damage::Damage;
use crate::heal::Heal;

/// What turns each damage and heal of the pass into its final amount, which the mode declares:
/// its `calc_damage` and `calc_heal`, each a pure call in the pass's batch, none of a hook its
/// script does not define, which leaves the amount as it is.
#[derive(Resource, Debug, Default)]
pub struct Weighers(CallbackTable<Weigh, 1>);

/// The mode's weighers of a damage and of a heal.
#[derive(Debug, Clone, Copy)]
pub struct Weigh {
    pub damage: fn(&mut ScriptBatch<'_>, Damage) -> Result<Option<Num>, CallError>,
    pub heal: fn(&mut ScriptBatch<'_>, Heal) -> Result<Option<Num>, CallError>,
}

impl Callbacks for Weighers {
    type Key = ();
    type Callback = Weigh;

    fn capability((): ()) -> Option<Capability> {
        Some(Capability::Mode)
    }

    fn register(&mut self, (): (), weigh: Weigh) {
        self.0.register(0, weigh);
    }
}

impl Weighers {
    /// The mode's weighers, when a mode declares them.
    pub(crate) const fn get(&self) -> Option<Weigh> {
        self.0.get(0)
    }
}
