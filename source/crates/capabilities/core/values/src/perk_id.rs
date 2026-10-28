use serde::{Deserialize, Serialize};

use crate::name_kind::{Article, NameKind, NameScope, Named};
use crate::names::NameId;

/// A perk of the mode, by its place among the mode's perks in name order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PerkId(u16);

impl NameId for PerkId {
    type Scope = ();
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> Self {
        Self(u16::try_from(index).expect("perks fit u16"))
    }

    fn index(self) -> usize {
        usize::from(self.0)
    }
}

impl Named for PerkId {
    const NAMES: NameKind = NameKind::new("perk", Article::A, NameScope::Match);
}
