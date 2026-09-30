use std::cell::Cell;
use std::rc::Rc;

use rhai::config::hashing;
use rhai::packages::{
    ArithmeticPackage, BasicArrayPackage, BasicIteratorPackage, BasicMapPackage,
    BasicStringPackage, LogicPackage, Package,
};
use rhai::{AST, CallFnOptions, Dynamic, Engine, FuncArgs, Scope};

use crate::error::ScriptError;

mod num_api;

/// Rhai hashes function signatures to resolve calls. It seeds the hash per process unless set,
/// so the engine fixes the seed: every build and process resolves calls alike.
const HASHING_SEED: [u64; 4] = [0x6361_6d70_6669_7265, 0x7363_7269_7074, 1, 2];
/// Rhai's own defaults differ between debug and release builds (call depth 8 against 64), so a
/// script could pass on a release server and fail in a debug verifier: every limit is set here.
const MAX_CALL_LEVELS: usize = 32;
const MAX_EXPR_DEPTH: usize = 64;
const MAX_FUNCTION_EXPR_DEPTH: usize = 32;
const MAX_STRING_SIZE: usize = 1024;
const MAX_ARRAY_SIZE: usize = 1024;
const MAX_MAP_SIZE: usize = 256;
const MAX_VARIABLES: usize = 256;
const MAX_FUNCTIONS: usize = 256;

/// How many operations scripts may run: in one call, and in all calls of one tick together. A
/// mode's manifest sets both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptLimits {
    pub per_call: u64,
    pub per_tick: u64,
}

/// A compiled script, by its place in the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptId(u32);

/// The Rhai engine that runs game scripts, and the scripts it compiled. Operations count the
/// same in every build, so a script over a limit fails the same way everywhere.
///
/// The engine has only what scripts need: integer arithmetic and logic, strings, arrays, maps,
/// iteration, and `Num`, with no floats, no clock, no `sleep`, no `eval` and no imports.
/// `print` and `debug` write to stderr in a debug build, to debug a script, and nothing in a
/// release one, so an untrusted script cannot fill a server's log; either way they change no
/// state and count the same operations. A capability adds its script API through `engine_mut`.
#[derive(Debug)]
pub struct ScriptHost {
    engine: Engine,
    scripts: Vec<AST>,
    per_tick: u64,
    /// Operations the calls of this tick ran, before the running one.
    spent: Rc<Cell<u64>>,
    /// Operations of the running call, as the progress callback last saw them.
    counted: Rc<Cell<u64>>,
}

impl ScriptHost {
    pub fn new(limits: ScriptLimits) -> ScriptHost {
        if let Err(Some(seed)) = hashing::set_hashing_seed(Some(HASHING_SEED)) {
            assert_eq!(seed, HASHING_SEED, "Rhai's hashing seed is the engine's");
        }
        let mut engine = Engine::new_raw();
        for package in [
            ArithmeticPackage::new().as_shared_module(),
            LogicPackage::new().as_shared_module(),
            BasicStringPackage::new().as_shared_module(),
            BasicArrayPackage::new().as_shared_module(),
            BasicMapPackage::new().as_shared_module(),
            BasicIteratorPackage::new().as_shared_module(),
        ] {
            engine.register_global_module(package);
        }
        engine
            .set_max_call_levels(MAX_CALL_LEVELS)
            .set_max_expr_depths(MAX_EXPR_DEPTH, MAX_FUNCTION_EXPR_DEPTH)
            .set_max_string_size(MAX_STRING_SIZE)
            .set_max_array_size(MAX_ARRAY_SIZE)
            .set_max_map_size(MAX_MAP_SIZE)
            .set_max_variables(MAX_VARIABLES)
            .set_max_functions(MAX_FUNCTIONS)
            .set_max_modules(0)
            .set_max_operations(limits.per_call)
            .disable_symbol("eval");
        if cfg!(debug_assertions) {
            engine
                .on_print(|text| eprintln!("script: {text}"))
                .on_debug(|text, _, at| eprintln!("script {at}: {text}"));
        } else {
            engine.on_print(|_| {}).on_debug(|_, _, _| {});
        }
        let spent = Rc::new(Cell::new(0));
        let counted = Rc::new(Cell::new(0));
        let (tick_spent, call_counted) = (Rc::clone(&spent), Rc::clone(&counted));
        engine.on_progress(move |count| {
            call_counted.set(count);
            (tick_spent.get() + count > limits.per_tick).then_some(Dynamic::UNIT)
        });
        let mut host = ScriptHost {
            engine,
            scripts: Vec::new(),
            per_tick: limits.per_tick,
            spent,
            counted,
        };
        num_api::register(&mut host.engine);
        host
    }

    /// For a capability to register its script API.
    pub const fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn compile(&mut self, source: &str) -> Result<ScriptId, ScriptError> {
        let ast = self.engine.compile(source).map_err(ScriptError::Compile)?;
        let id = ScriptId(u32::try_from(self.scripts.len()).expect("scripts fit u32"));
        self.scripts.push(ast);
        Ok(id)
    }

    /// Whether `script` defines a function `name` of `params` parameters, as a hook the engine
    /// calls must be.
    pub fn defines(&self, script: ScriptId, name: &str, params: usize) -> bool {
        self.scripts[script.0 as usize]
            .iter_functions()
            .any(|function| function.name == name && function.params.len() == params)
    }

    /// Starts a tick's operation budget.
    pub fn begin_tick(&mut self) {
        self.spent.set(0);
    }

    /// Operations this tick's calls ran so far.
    pub fn spent(&self) -> u64 {
        self.spent.get()
    }

    /// Calls the function `hook` of `script` with `args`. A call over its limit, or past what the
    /// tick's budget has left, fails like any other error; the operations it ran count against
    /// the tick either way.
    pub fn call(
        &mut self,
        script: ScriptId,
        hook: &str,
        args: impl FuncArgs,
    ) -> Result<Dynamic, ScriptError> {
        if self.spent.get() >= self.per_tick {
            return Err(ScriptError::TickBudget);
        }
        self.counted.set(0);
        let result = self.engine.call_fn_with_options::<Dynamic>(
            CallFnOptions::new().eval_ast(false),
            &mut Scope::new(),
            &self.scripts[script.0 as usize],
            hook,
            args,
        );
        self.spent.set(self.spent.get() + self.counted.get());
        result.map_err(|error| ScriptError::from_eval(&error))
    }
}

#[cfg(test)]
mod tests;
