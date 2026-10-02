use std::fmt;

use campfire_math::Num;

use crate::combat::heal::Heal;
use crate::scripts::error::CallError;
use crate::scripts::script_batch::ScriptBatch;

/// What turns each heal of the pass into its amount before the heal scale: the mode's
/// `calc_heal`, which the mode gives combat when its script defines one. Package data, not
/// state.
pub(crate) struct HealWeigher(Box<WeighFn>);

type WeighFn = dyn Fn(&mut ScriptBatch<'_>, Heal) -> Result<Num, CallError>;

impl HealWeigher {
    pub(crate) fn new(
        weigh: impl Fn(&mut ScriptBatch<'_>, Heal) -> Result<Num, CallError> + 'static,
    ) -> HealWeigher {
        HealWeigher(Box::new(weigh))
    }

    /// The amount of `heal`, in `batch`.
    pub(crate) fn weigh(&self, batch: &mut ScriptBatch<'_>, heal: Heal) -> Result<Num, CallError> {
        (self.0)(batch, heal)
    }
}

impl fmt::Debug for HealWeigher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HealWeigher")
    }
}
