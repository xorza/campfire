use rhai::{Dynamic, FuncArgs};

use crate::scripts::script_value::{ScriptType, ScriptValue};

/// The arguments of a script's hook: a tuple of values a script holds.
pub trait ScriptArgs {
    /// The arguments as the engine passes them.
    type Engine: FuncArgs;

    /// The arguments in the engine's form, each as `ScriptValue::engine` gives it.
    fn into_engine(self) -> Self::Engine;
}

// Rhai passes a tuple of each arity as a call's arguments; so do these, up to the most a hook takes.
macro_rules! script_args {
    (@engine $ty:ident) => {
        Dynamic
    };
    ($($ty:ident $value:ident),*) => {
        impl<$($ty: ScriptType),*> ScriptArgs for ($($ty,)*) {
            type Engine = ($(script_args!(@engine $ty),)*);

            fn into_engine(self) -> Self::Engine {
                let ($($value,)*) = self;
                ($(ScriptValue::engine($value),)*)
            }
        }
    };
}

script_args!(A a);
script_args!(A a, B b);
script_args!(A a, B b, C c);
script_args!(A a, B b, C c, D d);
script_args!(A a, B b, C c, D d, E e);
