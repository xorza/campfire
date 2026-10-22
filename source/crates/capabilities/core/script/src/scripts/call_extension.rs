use std::any::Any;
use std::fmt;

use bevy_ecs::world::World;
use campfire_values::{Named, Param, Scalar};

use crate::scripts::call_start::CallStart;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::script_role::ParamScope;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::typed_list::{TypedItem, TypedList};

/// A module's own extension of the call frame: what a call reads besides the core's, such as the
/// mode's state, and what it wrote, which commits once the call succeeds. The core names none of
/// them.
pub trait CallExtension: Any + fmt::Debug {
    /// When `commit` writes: before the call's effects apply, or after them.
    const COMMITS: Commit;

    /// Readies it for a batch of calls in `world`: what every call of the batch reads alike, as
    /// the tick's draws and the books, which no call's effects change.
    fn begin_batch(&mut self, world: &World);

    /// Readies it for the call `start`, which runs in `world`; an error fails the call.
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError>;

    /// Writes what the call changed, as `COMMITS` orders it among the call's effects.
    fn commit(&mut self, world: &mut World);
}

/// When an extension's writes commit against the call's effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Commit {
    /// Before the effects apply, so an effect reads what the call wrote.
    BeforeEffects,
    /// After the effects apply, so what the call wrote reaches a unit an effect spawns.
    AfterEffects,
}

/// An extension that holds the params of the calls of `SCOPE`, as `ctx.params` reads them.
pub trait CallParams: CallExtension {
    const SCOPE: ParamScope;

    /// The running call's param `name`, when it declares one.
    fn param(&self, name: &str) -> Option<ScriptValue>;

    /// The running call's param at place `at` among its call's, when it has one: a number of an
    /// action's effect list reads its param by the place its load resolved.
    fn param_at(&self, at: usize) -> Option<Scalar>;
}

/// An extension of any type, as the frame holds it.
trait AnyExtension: TypedItem + fmt::Debug {
    fn begin_batch(&mut self, world: &World);
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError>;
    fn commit(&mut self, world: &mut World);
    fn commits(&self) -> Commit;
}

impl<P: CallExtension> AnyExtension for P {
    fn begin_batch(&mut self, world: &World) {
        CallExtension::begin_batch(self, world);
    }

    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        CallExtension::begin(self, world, start)
    }

    fn commit(&mut self, world: &mut World) {
        CallExtension::commit(self, world);
    }

    fn commits(&self) -> Commit {
        P::COMMITS
    }
}

/// Where the frame finds the params of a scope: its extension's place, and the read of that
/// extension's own type.
#[derive(Debug, Clone, Copy)]
struct ParamsAt {
    at: usize,
    param: fn(&dyn Any, &str) -> Option<ScriptValue>,
    param_at: fn(&dyn Any, usize) -> Option<Scalar>,
}

/// The extensions of the frame, each of its own type, in the order they were added, and the
/// extension that holds each scope's params.
#[derive(Debug, Default)]
pub(crate) struct CallExtensions {
    extensions: TypedList<dyn AnyExtension>,
    params: [Option<ParamsAt>; ParamScope::ALL.len()],
}

impl CallExtensions {
    pub(crate) fn add<P: CallExtension>(&mut self, extension: P) {
        self.extensions.add(Box::new(extension));
    }

    /// Adds `extension`, the source of the params of its scope, which no extension holds yet.
    pub(crate) fn add_params<P: CallParams>(&mut self, extension: P) {
        let at = self.extensions.add(Box::new(extension));
        let scope = &mut self.params[P::SCOPE.index()];
        debug_assert!(scope.is_none(), "one extension holds a scope's params");
        *scope = Some(ParamsAt {
            at,
            param: Self::param_of::<P>,
            param_at: Self::param_at_of::<P>,
        });
    }

    /// The extension of type `P`, when one was added.
    pub(crate) fn get<P: CallExtension>(&self) -> Option<&P> {
        self.extensions.get()
    }

    /// The extension of type `P`, to change, when one was added.
    pub(crate) fn get_mut<P: CallExtension>(&mut self) -> Option<&mut P> {
        self.extensions.get_mut()
    }

    /// Readies every extension for a batch of calls in `world`.
    pub(crate) fn begin_batch(&mut self, world: &World) {
        for extension in self.extensions.iter_mut() {
            extension.begin_batch(world);
        }
    }

