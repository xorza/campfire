use std::fmt::Debug;

use campfire_script::{CallError, Frame, View};
use campfire_sim::StableId;

use crate::action_target::ActionTarget;

/// What a listed effect does, its names resolved as its action loaded: the module's own type,
/// which queues its effect in a call of the action.
pub trait ListedSpec: Debug + Send + Sync {
    /// Queues its effect in the call `frame` holds, which `view` sees, where `at` places it; an
    /// error fails the call, as the same `ctx` call fails.
    fn queue(&self, at: ListedAt, frame: &mut Frame, view: &View) -> Result<(), CallError>;
}

/// Where a listed effect applies: to `to`, in a list that reached `reached`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListedAt {
    pub to: EffectTo,
    pub reached: ActionTarget,
}

impl ListedAt {
    /// The unit the effect applies to, of the call `frame` holds: the unit the list reached, or
    /// the acting unit.
    pub fn unit(self, frame: &Frame) -> StableId {
        match self.to {
            EffectTo::Reached => self
                .reached
                .unit()
                .expect("the load lets only an effect to the source reach no unit"),
            EffectTo::Source => frame
                .acting()
                .expect("an action's list runs for its acting unit"),
        }
    }
}

/// Whom an effect applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectTo {
    /// The unit the list reached: a cast's target, or the unit a delivery hit.
    #[default]
    Reached,
    /// The acting unit.
    Source,
}
