use campfire_values::{
    Article, FilterData, FilterSyntax, NameKind, NameScope, Named, Relation, RelationSet, Tag,
    TagSet,
};

use crate::error::UnknownName;
use crate::tag_names::TagNames;

/// A filter as a match runs it: a set of relations, the tags a unit must have, and those it must
/// not, among them each tag that a filter selects only when it names it, as a delivery's, but one
/// it must have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    relations: RelationSet,
    all: TagSet,
    none: TagSet,
}

impl Filter {
    /// The filter `text` names, with its tags among the match's; the kind of the name it does
    /// not read, a filter's or a tag's.
    pub fn parse(text: &str, tags: &TagNames) -> Result<Self, NameKind> {
        let syntax = FilterSyntax::parse(text).ok_or(Self::NAMES)?;
        let terms = syntax.tags().map(|term| (term.name, term.negated));
        Self::of(syntax.relations, terms, tags).ok_or(Tag::NAMES)
    }

    /// The run-time form of `filter`, data's, with its tags among the match's; an error that
    /// names the filter for a tag the match does not have.
    pub fn resolve(filter: &FilterData, tags: &TagNames) -> Result<Self, UnknownName> {
        let terms = filter
            .tags
            .iter()
            .map(|tag| (tag.name.as_str(), tag.negated));
        Self::of(filter.relations, terms, tags).ok_or_else(|| UnknownName::new(Self::NAMES, filter))
    }

    /// The run-time form of `data`, as `resolve` gives it, or with none, the filter of enemies
    /// alone.
    pub fn resolve_or_enemies(
        data: Option<&FilterData>,
        tags: &TagNames,
    ) -> Result<Self, UnknownName> {
        data.map_or_else(
            || Ok(Self::of_relations(RelationSet::Enemies, tags)),
            |data| Self::resolve(data, tags),
        )
    }

    /// The filter of `relations` alone: every unit it selects, but those of a tag of `tags` that
    /// a filter selects only when it names it, as deliveries.
    pub const fn of_relations(relations: RelationSet, tags: &TagNames) -> Self {
        Self {
            relations,
            all: TagSet::NONE,
            none: tags.named_only(),
        }
    }

    /// The filter of `relations` and of `terms`, each a tag's name and whether it is negated,
    /// among `tags`; none for a tag the match lacks.
    fn of<'a>(
        relations: RelationSet,
        terms: impl Iterator<Item = (&'a str, bool)>,
        tags: &TagNames,
    ) -> Option<Self> {
        let mut filter = Self {
            relations,
            all: TagSet::NONE,
            none: TagSet::NONE,
        };
        for (name, negated) in terms {
            let tag = tags.named(name)?;
            if negated {
                filter.none = filter.none.with(tag);
            } else {
                filter.all = filter.all.with(tag);
            }
        }
        for tag in tags.named_only().iter() {
            if !filter.all.contains(tag) {
                filter.none = filter.none.with(tag);
            }
        }
        Some(filter)
    }

    /// Whether it selects a unit with `tags` of a team regarded with `relation`, as the unit it
    /// selects for regards it.
    pub const fn selects(self, relation: Relation, tags: TagSet) -> bool {
        self.relations.selects(relation) && tags.covers(self.all) && !tags.meets(self.none)
    }

    /// Whether a filter of every relation, as a `TagFilterData` resolves, selects a unit with
    /// `tags`.
    pub fn selects_tags(self, tags: TagSet) -> bool {
        debug_assert_eq!(self.relations, RelationSet::All, "a filter of tags alone");
        tags.covers(self.all) && !tags.meets(self.none)
    }
}

impl Named for Filter {
    const NAMES: NameKind = NameKind::new("filter", Article::A, NameScope::Match);
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_values::RelationSet;

    use crate::filter::Filter;
    use crate::match_names::MatchNames;

    impl Filter {
        /// The filter of `relations` alone, of the tags of the match `world` holds.
        pub fn of_match_relations(world: &World, relations: RelationSet) -> Self {
            Self::of_relations(relations, world.resource::<MatchNames>().tags())
        }

        /// The filter `text` names, of the tags of the match `world` holds.
        pub fn of_match(world: &World, text: &str) -> Self {
            let tags = world.resource::<MatchNames>().tags();
            Self::parse(text, tags).expect("a filter of the match's tags")
        }
    }
}
