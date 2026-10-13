use std::fmt;
use std::rc::Rc;

use crate::scripts::script_batch::ScriptBatch;

/// A function a capability calls in a script batch, which another part of the match gives it,
/// such as the mode's `calc_damage`: given an `Arg`, it gives an `Out`. Package data, not state;
/// each pair of types is a resource of its own. A clone shares the function, as a pass that calls
/// it while it writes the world holds it.
pub struct ScriptFn<Arg, Out>(Rc<dyn Fn(&mut ScriptBatch<'_>, Arg) -> Out>);

impl<Arg, Out> ScriptFn<Arg, Out> {
    pub fn new(call: impl Fn(&mut ScriptBatch<'_>, Arg) -> Out + 'static) -> Self {
        ScriptFn(Rc::new(call))
    }

    /// What it gives for `arg`, in `batch`.
    pub fn call(&self, batch: &mut ScriptBatch<'_>, arg: Arg) -> Out {
        (self.0)(batch, arg)
    }
}

impl<Arg, Out> Clone for ScriptFn<Arg, Out> {
    fn clone(&self) -> Self {
        ScriptFn(Rc::clone(&self.0))
    }
}

impl<Arg, Out> fmt::Debug for ScriptFn<Arg, Out> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScriptFn")
    }
}
