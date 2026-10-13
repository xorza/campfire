use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked, UnknownName};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::TextArg;
use crate::scripts::script_text::ScriptText;
use crate::units::tag_set::TagSet;
use crate::units::unit_types::UnitTypes;
use crate::values::filter_data::{FilterData, FilterSyntax};
use crate::values::relation::Relation;
use crate::values::relation_set::RelationSet;

/// A filter as a match runs it: a set of relations, the tags a unit must have, and those it must
/// not, among them each tag that a filter selects only when it names it, as a delivery's, but one
/// it must have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Filter {
    relations: RelationSet,
    all: TagSet,
    none: TagSet,
}

impl Filter {
    /// The filter `text` names, with its tags among the match's.
    pub(crate) fn parse(text: &str, types: &UnitTypes) -> Result<Filter, ApiError> {
        let syntax = FilterSyntax::parse(text).ok_or(ApiError::Unknown(NameKind::Filter))?;
        let terms = syntax.tags().map(|term| (term.name, term.negated));
        Filter::of(syntax.relations, terms, types).ok_or(ApiError::Unknown(NameKind::Tag))
    }

    /// The run-time form of `filter`, data's, with its tags among the match's; an error that
    /// names the filter for a tag the match does not have.
    pub(crate) fn resolve(filter: &FilterData, types: &UnitTypes) -> Result<Filter, UnknownName> {
        let terms = filter
            .tags
            .iter()
            .map(|tag| (tag.name.as_str(), tag.negated));
        Filter::of(filter.relations, terms, types)
            .ok_or_else(|| UnknownName::new(NameKind::Filter, filter))
    }

    /// The run-time form of `data`, as `resolve` gives it, or with none, the filter of enemies
    /// alone.
    pub(crate) fn resolve_or_enemies(
        data: Option<&FilterData>,
        types: &UnitTypes,
    ) -> Result<Filter, UnknownName> {
        data.map_or_else(
            || Ok(Filter::of_relations(RelationSet::Enemies, types)),
            |data| Filter::resolve(data, types),
        )
    }

    /// The filter of `relations` alone: every unit it selects, but those of a tag of `types` that
    /// a filter selects only when it names it, as deliveries.
    pub(crate) fn of_relations(relations: RelationSet, types: &UnitTypes) -> Filter {
        Filter {
            relations,
            all: TagSet::default(),
            none: types.named_only(),
        }
    }

    /// The filter of `relations` and of `terms`, each a tag's name and whether it is negated,
    /// among the tags of `types`; none for a tag `types` lacks.
    fn of<'a>(
        relations: RelationSet,
        terms: impl Iterator<Item = (&'a str, bool)>,
        types: &UnitTypes,
    ) -> Option<Filter> {
        let mut filter = Filter {
            relations,
            all: TagSet::default(),
            none: TagSet::default(),
        };
        for (name, negated) in terms {
            let tag = types.tags().named(name)?;
            if negated {
                filter.none = filter.none.with(tag);
            } else {
                filter.all = filter.all.with(tag);
            }
        }
        for tag in types.named_only().iter() {
            if !filter.all.contains(tag) {
                filter.none = filter.none.with(tag);
            }
        }
        Some(filter)
    }

    /// Whether it selects a unit with `tags` of a team regarded with `relation`, as the unit it
    /// selects for regards it.
    pub(crate) const fn selects(self, relation: Relation, tags: TagSet) -> bool {
        self.relations.selects(relation) && tags.covers(self.all) && !tags.meets(self.none)
    }
}

/// A filter a script gives, its tags among the match's.
impl TextArg for Filter {
    const NAMES: NameKind = NameKind::Filter;

    fn take_text(text: ScriptText, ctx: &ArgCtx<'_>) -> Checked<Filter> {
        let filter = Filter::parse(&text, ctx.get().view().names().types());
        Ok(filter.map_err(ApiError::fail)?)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::release::Release;
    use crate::units::filter::Filter;
    use crate::units::unit_types::UnitTypes;
    use crate::values::relation_set::RelationSet;

    impl Filter {
        /// The filter of `relations` alone, of a match of the release's engine tags.
        pub(crate) fn of_release_relations(relations: RelationSet) -> Filter {
            Filter::of_relations(relations, &UnitTypes::new(Release::engine_tags()))
        }
    }
}
