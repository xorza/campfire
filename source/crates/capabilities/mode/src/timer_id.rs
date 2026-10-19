use campfire_script::name_script_arg;
use campfire_values::{Article, NameId, NameKind, NameScope, Named};
use serde::{Deserialize, Serialize};

/// A timer of the mode, by its place among the mode's timers, in the order of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimerId(u16);

impl NameId for TimerId {
    type Scope = ();
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> Self {
        Self(u16::try_from(index).expect("timers fit u16"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

// A timer a script names is one the mode declares, which only the mode's calls name.
name_script_arg!(TimerId, |ctx, name| { ctx.view().names().named(name) });

impl Named for TimerId {
    const NAMES: NameKind = NameKind::new("timer", Article::A, NameScope::Match);
}
