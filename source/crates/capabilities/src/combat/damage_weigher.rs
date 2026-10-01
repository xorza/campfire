use std::fmt;

use campfire_math::Num;

use crate::combat::damage::Damage;
use crate::scripts::error::CallError;
use crate::scripts::script_batch::ScriptBatch;

/// What turns each damage of the pass into its final amount: the mode's `calc_damage`, which
/// the mode gives combat when its script defines one. Package data, not state.
pub(crate) struct DamageWeigher(Box<WeighFn>);

type WeighFn = dyn Fn(&mut ScriptBatch<'_>, Damage) -> Result<Num, CallError>;

impl DamageWeigher {
    pub(crate) fn new(
        weigh: impl Fn(&mut ScriptBatch<'_>, Damage) -> Result<Num, CallError> + 'static,
    ) -> DamageWeigher {
        DamageWeigher(Box::new(weigh))
    }

    /// The final amount of `damage`, in `batch`.
    pub(crate) fn weigh(
        &self,
        batch: &mut ScriptBatch<'_>,
        damage: Damage,
    ) -> Result<Num, CallError> {
        (self.0)(batch, damage)
    }
}

impl fmt::Debug for DamageWeigher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DamageWeigher")
    }
}
