use campfire_values::{FilterData, FilterSyntax, Relation, RelationSet, TagSet};

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked, UnknownName};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::TextArg;
use crate::scripts::script_text::ScriptText;
use crate::scripts::view::tag_names::TagNames;

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
    /// The filter `text` names, with its tags among the match's.
    pub fn parse(text: &str, tags: &TagNames) -> Result<Filter, ApiError> {
        let syntax = FilterSyntax::parse(text).ok_or(ApiError::Unknown(NameKind::Filter))?;
        let terms = syntax.tags().map(|term| (term.name, term.negated));
        Filter::of(syntax.relations, terms, tags).ok_or(ApiError::Unknown(NameKind::Tag))
    }

    /// The run-time form of `filter`, data's, with its tags among the match's; an error that
    /// names the filter for a tag the match does not have.
    pub fn resolve(filter: &FilterData, tags: &TagNames) -> Result<Filter, UnknownName> {
        let terms = filter
            .tags
            .iter()
            .map(|tag| (tag.name.as_str(), tag.negated));
        Filter::of(filter.relations, terms, tags)
            .ok_or_else(|| UnknownName::new(NameKind::Filter, filter))
    }

    /// The run-time form of `data`, as `resolve` gives it, or with none, the filter of enemies
    /// alone.
    pub fn resolve_or_enemies(
        data: Option<&FilterData>,
        tags: &TagNames,
    ) -> Result<Filter, UnknownName> {
        data.map_or_else(
            || Ok(Filter::of_relations(RelationSet::Enemies, tags)),
            |data| Filter::resolve(data, tags),
        )
    }

    /// The filter of `relations` alone: every unit it selects, but those of a tag of `tags` that
    /// a filter selects only when it names it, as deliveries.
    pub fn of_relations(relations: RelationSet, tags: &TagNames) -> Filter {
        Filter {
            relations,
            all: TagSet::default(),
            none: tags.named_only(),
        }
    }

    /// The filter of `relations` and of `terms`, each a tag's name and whether it is negated,
    /// among `tags`; none for a tag the match lacks.
    fn of<'a>(
        relations: RelationSet,
        terms: impl Iterator<Item = (&'a str, bool)>,
        tags: &TagNames,
    ) -> Option<Filter> {
        let mut filter = Filter {
            relations,
            all: TagSet::default(),
            none: TagSet::default(),
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
}

/// A filter a script gives, its tags among the match's.
impl TextArg for Filter {
    const NAMES: NameKind = NameKind::Filter;

    fn take_text(text: ScriptText, ctx: &ArgCtx<'_>) -> Checked<Filter> {
        let filter = Filter::parse(&text, ctx.get().view().names().tags());
        Ok(filter.map_err(ApiError::fail)?)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_values::RelationSet;

    use crate::scripts::view::View;
    use crate::scripts::view::filter::Filter;

    impl Filter {
        /// The filter of `relations` alone, of the tags of the match `world` holds.
        pub fn of_match_relations(world: &World, relations: RelationSet) -> Filter {
            let view = world.non_send::<View>();
            let names = view.names();
            Filter::of_relations(relations, names.tags())
        }

        /// The filter `text` names, of the tags of the match `world` holds.
        pub fn of_match(world: &World, text: &str) -> Filter {
            let view = world.non_send::<View>();
            Filter::parse(text, view.names().tags()).expect("a filter of the match's tags")
        }
    }
}
