use std::cell::{Cell, RefCell};
use std::num::NonZeroU64;
use std::rc::Rc;

use campfire_common::Tick;
use rhai::config::hashing;
use rhai::packages::{
    ArithmeticPackage, BasicArrayPackage, BasicIteratorPackage, BasicMapPackage,
    BasicStringPackage, LogicPackage, Package,
};
use rhai::{AST, CallFnOptions, Dynamic, Engine, FuncArgs, Scope};

use crate::script_host::budget::Budget;
use crate::script_host::call_charge::CallCharge;
use crate::script_host::error::{CompileError, ScriptError};
use crate::script_host::num_api::NumApi;
use crate::script_host::property_forwards::PropertyForwards;
use crate::script_host::script_prints::{PrintKind, ScriptPrints};

#[cfg(feature = "internals")]
pub(crate) mod bench;
pub(crate) mod budget;
pub(crate) mod call_charge;
pub(crate) mod error;
mod num_api;
mod property_forwards;
pub(crate) mod script_prints;

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

/// A compiled script, by its place in the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptId(u32);

impl ScriptId {
    /// The id of the script a host compiles at place `index`, counting from 0: a load names a
    /// match's scripts by the order the match compiles them.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the index is checked to fit u32 just before"
    )]
    pub const fn nth(index: usize) -> Self {
        assert!(index <= u32::MAX as usize, "scripts fit u32");
        Self(index as u32)
    }

    /// Its place among the host's scripts, in the order it compiled them.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The Rhai engine that runs game scripts, and the scripts it compiled. Operations count the
/// same in every build, so a script over a limit fails the same way everywhere: a call runs at
/// most `per_call` operations, and at most what its `Budget` has left.
///
/// The engine has only what scripts need: integer arithmetic and logic, strings, arrays, maps,
/// iteration, and `Num`, with no floats, no clock, no `sleep`, no `eval` and no imports. `print`
/// and `debug` record their text in a debug build, for the host to log as `ScriptPrints`, and
/// nothing in a release one; either way they change no state and count the same operations. A
/// capability adds its script API through `engine_mut`.
#[derive(Debug)]
pub struct ScriptHost {
    engine: Engine,
    scripts: Vec<AST>,
    /// The running call's budget, which the progress callback shares.
    progress: Rc<CallProgress>,
    properties: PropertyForwards,
    /// What the scripts printed since the last `take_prints`.
    printed: Rc<RefCell<ScriptPrints>>,
    /// The scope each call runs in, which Rhai empties again after it, kept so its buffers stay.
    scope: Scope<'static>,
}

/// The running call's budget as the host and Rhai's progress callback share it.
#[derive(Debug)]
struct CallProgress {
    /// The most operations of one call.
    per_call: u64,
    /// What the budget has left as the call starts.
    allowed: Cell<u64>,
    /// Operations of the call, as the progress callback last saw them.
    counted: Cell<u64>,
    /// The operations the call's native work counts, which the view shares once the ctx tags
    /// the host.
    charge: RefCell<CallCharge>,
}

impl CallProgress {
    /// The call's operations: Rhai's and its native work's.
    fn spent(&self) -> u64 {
        self.counted
            .get()
            .saturating_add(self.charge.borrow().get())
    }
}

impl ScriptHost {
    /// A host whose calls each run at most `per_call` operations.
    pub fn new(per_call: NonZeroU64) -> Self {
        // The seed is set once in a process: the set of each host after the first fails, keeps
        // the seed there, and gives back the one passed, so only the read after tells which
        // seed holds.
        hashing::set_hashing_seed(Some(HASHING_SEED)).ok();
        assert_eq!(
            hashing::get_hashing_seed(),
            &Some(HASHING_SEED),
            "Rhai's hashing seed is the engine's"
        );
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
            .set_strict_variables(true)
            .set_max_operations(per_call.get())
            .disable_symbol("eval")
            .disable_symbol("try")
            .disable_symbol("catch");
        let printed = Rc::new(RefCell::new(ScriptPrints::default()));
        if cfg!(debug_assertions) {
            let (on_print, on_debug) = (Rc::clone(&printed), Rc::clone(&printed));
            engine
                .on_print(move |text| on_print.borrow_mut().push(PrintKind::Print, text))
                .on_debug(move |text, _, at| {
                    let place = |at: Option<usize>| u32::try_from(at?).ok();
                    let kind = PrintKind::Debug {
                        line: place(at.line()),
                        column: place(at.position()),
                    };
                    on_debug.borrow_mut().push(kind, text);
                });
        } else {
            engine.on_print(|_| {}).on_debug(|_, _, _| {});
        }
        let progress = Rc::new(CallProgress {
            per_call: per_call.get(),
            allowed: Cell::new(0),
            counted: Cell::new(0),
            charge: RefCell::default(),
        });
        let call = Rc::clone(&progress);
        // Past the call's limit by its native work, `true`, which reads as `CallLimit`; past the
        // budget, `()`.
        engine.on_progress(move |count| {
            call.counted.set(count);
            let spent = call.spent();
            if spent > call.per_call {
                return Some(Dynamic::TRUE);
            }
            (spent > call.allowed.get()).then_some(Dynamic::UNIT)
        });
        let mut host = Self {
            engine,
            scripts: Vec::new(),
            progress,
            properties: PropertyForwards::new(),
            printed,
            scope: Scope::new(),
        };
        NumApi::register(&mut host.engine);
        host
    }

