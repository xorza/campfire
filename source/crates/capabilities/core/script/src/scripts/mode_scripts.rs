use std::iter::Copied;
use std::slice;

use serde::{Deserialize, Serialize};

use crate::script_host::ScriptId;

/// The scripts a mode runs, compiled, in the order its list names them, and which of them is the
/// mode's own: the one whose calls hold the mode's state and choices, and may end the match
/// (03-game-scripting.md, Rules scripts in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeScripts {
    /// Each once, at most `ListPlace::LIMIT`.
    scripts: Box<[ScriptId]>,
    mode: ListPlace,
}

/// A script's place in its mode's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ListPlace(u8);

impl ListPlace {
    /// The most scripts a mode's list holds.
    pub const LIMIT: usize = 1 << u8::BITS;

    pub const FIRST: Self = Self(0);

    /// The place `at`; none past the limit.
    pub fn new(at: usize) -> Option<Self> {
        u8::try_from(at).ok().map(Self)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl ModeScripts {
    /// The list `scripts`, each once and at most `ListPlace::LIMIT`, of which the one at `mode` is
    /// the mode's own.
    pub fn new(scripts: impl IntoIterator<Item = ScriptId>, mode: ListPlace) -> Self {
        let scripts: Box<[ScriptId]> = scripts.into_iter().collect();
        assert!(
            scripts.len() <= ListPlace::LIMIT && mode.index() < scripts.len(),
            "a mode's list holds its own script, within the limit"
        );
        debug_assert!(
            scripts
                .iter()
                .enumerate()
                .all(|(at, script)| !scripts[..at].contains(script)),
            "a mode's list names each script once"
        );
        Self { scripts, mode }
    }

    /// The list of the mode's own script alone.
    pub fn one(script: ScriptId) -> Self {
        Self::new([script], ListPlace::FIRST)
    }

    /// The place of the mode's own script.
    pub const fn mode_place(&self) -> ListPlace {
        self.mode
    }

    /// The scripts, in the list's order.
    pub fn iter(&self) -> Copied<slice::Iter<'_, ScriptId>> {
        self.scripts.iter().copied()
    }
}

impl<'a> IntoIterator for &'a ModeScripts {
    type Item = ScriptId;
    type IntoIter = Copied<slice::Iter<'a, ScriptId>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_names_its_scripts_in_order_and_its_own_by_place() {
        let (a, b, c) = (ScriptId::nth(4), ScriptId::nth(1), ScriptId::nth(7));
        let list = ModeScripts::new([a, b, c], ListPlace::new(1).unwrap());
        assert_eq!(list.iter().collect::<Vec<_>>(), [a, b, c]);
        assert_eq!((list.mode_place().index(), list.iter().len()), (1, 3));
        let one = ModeScripts::one(a);
        assert_eq!(one.iter().collect::<Vec<_>>(), [a]);
        assert_eq!(one.mode_place(), ListPlace::FIRST);
        // A place fits a byte: 255 is the last, 256 none.
        assert_eq!(ListPlace::new(255).map(ListPlace::index), Some(255));
        assert_eq!(ListPlace::new(256), None);
    }
}
