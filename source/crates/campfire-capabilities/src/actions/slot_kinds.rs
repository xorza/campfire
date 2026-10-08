use std::collections::BTreeMap;
use std::num::NonZeroU8;

use bevy_ecs::resource::Resource;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::actions::slot_kind::SlotKind;
use crate::stats::level::Level;
use crate::values::declared_name::DeclaredName;
use crate::values::rank::Rank;

/// The mode's `[[slots]]`: the kinds of slot actions sit in on a unit, in order. As a resource,
/// the match's, which the `learn` order reads; empty until the mode's books install.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct SlotKinds(pub Vec<SlotKindData>);

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

    /// Whether `kind` is one of the mode's, where an action of `ranks` ranks finds a level for
    /// each rank, when the kind gives them.
    pub(crate) fn holds(&self, kind: SlotKind, ranks: usize) -> bool {
        self.0.get(kind.index()).is_some_and(|data| {
            let levels = data.ranks.as_ref().and_then(SlotRanks::levels);
            levels.is_none_or(|levels| ranks <= levels.len())
        })
    }

    /// How many ranks an action in `kind` has.
    pub fn ranks(&self, kind: SlotKind) -> u8 {
        self.0[kind.index()]
            .ranks
            .as_ref()
            .map_or(1, |ranks| ranks.count().get())
    }

    /// The level of the `level` track that `rank`, from 1, of an action in `kind` needs; none
    /// when the kind gives its ranks no levels.
    pub(crate) fn level_of(&self, kind: SlotKind, rank: Rank) -> Option<Level> {
        let levels = self.0[kind.index()].ranks.as_ref()?.levels()?;
        Some(levels[rank.index()])
    }

    /// The rank an action in `kind` has as its unit spawns or it is granted: the first for a kind
    /// with no `ranks`, none for one whose ranks are learned.
    pub(crate) fn first_rank(&self, kind: SlotKind) -> Option<Rank> {
        self.0[kind.index()].ranks.is_none().then_some(Rank::FIRST)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_reads_its_ranks_and_the_levels_they_need() {
        let read = |text: &str| toml::from_str::<SlotKindData>(text);
        let levels = |text: &str| {
            let kind = read(&format!("name = \"basic\"\n{text}")).unwrap();
            let ranks = kind.ranks.unwrap();
            let levels = ranks
                .levels()
                .map(|levels| levels.iter().map(|level| level.get()));
            (ranks.count().get(), levels.map(Iterator::collect::<Vec<_>>))
        };
        assert_eq!(read("name = \"spell\"").unwrap().ranks, None);
        assert_eq!(levels("ranks = 3"), (3, None));
        assert_eq!(
            levels("ranks = 3\nlevels = [6, 11, 16]"),
            (3, Some(vec![6, 11, 16]))
        );
        // Two ranks may need the same level: each is at least the one before.
        assert_eq!(levels("ranks = 2\nlevels = [4, 4]"), (2, Some(vec![4, 4])));
        let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
        for refused in [
            "levels = [1, 2]",
            "levels = [1, 2, 3, 4]",
            "levels = [2, 1, 3]",
        ] {
            let text = format!("name = \"basic\"\nranks = 3\n{refused}");
            assert!(
                refusal(&text).starts_with("a slot kind's `levels` gives"),
                "{refused}"
            );
        }
        assert!(refusal("name = \"spell\"\nlevels = [1]").ends_with("needs its `ranks`"));
        assert!(
            refusal("name = \"basic\"\nranks = 1\nlevels = [0]")
                .starts_with("a level is at least 1")
        );
        // The mode's kinds hold an action in one of them that finds a level for each of its
        // ranks: in `basic`, of three levels, three ranks and no more; in `spell`, of none, any.
        let basic = read("name = \"basic\"\nranks = 3\nlevels = [6, 11, 16]").unwrap();
        let kinds = SlotKinds(vec![basic, read("name = \"spell\"").unwrap()]);
        let [basic, spell, past] = [0, 1, 2].map(SlotKind::new);
        assert!(kinds.holds(basic, 3) && !kinds.holds(basic, 4));
        assert!(kinds.holds(spell, 30) && !kinds.holds(past, 0));
    }
}