    /// Readies every extension for the call `start` in `world`, in order; the first error fails it.
    pub(crate) fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        self.extensions
            .iter_mut()
            .try_for_each(|extension| extension.begin(world, start))
    }

    /// Commits what the call wrote to every extension that commits at `when`, in order.
    pub(crate) fn commit(&mut self, world: &mut World, when: Commit) {
        for extension in self.extensions.iter_mut() {
            if extension.commits() == when {
                extension.commit(world);
            }
        }
    }

    /// The running call's param `name` of `scope`, else of the scope it falls back to, as a
    /// bot's params lie over the mode's; an error when no extension holds the scope's params, or
    /// none it falls back to declares such a param.
    pub(crate) fn param(&self, scope: ParamScope, name: &str) -> Result<ScriptValue, ApiError> {
        let own = self.params[scope.index()]
            .and_then(|source| (source.param)(self.extensions.at(source.at).as_any(), name));
        if let Some(value) = own {
            return Ok(value);
        }
        match scope.falls_back() {
            Some(under) => self.param(under, name),
            None if self.params[scope.index()].is_none() => Err(scope.missing()),
            None => Err(ApiError::Unknown(Param::NAMES)),
        }
    }

    /// The param `name` of `extension`, an extension of type `P`.
    fn param_of<P: CallParams>(extension: &dyn Any, name: &str) -> Option<ScriptValue> {
        let extension: &P = extension
            .downcast_ref()
            .expect("a scope's extension is of its type");
        extension.param(name)
    }

    /// The running call's param of `scope` at place `at`; none when no extension holds the scope's
    /// params, or its call has no param there.
    pub(crate) fn param_at(&self, scope: ParamScope, at: usize) -> Option<Scalar> {
        let source = self.params[scope.index()]?;
        (source.param_at)(self.extensions.at(source.at).as_any(), at)
    }

    /// The param at place `at` of `extension`, an extension of type `P`.
    fn param_at_of<P: CallParams>(extension: &dyn Any, at: usize) -> Option<Scalar> {
        let extension: &P = extension
            .downcast_ref()
            .expect("a scope's extension is of its type");
        extension.param_at(at)
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;

    use super::*;

    /// An extension with the mode's params: `waves`, and no other.
    #[derive(Debug)]
    struct ModeParams;

    impl CallExtension for ModeParams {
        const COMMITS: Commit = Commit::AfterEffects;

        fn begin_batch(&mut self, _: &World) {}

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

        fn param_at(&self, at: usize) -> Option<Scalar> {
            (at == 0).then(|| Scalar::from(Num::int(3)))
        }
    }

    #[test]
    fn a_call_reads_its_scopes_params_from_the_one_part_that_holds_them() {
        let mut extensions = CallExtensions::default();
        // With no extension of a scope, a mode's call finds no mode, an ability's no param.
        let read = |extensions: &CallExtensions, scope, name| {
            extensions.param(scope, name).map(|value| value.as_int())
        };
        assert_eq!(
            read(&extensions, ParamScope::Mode, "waves"),
            Err(ApiError::NoMode)
        );
        let unknown = Err(ApiError::Unknown(Param::NAMES));
        assert_eq!(read(&extensions, ParamScope::Action, "waves"), unknown);
        assert_eq!(extensions.param_at(ParamScope::Mode, 0), None);
        extensions.add(Plain);
        extensions.add_params(ModeParams);
        assert_eq!(read(&extensions, ParamScope::Mode, "waves"), Ok(Some(3)));
        assert_eq!(read(&extensions, ParamScope::Mode, "wave"), unknown);
        assert_eq!(read(&extensions, ParamScope::Action, "waves"), unknown);
        // By place too: the scope's extension gives its param at 0, none past it, and no other
        // scope finds it.
        assert_eq!(
            extensions.param_at(ParamScope::Mode, 0),
            Some(Scalar::from(Num::int(3)))
        );
        assert_eq!(extensions.param_at(ParamScope::Mode, 1), None);
        assert_eq!(extensions.param_at(ParamScope::Action, 0), None);
        assert!(extensions.get::<ModeParams>().is_some());
    }

    /// An extension with no params, added first, so the params' extension is not the first.
    #[derive(Debug)]
    struct Plain;

    impl CallExtension for Plain {
        const COMMITS: Commit = Commit::BeforeEffects;

        fn begin_batch(&mut self, _: &World) {}

        fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
            Ok(())
        }

        fn commit(&mut self, _: &mut World) {}
    }
}
