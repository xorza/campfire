use bevy_ecs::resource::Resource;
use campfire_script::ScriptId;

use crate::scripts::hook::Hook;
use crate::scripts::hook_set::HookSet;

/// The hooks each script of a match defines, by `ScriptId`, in the order the match compiles its
/// scripts: what a book reads to know which hooks of a script its owner calls, with no script
/// host. The match fills it as it compiles, and the package load builds the same from what it
/// read of each script.
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
            .push(HookSet::of(Hook::ALL.into_iter().filter(defines)));
    }

    /// The hooks of `hooks` that `script` defines, none with no script.
    pub(crate) fn defines(&self, script: Option<ScriptId>, hooks: &[Hook]) -> HookSet {
        let Some(script) = script else {
            return HookSet::default();
        };
        let defined = self.hooks[script.index()];
        HookSet::of(hooks.iter().copied().filter(|&hook| defined.contains(hook)))
    }

    /// How many scripts it holds.
    pub(crate) const fn len(&self) -> usize {
        self.hooks.len()
    }
}

#[cfg(test)]
mod tests;
