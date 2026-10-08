use std::num::NonZeroU8;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::stats::level::Level;
use crate::values::declared_name::DeclaredName;

/// A slot kind: its name, and its ranks. A kind with no `ranks` has one rank, learned from the
/// spawn; one with `ranks` starts unlearned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotKindData {
    pub name: DeclaredName,
    pub ranks: Option<SlotRanks>,
}

/// The ranks of a kind whose ranks are learned, and, when the kind gives them, the level of the
/// `level` track each rank needs: one level for each rank, each at least the one before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotRanks {
    count: NonZeroU8,
    levels: Option<Box<[Level]>>,
}

impl SlotRanks {
    /// `count` ranks that need `levels`; `None` for levels that are not one for each rank, or
    /// one below the one before.
    pub(crate) fn new(count: NonZeroU8, levels: Option<Vec<Level>>) -> Option<SlotRanks> {
        if let Some(levels) = &levels
            && (levels.len() != usize::from(count.get()) || !levels.is_sorted())
        {
            return None;
        }
        Some(SlotRanks {
            count,
            levels: levels.map(Vec::into_boxed_slice),
        })
    }

    pub(crate) const fn count(&self) -> NonZeroU8 {
        self.count
    }

    /// The level each rank needs, from rank 1, when the kind gives them.
    pub fn levels(&self) -> Option<&[Level]> {
        self.levels.as_deref()
    }
}

/// Data is untrusted, so `levels` on a kind with no `ranks`, or levels that `SlotRanks::new`
/// refuses, fail to read.
impl<'de> Deserialize<'de> for SlotKindData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<SlotKindData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: DeclaredName,
            ranks: Option<NonZeroU8>,
            levels: Option<Vec<Level>>,
        }
        let Fields {
            name,
            ranks,
            levels,
        } = Fields::deserialize(deserializer)?;
        let ranks = match (ranks, levels) {
            (None, None) => None,
            (None, Some(_)) => {
                return Err(D::Error::custom("a slot kind's `levels` needs its `ranks`"));
            }
            (Some(count), levels) => Some(SlotRanks::new(count, levels).ok_or_else(|| {
                D::Error::custom(
                    "a slot kind's `levels` gives one level for each rank, each at least the one \
                     before",
                )
            })?),
        };
        Ok(SlotKindData { name, ranks })
    }
}
