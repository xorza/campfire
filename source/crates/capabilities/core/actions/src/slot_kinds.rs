use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use campfire_units::NeededLevel;
use campfire_values::{NameId, Names, NamesError, Rank};
use serde::Deserialize;

use crate::error::SlotKindsError;
use crate::slot_kind::SlotKind;
use crate::slot_kind_data::{SlotKindData, SlotRanks};

/// The mode's `[[slots]]`: the kinds of slot actions sit in on a unit, in order, and their names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Vec<SlotKindData>")]
pub struct SlotKinds {
    kinds: Vec<SlotKindData>,
    names: Arc<Names<SlotKind>>,
}

/// The kinds of `kinds`, as `SlotKinds::new` takes them: the form `[[slots]]` reads into.
impl TryFrom<Vec<SlotKindData>> for SlotKinds {
    type Error = SlotKindsError;

    fn try_from(kinds: Vec<SlotKindData>) -> Result<Self, SlotKindsError> {
        Self::new(kinds)
    }
}

impl SlotKinds {
    /// The kinds of `kinds`, in order, each named by its id; an error for a kind named twice, or
    /// more kinds than an id counts.
    pub fn new(kinds: impl IntoIterator<Item = SlotKindData>) -> Result<Self, SlotKindsError> {
        Self::of(kinds.into_iter().collect())
    }

    fn of(kinds: Vec<SlotKindData>) -> Result<Self, SlotKindsError> {
        let mut names = Names::default();
        for kind in &kinds {
            names
                .try_add(kind.name.as_str())
                .map_err(|error| match error {
                    NamesError::Full => SlotKindsError::TooMany,
                    NamesError::Repeated => SlotKindsError::Repeated(kind.name.clone()),
                })?;
        }
        Ok(Self {
            kinds,
            names: Arc::new(names),
        })
    }

    /// Every kind, in order.
    pub fn kinds(&self) -> &[SlotKindData] {
        &self.kinds
    }

    /// The kinds' names.
    pub const fn names(&self) -> &Arc<Names<SlotKind>> {
        &self.names
    }

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
                let Some(kind) = self.names.named(kind.as_str()) else {
                    continue;
                };
                for id in ids {
                    let held = *ranks.entry(id.as_str()).or_insert_with(|| self.ranks(kind));
                    if held != self.ranks(kind) {
                        return Err(id);
                    }
                }
            }
        }
        Ok(ranks)
    }

    /// Whether `kind` is one of the mode's, where an action of `ranks` ranks finds a level for
    /// each rank, when the kind gives them.
    pub(crate) fn holds(&self, kind: SlotKind, ranks: usize) -> bool {
        self.kinds.get(kind.index()).is_some_and(|data| {
            let levels = data.ranks.as_ref().and_then(SlotRanks::levels);
            levels.is_none_or(|levels| ranks <= levels.len())
        })
    }

    /// How many ranks an action in `kind` has.
    pub fn ranks(&self, kind: SlotKind) -> u8 {
        self.kinds[kind.index()]
            .ranks
            .as_ref()
            .map_or(1, |ranks| ranks.count().get())
    }

    /// The level of the `level` track that `rank`, from 1, of an action in `kind` needs; none
    /// when the kind gives its ranks no levels.
    pub fn level_of(&self, kind: SlotKind, rank: Rank) -> Option<NeededLevel> {
        let levels = self.kinds[kind.index()].ranks.as_ref()?.levels()?;
        Some(levels[rank.index()])
    }

    /// The rank an action in `kind` has as its unit spawns or it is granted: the first for a kind
    /// with no `ranks`, none for one whose ranks are learned.
    pub fn first_rank(&self, kind: SlotKind) -> Option<Rank> {
        self.kinds[kind.index()]
            .ranks
            .is_none()
            .then_some(Rank::FIRST)
    }
}

impl Section for SlotKinds {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "slots";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    /// A mode's data, of its slot kinds alone.
    #[derive(Debug, Deserialize)]
    struct Mode {
        #[expect(dead_code, reason = "the test reads only whether it decodes")]
        slots: SlotKinds,
    }

    #[test]
    fn a_kind_reads_its_ranks_and_the_levels_they_need() {
        let read = |text: &str| Toml::parse::<SlotKindData>(text);
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
        let kinds = SlotKinds::new([basic, read("name = \"spell\"").unwrap()]).unwrap();
        let [basic, spell, past] = [0, 1, 2].map(SlotKind::new);
        assert!(kinds.holds(basic, 3) && !kinds.holds(basic, 4));
        assert!(kinds.holds(spell, 30) && !kinds.holds(past, 0));
        assert_eq!(kinds.names().named("spell"), Some(spell));
        // A list that names a kind twice, or more kinds than a `u8` id counts, is refused, so no
        // id stands for another's place; 256 kinds are the most.
        let kind = |name: &str| read(&format!("name = \"{name}\"")).unwrap();
        let twice = SlotKinds::new([kind("basic"), kind("spell"), kind("basic")]);
        let basic = DeclaredName::new("basic").unwrap();
        assert_eq!(twice, Err(SlotKindsError::Repeated(basic)));
        let many = |count: usize| SlotKinds::new((0..count).map(|at| kind(&format!("k{at}"))));
        assert_eq!(
            many(256).unwrap().names().named("k255"),
            Some(SlotKind::new(255))
        );
        assert_eq!(many(257), Err(SlotKindsError::TooMany));
        let repeated = "[[slots]]\nname = \"basic\"\n[[slots]]\nname = \"basic\"";
        let refused = Toml::parse::<Mode>(repeated).unwrap_err();
        assert_eq!(refused.message(), "the slot kind `basic` twice");
    }
}
