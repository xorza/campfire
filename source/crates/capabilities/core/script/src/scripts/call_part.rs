use std::any::Any;
use std::fmt;

use bevy_ecs::world::World;

use crate::scripts::call_start::CallStart;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_role::ParamScope;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::type_slots::{Slot, TypeSlots};

/// A module's own part of the call frame: what a call reads besides the core's, such as the
/// mode's state, and what it wrote, which commits once the call succeeds. The core names none of
/// them.
pub trait CallPart: Any + fmt::Debug {
    /// When `commit` writes: before the call's effects apply, or after them.
    const COMMITS: Commit;

    /// Readies it for the call `start`, which runs in `world`; an error fails the call.
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError>;

    /// Writes what the call changed, as `COMMITS` orders it among the call's effects.
    fn commit(&mut self, world: &mut World);
}

/// When a part's writes commit against the call's effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Commit {
    /// Before the effects apply, so an effect reads what the call wrote.
    BeforeEffects,
    /// After the effects apply, so what the call wrote reaches a unit an effect spawns.
    AfterEffects,
}

/// A part that holds the params of the calls of `SCOPE`, as `ctx.p` reads them.
pub trait CallParams: CallPart {
    const SCOPE: ParamScope;

    /// The running call's param `name`, when it declares one.
    fn param(&self, name: &str) -> Option<ScriptValue>;
}

/// A part of any type, as the frame holds it.
trait AnyPart: Slot + fmt::Debug {
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError>;
    fn commit(&mut self, world: &mut World);
    fn commits(&self) -> Commit;
}

impl<P: CallPart> AnyPart for P {
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        CallPart::begin(self, world, start)
    }

    fn commit(&mut self, world: &mut World) {
        CallPart::commit(self, world);
    }

    fn commits(&self) -> Commit {
        P::COMMITS
    }
}

/// Where the frame finds the params of a scope: its part's place, and the read of that part's
/// own type.
#[derive(Debug, Clone, Copy)]
struct ParamsAt {
    at: usize,
    param: fn(&dyn Any, &str) -> Option<ScriptValue>,
}

/// The parts of the frame, each of its own type, in the order they were added, and the part
/// that holds each scope's params.
#[derive(Debug, Default)]
pub(crate) struct CallParts {
    parts: TypeSlots<dyn AnyPart>,
    params: [Option<ParamsAt>; ParamScope::ALL.len()],
}

impl CallParts {
    pub(crate) fn add<P: CallPart>(&mut self, part: P) {
        self.parts.add(Box::new(part));
    }

    /// Adds `part`, the source of the params of its scope, which no part holds yet.
    pub(crate) fn add_params<P: CallParams>(&mut self, part: P) {
        let at = self.parts.add(Box::new(part));
        let scope = &mut self.params[P::SCOPE.index()];
        debug_assert!(scope.is_none(), "one part holds a scope's params");
        *scope = Some(ParamsAt {
            at,
            param: CallParts::param_of::<P>,
        });
    }

    /// The part of type `P`, when one was added.
    pub(crate) fn get<P: CallPart>(&self) -> Option<&P> {
        self.parts.get()
    }

    /// The part of type `P`, to change, when one was added.
    pub(crate) fn get_mut<P: CallPart>(&mut self) -> Option<&mut P> {
        self.parts.get_mut()
    }

    /// Readies every part for the call `start` in `world`, in order; the first error fails it.
    pub(crate) fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        self.parts
            .iter_mut()
            .try_for_each(|part| part.begin(world, start))
    }

    /// Commits what the call wrote to every part that commits at `when`, in order.
    pub(crate) fn commit(&mut self, world: &mut World, when: Commit) {
        for part in self.parts.iter_mut() {
            if part.commits() == when {
                part.commit(world);
            }
        }
    }

    /// The running call's param `name` of `scope`; an error when no part holds the scope's
    /// params, or its part declares no such param.
    pub(crate) fn param(&self, scope: ParamScope, name: &str) -> Result<ScriptValue, ApiError> {
        let source = self.params[scope.index()].ok_or_else(|| scope.missing())?;
        let value = (source.param)(self.parts.at(source.at).as_any(), name);
        value.ok_or(ApiError::Unknown(NameKind::Param))
    }

    /// The param `name` of `part`, a part of type `P`.
    fn param_of<P: CallParams>(part: &dyn Any, name: &str) -> Option<ScriptValue> {
        let part: &P = part.downcast_ref().expect("a scope's part is of its type");
        part.param(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A part with the mode's params: `waves`, and no other.
    #[derive(Debug)]
    struct ModeParams;

    impl CallPart for ModeParams {
        const COMMITS: Commit = Commit::AfterEffects;

        fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
            Ok(())
        }

        fn commit(&mut self, _: &mut World) {}
    }

    impl CallParams for ModeParams {
        const SCOPE: ParamScope = ParamScope::Mode;

        fn param(&self, name: &str) -> Option<ScriptValue> {
            (name == "waves").then(|| ScriptValue::of(3_i64))
        }
    }

    #[test]
    fn a_call_reads_its_scopes_params_from_the_one_part_that_holds_them() {
        let mut parts = CallParts::default();
        // With no part of a scope, a mode's call finds no mode, an ability's no param.
        let read =
            |parts: &CallParts, scope, name| parts.param(scope, name).map(|value| value.as_int());
        assert_eq!(
            read(&parts, ParamScope::Mode, "waves"),
            Err(ApiError::NoMode)
        );
        let unknown = Err(ApiError::Unknown(NameKind::Param));
        assert_eq!(read(&parts, ParamScope::Action, "waves"), unknown);
        parts.add(Plain);
        parts.add_params(ModeParams);
        assert_eq!(read(&parts, ParamScope::Mode, "waves"), Ok(Some(3)));
        assert_eq!(read(&parts, ParamScope::Mode, "wave"), unknown);
        assert_eq!(read(&parts, ParamScope::Action, "waves"), unknown);
        assert!(parts.get::<ModeParams>().is_some());
    }

    /// A part with no params, added first, so the params' part is not the first.
    #[derive(Debug)]
    struct Plain;

    impl CallPart for Plain {
        const COMMITS: Commit = Commit::BeforeEffects;

        fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
            Ok(())
        }

        fn commit(&mut self, _: &mut World) {}
    }
}