    /// Counts `charge`, the native work of its calls, toward each call's limit and budget.
    pub(crate) fn share_charge(&self, charge: CallCharge) {
        *self.progress.charge.borrow_mut() = charge;
    }

    /// For the script API's binding and the `ctx` it tags the engine with.
    pub(crate) const fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Gives a type that reads and writes its properties through its indexer a getter and a
    /// setter of each property name the host's scripts use, before or after: `forward` registers
    /// those of one name, to call the indexer directly. Rhai tries a getter before the indexer,
    /// and builds an error message when it finds none, so a forwarded access costs one operation
    /// where it cost two, and builds no message. The type has no getter or setter of its own.
    pub fn forward_properties(&mut self, forward: impl Fn(&mut Engine, &str) + 'static) {
        self.properties.add(&mut self.engine, Box::new(forward));
    }

    pub fn compile(&mut self, source: &str) -> Result<ScriptId, CompileError> {
        let ast = self.parse(source)?;
        self.properties.learn(&mut self.engine, &ast);
        let id = ScriptId::nth(self.scripts.len());
        self.scripts.push(ast);
        Ok(id)
    }

    /// How many scripts the host compiled.
    pub const fn compiled(&self) -> usize {
        self.scripts.len()
    }

    /// The AST of `source`, compiled with the host's limits but not kept, for the package load
    /// checks to walk.
    pub fn parse(&self, source: &str) -> Result<AST, CompileError> {
        self.engine.compile(source).map_err(CompileError)
    }

    /// The functions `script` defines, each by its name and its count of parameters.
    pub fn functions(&self, script: ScriptId) -> impl Iterator<Item = (&str, usize)> {
        self.scripts[script.index()]
            .iter_functions()
            .map(|function| (function.name, function.params.len()))
    }

    /// Moves what the scripts printed since the last take into `report`, as the prints of `tick`.
    pub fn take_prints(&self, tick: Tick, report: &mut ScriptPrints) {
        report.take(tick, &mut self.printed.borrow_mut());
    }

    /// Calls the function `hook` of `script` with `args`, drawing from `budget`. A call over its
    /// limit, or past what the budget has left, fails like any other error; the operations it
    /// ran count against the budget either way. A spent budget fails a call before it runs.
    pub fn call(
        &mut self,
        budget: &mut Budget,
        script: ScriptId,
        hook: &str,
        args: impl FuncArgs,
    ) -> Result<Dynamic, ScriptError> {
        if budget.left() == 0 {
            return Err(ScriptError::TickBudget);
        }
        self.progress.allowed.set(budget.left());
        self.progress.counted.set(0);
        self.progress.charge.borrow().clear();
        let result = self.engine.call_fn_with_options::<Dynamic>(
            CallFnOptions::new().eval_ast(false),
            &mut self.scope,
            &self.scripts[script.index()],
            hook,
            args,
        );
        debug_assert!(self.scope.is_empty(), "a call rewinds its scope");
        // A call spends at most its limit, which its last native step may pass.
        budget.spend(self.progress.spent().min(self.progress.per_call));
        result.map_err(ScriptError::from_eval)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::any::TypeId;

    use crate::script_host::ScriptHost;

    /// A function the engine binds: its name, and its parameters' types.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct BoundFunction {
        pub name: String,
        pub params: Vec<TypeId>,
    }

    impl ScriptHost {
        /// Each function the engine binds, Rhai's own among them, sorted, each once.
        pub fn bound_functions(&self) -> Vec<BoundFunction> {
            let mut functions = self.engine.collect_fn_metadata(
                None,
                |info| {
                    Some(BoundFunction {
                        name: info.metadata.name.to_string(),
                        params: info.metadata.param_types.to_vec(),
                    })
                },
                true,
            );
            functions.sort_unstable();
            functions.dedup();
            functions
        }
    }
}

#[cfg(test)]
mod tests;
