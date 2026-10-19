use crate::scripts::script_args::ScriptArgs;
use crate::scripts::script_role::ScriptRole;

/// A hook the engine calls in a script, a type of the module whose system calls it, which
/// records it: the script function's name, the role whose scripts define it, the names of its
/// parameters, `ctx` first, and the types of the arguments its call passes
/// (08-script-api.md, A hook is its caller's type).
pub trait HookSpec: 'static {
    const NAME: &'static str;
    const ROLE: ScriptRole;
    const PARAMS: &'static [&'static str];
    type Args: ScriptArgs;
}

/// A hook as the registry, the load and the failures read it: its spec's name, role and
/// parameters' names. Its name tells it from every other the registry holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hook {
    pub name: &'static str,
    pub role: ScriptRole,
    pub param_names: &'static [&'static str],
}

impl Hook {
    /// The prefix of a pure hook, whose call returns a value and only reads.
    const PURE: &'static str = "calc_";

    /// The prefixes that mark a script function as a hook: one with either that is no hook of
    /// the API is a misspelling, not a helper.
    pub const PREFIXES: [&'static str; 2] = ["on_", Self::PURE];

    /// The hook `H` specifies, whose call passes one argument for each of its parameters.
    pub const fn of<H: HookSpec>() -> Self {
        const {
            assert!(
                <H::Args as ScriptArgs>::LEN == H::PARAMS.len(),
                "a hook's call passes one argument for each parameter"
            );
        }
        Self {
            name: H::NAME,
            role: H::ROLE,
            param_names: H::PARAMS,
        }
    }

    /// Whether it is a pure hook, `calc_<value>`, whose call returns a value and only reads
    /// (08-script-api.md, Hooks by name).
    pub fn is_pure(self) -> bool {
        self.name.starts_with(Self::PURE)
    }

    /// How many parameters the function takes, `ctx` first.
    pub const fn params(self) -> usize {
        self.param_names.len()
    }
}
