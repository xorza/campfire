use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_script::{Hook, HookSet, ScriptId, ScriptRole, View};
use campfire_sim::Capability;

/// The hooks each script of a match defines, by `ScriptId`, in the order the match compiles its
/// scripts: what a book reads to know which hooks of a script its owner calls, with no script
/// host. The package load builds it from what it read of each script, and a match installs a
/// copy.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptBook {
    hooks: Vec<HookSet>,
}

impl ScriptBook {
    /// Adds the next script, which defines `functions`, each by its name and its count of
    /// parameters.
    pub fn push<'a>(&mut self, functions: impl IntoIterator<Item = (&'a str, usize)>) {
        let functions: Vec<(&str, usize)> = functions.into_iter().collect();
        let defines = |hook: &Hook| functions.contains(&(hook.name(), hook.params()));
        self.hooks
            .push(Hook::ALL.into_iter().filter(defines).collect());
    }

    /// The hooks of `role` that `script` defines, none with no script.
    pub fn defines(&self, script: Option<ScriptId>, role: ScriptRole) -> HookSet {
        let Some(script) = script else {
            return HookSet::default();
        };
        let defined = self.hooks[script.index()];
        let hooks = Hook::ALL.into_iter().filter(|hook| hook.role() == role);
        hooks.filter(|&hook| defined.contains(hook)).collect()
    }
}

impl Book for ScriptBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = true;

    fn share(&self, _: &View) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::units::script_book::ScriptBook;

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
