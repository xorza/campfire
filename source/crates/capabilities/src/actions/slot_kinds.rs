use std::collections::BTreeMap;
use std::num::NonZeroU8;

use serde::Deserialize;

use crate::actions::slot_kind::SlotKind;
use crate::values::declared_name::DeclaredName;

/// The mode's `[[slots]]`: the kinds of slot actions sit in on a unit, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct SlotKinds(pub Vec<SlotKindData>);

/// A slot kind: its name, its ranks, and the level each rank needs. A kind with no `ranks` has
/// one rank, learned from the spawn; one with `ranks` starts unlearned.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotKindData {
    pub name: DeclaredName,
    pub ranks: Option<NonZeroU8>,
    #[serde(default)]
    pub levels: Vec<u32>,
}

impl SlotKinds {
    /// The ranks of each action that the unit types of `slots` place, each type's actions by
    /// slot kind: those of the kind it sits in. An error names an action they place in kinds of
    /// other ranks. A kind it does not hold places nothing.
    pub fn slotted_ranks<'u>(
        &self,
        slots: impl IntoIterator<Item = &'u BTreeMap<DeclaredName, Vec<DeclaredName>>>,
    ) -> Result<BTreeMap<&'u str, u8>, &'u DeclaredName> {
        let mut ranks = BTreeMap::new();
        for slots in slots {
            for (kind, ids) in slots {
                let Some(kind) = self.named(kind.as_str()) else {
                    continue;
                };
                for id in ids {
                    let held = *ranks.entry(id.as_str()).or_insert(self.ranks(kind));
                    if held != self.ranks(kind) {
                        return Err(id);
                    }
                }
            }
        }
        Ok(ranks)
    }

    /// The kind `name`, if the mode declares it.
    pub fn named(&self, name: &str) -> Option<SlotKind> {
        let at = self.0.iter().position(|kind| kind.name.as_str() == name)?;
        Some(SlotKind::new(u8::try_from(at).ok()?))
    }

    /// How many ranks an action in `kind` has.
    pub fn ranks(&self, kind: SlotKind) -> u8 {
        self.0[kind.index()].ranks.map_or(1, NonZeroU8::get)
    }

    /// The rank an action in `kind` has as its unit spawns or it is granted: 1 for a kind with
    /// no `ranks`, 0 for one whose ranks are learned.
    pub(crate) fn first_rank(&self, kind: SlotKind) -> u8 {
        u8::from(self.0[kind.index()].ranks.is_none())
    }
}
