use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError};
use campfire_script::{Hook, HookSet, MatchNames, ScriptId, ScriptRole};

/// The hooks each script of a match defines, by `ScriptId`, in the order the match compiles its
/// scripts: what a book reads to know which hooks of a script its owner calls, with no script
/// host. The package load builds it from what it read of each script, and a match installs a
/// copy.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct ScriptBook {
    /// The hooks of the script API, which a script may define.
    api: Arc<[Hook]>,
    hooks: Vec<HookSet>,
}

impl ScriptBook {
    /// A book of no script, whose scripts may define `api`, the hooks of the script API.
    pub fn new(api: impl IntoIterator<Item = Hook>) -> Self {
        Self {
            api: api.into_iter().collect(),
            hooks: Vec::new(),
        }
    }

    /// Adds the next script, which defines `functions`, each by its name and its count of
    /// parameters: the hooks of the script API among them.
    pub fn push<'a>(&mut self, functions: impl IntoIterator<Item = (&'a str, usize)>) {
        let functions: Vec<(&str, usize)> = functions.into_iter().collect();
        let defines = |hook: &&Hook| functions.contains(&(hook.name, hook.params()));
        let defined = self.api.iter().filter(defines).copied().collect();
        self.hooks.push(defined);
    }

    /// The hooks of `role` that `script` defines, none with no script.
    pub fn defines(&self, script: Option<ScriptId>, role: ScriptRole) -> HookSet {
        script.map_or_else(HookSet::default, |script| {
            self.hooks[script.index()].of_role(role)
        })
    }

    /// The hooks of `role` one of `scripts` defines.
    pub fn defines_any(
        &self,
        scripts: impl IntoIterator<Item = ScriptId>,
        role: ScriptRole,
    ) -> HookSet {
        let held = scripts
            .into_iter()
            .flat_map(|script| &self.hooks[script.index()]);
        held.filter(|hook| hook.role == role).collect()
    }
}

impl ScriptBook {
    /// The hooks of the match's scripts, which the load gives as it compiles them.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let given = cx.given::<Self>();
        Ok(given.expect("the load gives the match's scripts").clone())
    }
}

impl Book for ScriptBook {
    const SCRIPTS_ONLY: bool = true;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::script_book::ScriptBook;

    impl ScriptBook {
        /// How many scripts it holds.
        pub const fn len(&self) -> usize {
            self.hooks.len()
        }

        pub const fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }
}

#[cfg(test)]
mod tests;
