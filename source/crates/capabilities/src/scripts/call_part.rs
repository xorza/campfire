use std::any::{Any, TypeId};
use std::fmt;

use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;

use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;
use crate::scripts::hook::ScriptRole;

/// A capability's own part of the call frame: what a call reads besides the core's, such as its
/// params or the mode's state, and what it wrote that applies once its effects did. The core
/// names none of them.
pub(crate) trait CallPart: Any + fmt::Debug {
    /// Readies it for the call `start`, which runs in `world`; an error fails the call.
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError>;

    /// The running call's param `name`, when this part holds the params of a call of `role`.
    fn param_named(&self, role: Option<ScriptRole>, name: &str) -> Option<Dynamic>;

    /// Applies what the call wrote to it, once its effects applied.
    fn apply(&mut self, world: &mut World);
}

/// The parts of the frame, each of its own type, in the order they were added.
#[derive(Debug, Default)]
pub(crate) struct CallParts(Vec<Box<dyn CallPart>>);

impl CallParts {
    pub(crate) fn add<P: CallPart>(&mut self, part: P) {
        debug_assert!(self.get::<P>().is_none(), "one part of each type");
        self.0.push(Box::new(part));
    }

    /// The part of type `P`, when one was added.
    pub(crate) fn get<P: CallPart>(&self) -> Option<&P> {
        let part: &dyn Any = self
            .0
            .iter()
            .find(|part| (***part).type_id() == TypeId::of::<P>())?
            .as_ref();
        part.downcast_ref()
    }

    /// The part of type `P`, to change, when one was added.
    pub(crate) fn get_mut<P: CallPart>(&mut self) -> Option<&mut P> {
        let part: &mut dyn Any = self
            .0
            .iter_mut()
            .find(|part| (***part).type_id() == TypeId::of::<P>())?
            .as_mut();
        part.downcast_mut()
    }

    /// Readies every part for the call `start` in `world`, in order; the first error fails it.
    pub(crate) fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        self.0
            .iter_mut()
            .try_for_each(|part| part.begin(world, start))
    }

    /// The running call's param `name` of the first part that holds it.
    pub(crate) fn param_named(&self, role: Option<ScriptRole>, name: &str) -> Option<Dynamic> {
        self.0.iter().find_map(|part| part.param_named(role, name))
    }

    /// Applies what the call wrote to every part, in order.
    pub(crate) fn apply(&mut self, world: &mut World) {
        for part in &mut self.0 {
            part.apply(world);
        }
    }
}
