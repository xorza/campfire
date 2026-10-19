use crate::script_host::ScriptId;
use crate::scripts::applies::Applies;
use crate::scripts::script_facts::ScriptFacts;

/// What each of a match's scripts uses of its packages' data, in the order a match compiles them,
/// as the load parses them: what the rules of the data its scripts name read.
#[derive(Debug, Default)]
pub struct ScriptUses(Vec<ScriptUse>);

/// What one script uses: the modifiers it applies, and the params its `ctx.params` reads.
#[derive(Debug, Default)]
pub struct ScriptUse {
    pub applied: Vec<AppliedModifier>,
    pub params: Vec<String>,
}

/// A modifier a script applies, by the literal name it gives, and how.
#[derive(Debug, Clone)]
pub struct AppliedModifier {
    pub name: String,
    pub applies: Applies,
}

impl ScriptUses {
    /// Adds what the next script of the match uses, of its `facts`.
    pub fn push(&mut self, facts: &ScriptFacts) {
        let applied = facts.applied().map(|(name, applies)| AppliedModifier {
            name: name.to_owned(),
            applies,
        });
        self.0.push(ScriptUse {
            applied: applied.collect(),
            params: facts.params.clone(),
        });
    }

    /// What `script`, a script of the match, uses.
    pub fn of(&self, script: ScriptId) -> &ScriptUse {
        &self.0[script.index()]
    }
}
